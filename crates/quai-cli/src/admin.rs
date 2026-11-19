//! setup, sync, tokens, global settings, refresh, upgrade.

use crate::ctx::Ctx;
use crate::output::Output;
use crate::sources::{dnf_makecache, run};
use anyhow::{Context, Result};
use quai_core::config::{self, DEFAULT_LISTEN, SYSTEM_GROUP};
use quai_core::error::{not_found, usage};
use quai_core::fsutil::{chown_root_group, write_atomic};
use quai_core::sign::Signer;
use quai_core::{repofile, system};
use quai_types::{ActionResult, Settings, TokenInfo};
use std::io::Read;
use std::process::Command;

fn ok(message: impl Into<String>, details: Vec<String>) -> Output {
    Output::Action(ActionResult { ok: true, message: message.into(), source: None, details, reboot_required: false })
}

/// Idempotent first-run setup: directories, default config, signing key, socket.
pub fn ensure_setup(ctx: &Ctx, systemd: bool) -> Result<Vec<String>> {
    let p = &ctx.paths;
    let mut details = Vec::new();
    std::fs::create_dir_all(p.sources_dir()).with_context(|| format!("creating {}", p.sources_dir().display()))?;
    config::ensure_private_dir(p)?;
    if !p.global_config().exists() {
        config::save_global(p, &ctx.global)?;
        details.push(format!("created {}", p.global_config().display()));
    }

    let mut key_changed = false;
    if ctx.global.defaults.sign_metadata {
        if !p.secret_key().exists() {
            let uid = format!("quai local repository key ({}) <root@localhost>", system::hostname());
            let g = Signer::generate(&uid)?;
            write_atomic(&p.secret_key(), g.secret_armored.as_bytes(), 0o640)?;
            chown_root_group(&p.secret_key(), SYSTEM_GROUP)?;
            write_atomic(&p.public_key(), g.public_armored.as_bytes(), 0o644)?;
            details.push(format!("generated the metadata signing key {}", g.signer.fingerprint()));
            key_changed = true;
        } else if !p.public_key().exists() {
            let signer = Signer::load(&p.secret_key())?;
            write_atomic(&p.public_key(), signer.public_armored()?.as_bytes(), 0o644)?;
            details.push(format!("restored {}", p.public_key().display()));
        }
    }

    if systemd && system::has_systemd() {
        let dropin = p.socket_dropin();
        let listen = &ctx.global.daemon.listen;
        let mut reload = false;
        if listen != DEFAULT_LISTEN {
            let body = format!("# Written by `quai setup` from quai.toml\n[Socket]\nListenStream=\nListenStream={listen}\n");
            if std::fs::read_to_string(&dropin).ok().as_deref() != Some(body.as_str()) {
                write_atomic(&dropin, body.as_bytes(), 0o644)?;
                reload = true;
            }
        } else if dropin.exists() {
            std::fs::remove_file(&dropin)?;
            reload = true;
        }
        if reload {
            system::systemctl(&["daemon-reload"])?;
            system::systemctl(&["restart", "quaid.socket"]).ok();
            details.push(format!("quaid.socket now listens on {listen}"));
        }
        let enabled = system::systemctl(&["is-enabled", "quaid.socket"]).map(|s| s.trim() == "enabled").unwrap_or(false);
        if !enabled {
            system::systemctl(&["enable", "--now", "quaid.socket"]).context("enabling quaid.socket")?;
            details.push("enabled quaid.socket".into());
        } else if system::systemctl(&["is-active", "quaid.socket"]).is_err() {
            system::systemctl(&["start", "quaid.socket"])?;
            details.push("started quaid.socket".into());
        }
        if key_changed {
            // quaid loads the key at start-up.
            system::systemctl(&["try-restart", "quaid.service"]).ok();
        }
    }
    Ok(details)
}

pub fn setup(ctx: &Ctx, systemd: bool) -> Result<Output> {
    ctx.require_root("quai setup")?;
    let mut details = ensure_setup(ctx, systemd)?;
    let (sources, bad) = config::load_sources(&ctx.paths);
    let report = repofile::sync(&ctx.paths, &ctx.global, &sources)?;
    if !report.written.is_empty() {
        details.push(format!("updated repo files: {}", report.written.join(", ")));
    }
    for (id, e) in bad {
        details.push(format!("unreadable source '{id}': {e:#}"));
    }
    if details.is_empty() {
        details.push("nothing to do, quai was already set up".into());
    }
    Ok(ok("quai is set up", details))
}

