//! list, status, doctor: read-only, no root needed.

use crate::ctx::Ctx;
use crate::output::Output;
use anyhow::Result;
use quai_core::config::{self, SourceConfig};
use quai_core::engine::{Engine, Snapshot};
use quai_core::evr::compare_evr;
use quai_core::{repofile, system};
use quai_types::{DoctorCheck, DoctorReport, PackageState, SourceState, SourceStatus, StatusReport};
use std::cmp::Ordering;
use std::collections::BTreeMap;

pub fn list(ctx: &Ctx) -> Result<Output> {
    let (sources, bad) = config::load_sources(&ctx.paths);
    for (id, e) in bad {
        ctx.note(format!("unreadable source '{id}': {e:#}"));
    }
    Ok(Output::Sources(sources.iter().map(|s| s.to_view()).collect()))
}

fn source_status(
    ctx: &Ctx,
    src: &SourceConfig,
    snap: Option<&Snapshot>,
    installed: &BTreeMap<String, Vec<(String, String)>>,
) -> SourceStatus {
    let host = system::host_arch();
    let mut packages = Vec::new();
    for name in &src.packages {
        // Newest served EVR installable on this machine.
        let available = snap.and_then(|s| {
            s.packages
                .iter()
                .filter(|p| &p.name == name && system::arch_compatible(&p.arch, &host))
                .max_by(|a, b| compare_evr(&a.evr, &b.evr))
                .map(|p| (p.evr.clone(), p.arch.clone()))
        });
        let inst = installed.get(name).and_then(|v| v.iter().max_by(|a, b| compare_evr(&a.0, &b.0))).cloned();
        let update_available = match (&inst, &available) {
            (Some((i, _)), Some((a, _))) => compare_evr(a, i) == Ordering::Greater,
            _ => false,
        };
        packages.push(PackageState {
            name: name.clone(),
            arch: available.as_ref().map(|(_, a)| a.clone()).or_else(|| inst.as_ref().map(|(_, a)| a.clone())),
            available: available.map(|(e, _)| e),
            installed: inst.map(|(e, _)| e),
            update_available,
        });
    }
    let any_installed = packages.iter().any(|p| p.installed.is_some());
    let state = if !src.enabled {
        SourceState::Disabled
    } else if snap.is_none() {
        SourceState::NeverChecked
    } else if snap.map(|s| s.last_error.is_some() && s.packages.is_empty()).unwrap_or(false) {
        SourceState::Error
    } else if packages.iter().any(|p| p.update_available) {
        SourceState::UpdateAvailable
    } else if !any_installed {
        SourceState::NotInstalled
    } else if snap.map(|s| s.last_error.is_some()).unwrap_or(false) {
        SourceState::Error
    } else {
        SourceState::UpToDate
    };
    SourceStatus {
        source: src.to_view(),
        state,
        packages,
        last_check: snap.and_then(|s| s.last_check).map(|t| t.to_rfc3339()),
        last_error: snap.and_then(|s| s.last_error.clone()),
        latest_release: snap.and_then(|s| s.latest_release.clone()),
        excluded: snap.map(|s| s.excluded.clone()).unwrap_or_default(),
        repo_file_in_sync: repofile::in_sync(&ctx.paths, &ctx.global, src),
    }
}

pub async fn compute_status(ctx: &Ctx, id: Option<&str>) -> Result<StatusReport> {
    let sources = match id {
        Some(id) => vec![config::load_source(&ctx.paths, id)?],
        None => config::load_sources(&ctx.paths).0,
    };
    let names: Vec<String> = sources.iter().flat_map(|s| s.packages.clone()).collect();
    let installed = system::installed_versions(&names);
    let list = sources
        .iter()
        .map(|s| {
            let snap = Engine::load_snapshot(&ctx.paths, &s.id);
            source_status(ctx, s, snap.as_ref(), &installed)
        })
        .collect();
    Ok(StatusReport { sources: list, daemon_reachable: ctx.daemon_healthy().await, atomic: system::is_atomic() })
}

pub async fn status(ctx: &Ctx, id: Option<&str>) -> Result<Output> {
    Ok(Output::Status(compute_status(ctx, id).await?))
}

fn check(id: &str, title: &str, ok: bool, detail: impl Into<String>, fix: Option<&str>) -> DoctorCheck {
    DoctorCheck { id: id.into(), title: title.into(), ok, detail: detail.into(), fix: fix.map(|s| s.to_string()) }
}