pub fn sync(ctx: &Ctx) -> Result<Output> {
    ctx.require_root("quai sync")?;
    let (sources, bad) = config::load_sources(&ctx.paths);
    let report = repofile::sync(&ctx.paths, &ctx.global, &sources)?;
    let mut details = Vec::new();
    for id in &report.written {
        details.push(format!("wrote {}", ctx.paths.repo_file(id).display()));
    }
    for id in &report.removed {
        details.push(format!("removed {}", ctx.paths.repo_file(id).display()));
    }
    let failed = !bad.is_empty();
    for (id, e) in bad {
        details.push(format!("✗ {id}: {e:#}"));
    }
    Ok(Output::Action(ActionResult {
        ok: !failed,
        message: format!("{} repo file(s) written, {} removed", report.written.len(), report.removed.len()),
        source: None,
        details,
        reboot_required: false,
    }))
}

fn validate_host(host: &str) -> Result<String> {
    let h = host.trim().to_ascii_lowercase();
    if h.is_empty() || !h.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':')) {
        return Err(usage(format!("invalid host '{host}'")));
    }
    Ok(h)
}

fn restart_quaid() {
    if system::has_systemd() {
        system::systemctl(&["try-restart", "quaid.service"]).ok();
    }
}

pub fn token_set(ctx: &Ctx, host: &str) -> Result<Output> {
    ctx.require_root("quai token set")?;
    let host = validate_host(host)?;
    if ctx.interactive() {
        eprintln!("Paste the token for {host}, then press Enter and Ctrl-D:");
    }
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let token = input.lines().next().unwrap_or("").trim().to_string();
    if token.is_empty() || token.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(usage("no valid token on stdin"));
    }
    let mut tokens = config::load_tokens(&ctx.paths);
    tokens.tokens.insert(host.clone(), token);
    config::save_tokens(&ctx.paths, &tokens)?;
    restart_quaid();
    Ok(ok(format!("Token saved for {host}"), vec![format!("stored in {}", ctx.paths.tokens_file().display())]))
}

pub fn token_clear(ctx: &Ctx, host: &str) -> Result<Output> {
    ctx.require_root("quai token clear")?;
    let host = validate_host(host)?;
    let mut tokens = config::load_tokens(&ctx.paths);
    if tokens.tokens.remove(&host).is_none() {
        return Err(not_found(format!("no token for {host}")));
    }
    config::save_tokens(&ctx.paths, &tokens)?;
    restart_quaid();
    Ok(ok(format!("Token removed for {host}"), vec![]))
}

pub fn settings(ctx: &Ctx) -> Result<Output> {
    let g = &ctx.global;
    let stored = config::load_tokens(&ctx.paths);
    let mut hosts: Vec<String> = ["github.com", "gitlab.com", "codeberg.org"].iter().map(|s| s.to_string()).collect();
    for h in stored.tokens.keys() {
        if !hosts.contains(h) {
            hosts.push(h.clone());
        }
    }
    let tokens = hosts.into_iter().map(|h| TokenInfo { configured: stored.get(&h).is_some(), host: h }).collect();
    Ok(Output::Settings(Settings {
        listen: g.daemon.listen.clone(),
        check_interval_mins: g.defaults.check_interval_mins,
        keep_versions: g.defaults.keep_versions,
        metadata_expire: g.defaults.metadata_expire.clone(),
        sign_metadata: g.defaults.sign_metadata,
        tokens,
    }))
}

pub fn config_set(ctx: &Ctx, key: &str, value: &str) -> Result<Output> {
    ctx.require_root("quai config")?;
    let mut g = ctx.global.clone();
    let v = value.trim();
    let mut needs_sync = false;
    let mut needs_setup = false;
    match key {
        "listen" => {
            let addr: std::net::SocketAddr = v.parse().map_err(|_| usage("listen must be IP:PORT"))?;
            if !addr.ip().is_loopback() {
                return Err(usage("quaid must listen on a loopback address"));
            }
            g.daemon.listen = addr.to_string();
            needs_sync = true;
            needs_setup = true;
        }
        "check-interval-mins" => {
            let n: u32 = v.parse().map_err(|_| usage("a number of minutes"))?;
            if !(1..=1440).contains(&n) {
                return Err(usage("check-interval-mins must be between 1 and 1440"));
            }
            g.defaults.check_interval_mins = n;
        }
        "keep-versions" => {
            let n: u32 = v.parse().map_err(|_| usage("a number"))?;
            if !(1..=20).contains(&n) {
                return Err(usage("keep-versions must be between 1 and 20"));
            }
            g.defaults.keep_versions = n;
        }
        "metadata-expire" => {
            let ok = regex::Regex::new(r"^(\d+[smhd]?|never)$").unwrap().is_match(v);
            if !ok {
                return Err(usage("metadata-expire looks like 6h, 30m, 1d or never"));
            }
            g.defaults.metadata_expire = v.to_string();
            needs_sync = true;
        }
        "sign-metadata" => {
            g.defaults.sign_metadata = match v {
                "true" | "yes" | "1" => true,
                "false" | "no" | "0" => false,
                _ => return Err(usage("sign-metadata is true or false")),
            };
            needs_sync = true;
            needs_setup = g.defaults.sign_metadata;
        }
        other => {
            return Err(usage(format!(
                "unknown setting '{other}'; expected listen, check-interval-mins, keep-versions, metadata-expire or sign-metadata"
            )))
        }
    }
    config::save_global(&ctx.paths, &g)?;
    let new_ctx = Ctx { paths: ctx.paths.clone(), global: g, json: ctx.json, yes: ctx.yes };
    let mut details = vec![format!("{key} = {v}")];
    if needs_setup {
        details.extend(ensure_setup(&new_ctx, true)?);
    }
    if needs_sync {
        let (sources, _) = config::load_sources(&new_ctx.paths);
        let r = repofile::sync(&new_ctx.paths, &new_ctx.global, &sources)?;
        if !r.written.is_empty() {
            details.push(format!("updated repo files: {}", r.written.join(", ")));
        }
    }
    restart_quaid();
    Ok(ok("Setting saved", details))
}

pub async fn refresh(ctx: &Ctx, id: Option<&str>) -> Result<Output> {
    if let Some(id) = id {
        config::load_source(&ctx.paths, id)?;
    }
    Ok(Output::Refresh(ctx.daemon_refresh(id).await?))
}

pub async fn upgrade(ctx: &Ctx, id: Option<&str>) -> Result<Output> {
    ctx.require_root("quai upgrade")?;
    let sources = match id {
        Some(id) => vec![config::load_source(&ctx.paths, id)?],
        None => config::load_sources(&ctx.paths).0.into_iter().filter(|s| s.enabled).collect(),
    };
    let names: Vec<String> = sources.iter().flat_map(|s| s.packages.clone()).collect();
    let installed: Vec<String> = system::installed_versions(&names).into_keys().collect();
    if installed.is_empty() {
        return Ok(ok("Nothing to upgrade: none of these packages is installed", vec![]));
    }
    let ids: Vec<String> = sources.iter().map(|s| s.id.clone()).collect();
    let mut details = Vec::new();
    // Make quaid look at the forges now rather than within the check interval.
    if let Err(e) = ctx.daemon_refresh(id).await {
        details.push(format!("{e:#}"));
    }
    if system::is_atomic() {
        dnf_makecache(ctx, &ids, true)?;
        run(ctx, Command::new("rpm-ostree").arg("upgrade"))?;
        return Ok(Output::Action(ActionResult {
            ok: true,
            message: "rpm-ostree staged the upgrade".into(),
            source: None,
            details,
            reboot_required: true,
        }));
    }
    if let Some(d) = dnf_makecache(ctx, &ids, true)? {
        details.push(d);
    }
    let before = system::installed_versions(&installed);
    let mut cmd = system::dnf().ok_or_else(|| not_found("dnf is not installed"))?;
    cmd.arg("-y").arg("upgrade").args(&installed);
    run(ctx, &mut cmd)?;
    let after = system::installed_versions(&installed);
    let mut changed = 0;
    for (name, versions) in &after {
        let old = before.get(name).and_then(|v| v.first()).map(|(e, _)| e.clone()).unwrap_or_default();
        let new = versions.first().map(|(e, _)| e.clone()).unwrap_or_default();
        if old != new {
            changed += 1;
            details.push(format!("{name}: {old} → {new}"));
        }
    }
    let message = if changed == 0 { "Already up to date".to_string() } else { format!("Upgraded {changed} package(s)") };
    Ok(ok(message, details))
}