pub async fn doctor(ctx: &Ctx) -> Result<Output> {
    let p = &ctx.paths;
    let mut checks = Vec::new();

    let global_ok = config::load_global(p);
    checks.push(match &global_ok {
        Ok(_) if p.global_config().exists() => check("config", "Configuration", true, p.global_config().display().to_string(), None),
        Ok(_) => check("config", "Configuration", false, "quai has not been set up yet", Some("sudo quai setup")),
        Err(e) => check("config", "Configuration", false, format!("{e:#}"), Some("fix /etc/quai/quai.toml")),
    });

    let (sources, bad) = config::load_sources(p);
    checks.push(check(
        "sources",
        "Sources",
        bad.is_empty(),
        if bad.is_empty() {
            format!("{} source(s)", sources.len())
        } else {
            bad.iter().map(|(id, e)| format!("{id}: {e:#}")).collect::<Vec<_>>().join("; ")
        },
        Some("fix or remove the files listed under /etc/quai/sources.d"),
    ));

    if ctx.global.defaults.sign_metadata {
        let public = p.public_key().exists();
        let secret = if system::is_root() { Some(p.secret_key().exists()) } else { None };
        let ok = public && secret.unwrap_or(true);
        let detail = match (public, secret) {
            (true, Some(true)) => format!("key pair present ({})", p.public_key().display()),
            (true, None) => "public key present (run as root to check the secret key)".into(),
            _ => "the signing key is missing".into(),
        };
        checks.push(check("signing-key", "Metadata signing key", ok, detail, Some("sudo quai setup")));
    }

    if system::has_systemd() {
        let enabled = system::systemctl(&["is-enabled", "quaid.socket"]).map(|s| s.trim() == "enabled").unwrap_or(false);
        let active = system::systemctl(&["is-active", "quaid.socket"]).is_ok();
        checks.push(check(
            "socket",
            "quaid.socket",
            enabled && active,
            format!("{}, {}", if enabled { "enabled" } else { "not enabled" }, if active { "listening" } else { "not listening" }),
            Some("sudo systemctl enable --now quaid.socket"),
        ));
    }

    let healthy = ctx.daemon_healthy().await;
    checks.push(check(
        "daemon",
        "quaid answers",
        healthy,
        if healthy { format!("http://{}", ctx.global.daemon.listen) } else { format!("nothing answers on {}", ctx.global.daemon.listen) },
        Some("sudo quai setup; journalctl -u quaid.service"),
    ));

    let out_of_sync: Vec<String> =
        sources.iter().filter(|s| !repofile::in_sync(p, &ctx.global, s)).map(|s| s.id.clone()).collect();
    let orphans: Vec<String> =
        repofile::managed_ids(p).into_iter().filter(|id| !sources.iter().any(|s| &s.id == id)).collect();
    checks.push(check(
        "repo-files",
        "dnf repository files",
        out_of_sync.is_empty() && orphans.is_empty(),
        if out_of_sync.is_empty() && orphans.is_empty() {
            "in sync".to_string()
        } else {
            format!("out of date: {:?}, orphaned: {:?}", out_of_sync, orphans)
        },
        Some("sudo quai sync"),
    ));

    let dnf = system::dnf_command();
    checks.push(check(
        "dnf",
        "Package manager",
        dnf.is_some() || system::is_atomic(),
        match (dnf, system::is_atomic()) {
            (_, true) => "Fedora Atomic: rpm-ostree applies updates at the next boot".to_string(),
            (Some(d), false) => d.to_string(),
            (None, false) => "dnf not found".to_string(),
        },
        None,
    ));

    let mut low_rate = None;
    for s in &sources {
        let snap = Engine::load_snapshot(p, &s.id);
        let (ok, detail) = match &snap {
            None if !s.enabled => (true, "disabled".to_string()),
            None => (false, "never checked".to_string()),
            Some(sn) => {
                if let Some(rl) = &sn.rate_limit {
                    if rl.remaining.map(|r| r < 10).unwrap_or(false) {
                        low_rate = Some(s.host.clone());
                    }
                }
                match (&sn.last_error, sn.excluded.is_empty()) {
                    (Some(e), _) => (false, e.clone()),
                    (None, false) => (
                        false,
                        sn.excluded.iter().map(|x| format!("{} not served: {}", x.name, x.reason)).collect::<Vec<_>>().join("; "),
                    ),
                    (None, true) => (
                        true,
                        format!(
                            "{} package(s) served{}",
                            sn.packages.len(),
                            sn.latest_release.as_ref().map(|t| format!(", latest {t}")).unwrap_or_default()
                        ),
                    ),
                }
            }
        };
        let fix = format!("quai refresh {}", s.id);
        checks.push(check(&format!("source-{}", s.id), &format!("Source {}", s.id), ok, detail, Some(&fix)));
    }
    if let Some(host) = low_rate {
        let fix = format!("sudo quai token set --host {host}");
        checks.push(check("rate-limit", "API rate limit", false, format!("{host} API budget nearly used up"), Some(&fix)));
    }

    let ok = checks.iter().all(|c| c.ok);
    Ok(Output::Doctor(DoctorReport { ok, checks }))
}
