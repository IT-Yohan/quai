//! preview, add, remove, set, enable/disable, export, import.

use crate::admin;
use crate::ctx::Ctx;
use crate::output::{self, Output};
use crate::Selection;
use anyhow::{Context, Result};
use quai_core::config::{self, ExportFile, SourceConfig};
use quai_core::error::{conflict, not_found, usage};
use quai_core::forge::ForgeRef;
use quai_core::{repofile, spec, system};
use quai_types::{ActionResult, Channel, ForgeKind, Preview, PreviewPackage, ReleaseInfo, Severity, Warning};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Regex matching an asset name whatever its version: `app-1.2.3-el9.x86_64.rpm` → `^app-.*-el9\.x86_64\.rpm$`.
pub fn suggest_pattern(asset: &str, version: &str) -> String {
    let escaped = regex::escape(asset);
    let v = regex::escape(version);
    let body = if !version.is_empty() && escaped.contains(&v) { escaped.replacen(&v, ".*", 1) } else { escaped };
    format!("^{body}$")
}

fn parse_channel(ctx: &Ctx, s: Option<&str>) -> Result<Channel> {
    match s {
        None => Ok(ctx.global.defaults.channel),
        Some(v) => Channel::parse(v).ok_or_else(|| usage(format!("unknown channel '{v}'"))),
    }
}

fn forge_ref(sel: &Selection) -> Result<ForgeRef> {
    let forge = match &sel.forge {
        Some(f) => Some(ForgeKind::parse(f).ok_or_else(|| usage(format!("unknown forge '{f}'")))?),
        None => None,
    };
    let mut fref = spec::parse(&sel.spec, forge)?;
    fref.api_url = sel.api_url.clone();
    Ok(fref)
}

/// Everything `add` needs to know, computed without root.
pub async fn compute_preview(ctx: &Ctx, sel: &Selection) -> Result<(Preview, ForgeRef, Channel)> {
    let fref = forge_ref(sel)?;
    let channel = parse_channel(ctx, sel.channel.as_deref())?;
    let engine = ctx.engine()?;
    let core = engine.preview(&fref, channel, sel.asset_pattern.as_deref()).await?;

    let names: Vec<String> =
        core.packages.iter().map(|p| p.info.name.clone()).collect::<BTreeSet<_>>().into_iter().collect();
    let installed = system::installed_versions(&names);
    let other: BTreeMap<String, Option<String>> =
        names.iter().map(|n| (n.clone(), system::other_repo_providing(n))).collect();

    let mut warnings = core.warnings.clone();
    for v in &core.variants {
        if let Some(first) = v.assets.first() {
            let version = core.packages.iter().find(|p| &p.asset.name == first).map(|p| p.info.version.clone());
            warnings.push(Warning {
                code: "multiple_variants".into(),
                severity: Severity::Warning,
                message: format!(
                    "{} files provide {} for {}; pick one, e.g. --asset-pattern '{}'",
                    v.assets.len(),
                    v.name,
                    v.arch,
                    suggest_pattern(first, &version.unwrap_or_default())
                ),
            });
        }
    }
    for (name, repo) in &other {
        if let Some(r) = repo {
            warnings.push(Warning {
                code: "name_in_other_repo".into(),
                severity: Severity::Warning,
                message: format!("repository '{r}' already ships a package named {name}; dnf will pick the newest version"),
            });
        }
    }

    let packages = core
        .packages
        .iter()
        .map(|p| PreviewPackage {
            name: p.info.name.clone(),
            evr: p.info.evr(),
            arch: p.info.arch.clone(),
            summary: p.info.summary.clone(),
            size: p.size,
            asset_name: p.asset.name.clone(),
            checksum: p.checksum,
            installed: installed.get(&p.info.name).and_then(|v| v.first()).map(|(evr, _)| evr.clone()),
            other_repo: other.get(&p.info.name).cloned().flatten(),
        })
        .collect();

    let last = fref.project.rsplit('/').next().unwrap_or(&fref.project).to_string();
    let preview = Preview {
        forge: fref.forge,
        host: fref.host.clone(),
        project: fref.project.clone(),
        web_url: fref.web_url(),
        suggested_id: config::unique_id(&ctx.paths, &fref.project),
        suggested_name: title_case(&last),
        release: ReleaseInfo {
            tag: core.release.tag.clone(),
            name: core.release.name.clone(),
            published_at: core.release.published_at.map(|t| t.to_rfc3339()),
            prerelease: core.release.prerelease,
        },
        packages,
        variants: core.variants.clone(),
        warnings,
        asset_pattern: sel.asset_pattern.clone(),
        host_arch: system::host_arch(),
    };
    Ok((preview, fref, channel))
}

pub async fn preview(ctx: &Ctx, sel: &Selection) -> Result<Output> {
    let (p, _, _) = compute_preview(ctx, sel).await?;
    Ok(Output::Preview(p))
}

fn installable(names: &[String]) -> Vec<String> {
    names.iter().filter(|n| !n.ends_with("-debuginfo") && !n.ends_with("-debugsource")).cloned().collect()
}

/// Install packages with dnf, or layer them with rpm-ostree on Atomic. Returns (details, reboot).
pub fn install_packages(ctx: &Ctx, names: &[String]) -> Result<(Vec<String>, bool)> {
    let names = installable(names);
    if names.is_empty() {
        return Ok((vec![], false));
    }
    if system::is_atomic() {
        let mut cmd = Command::new("rpm-ostree");
        cmd.arg("install").arg("--idempotent").args(&names);
        run(ctx, &mut cmd)?;
        return Ok((vec![format!("rpm-ostree install {}: staged for the next boot", names.join(" "))], true));
    }
    let mut cmd = system::dnf().ok_or_else(|| not_found("dnf is not installed"))?;
    cmd.arg("-y").arg("install").args(&names);
    run(ctx, &mut cmd)?;
    Ok((vec![format!("installed {}", names.join(", "))], false))
}

/// Run a system command: visible in a terminal, captured for --json.
pub fn run(ctx: &Ctx, cmd: &mut Command) -> Result<()> {
    if ctx.json {
        system::run_captured(cmd).map(|_| ())
    } else {
        system::run_interactive(cmd)
    }
}

/// Have dnf fetch the new repository once: validates the chain and imports the key.
pub fn dnf_makecache(ctx: &Ctx, ids: &[String], refresh: bool) -> Result<Option<String>> {
    let Some(mut cmd) = system::dnf() else { return Ok(None) };
    if system::is_atomic() {
        let mut cmd = Command::new("rpm-ostree");
        cmd.arg("refresh-md");
        if refresh {
            cmd.arg("--force");
        }
        system::run_captured(&mut cmd)?;
        return Ok(Some("rpm-ostree refreshed its metadata".into()));
    }
    cmd.arg("-y").arg("makecache");
    if refresh {
        cmd.arg("--refresh");
    }
    for id in ids {
        let repo = repofile::repo_id(id);
        cmd.arg(format!("--repo={repo}"));
        // The .repo files say skip_if_unavailable=1 so a broken source never blocks
        // dnf; here we want its errors, signature failures included.
        cmd.arg(format!("--setopt={repo}.skip_if_unavailable=0"));
    }
    system::run_captured(&mut cmd).with_context(|| "dnf could not load the quai repository".to_string())?;
    let _ = ctx;
    Ok(Some(format!("dnf loaded {}", ids.iter().map(|i| repofile::repo_id(i)).collect::<Vec<_>>().join(", "))))
}

fn rollback(ctx: &Ctx, id: &str) {
    let _ = config::remove_source(&ctx.paths, id);
    let _ = repofile::remove(&ctx.paths, id);
}

pub async fn add(
    ctx: &Ctx,
    sel: &Selection,
    id: Option<String>,
    name: Option<String>,
    keep: Option<u32>,
    install: bool,
) -> Result<Output> {
    ctx.require_root("quai add")?;
    let (preview, fref, channel) = compute_preview(ctx, sel).await?;

    if !preview.variants.is_empty() && sel.asset_pattern.is_none() {
        let hints: Vec<String> =
            preview.warnings.iter().filter(|w| w.code == "multiple_variants").map(|w| w.message.clone()).collect();
        return Err(usage(format!("this release ships several variants of the same package: {}", hints.join("; "))));
    }
    if let Some(existing) = config::find_by_project(&ctx.paths, fref.forge, &fref.host, &fref.project) {
        return Err(conflict(format!("{} is already tracked as '{}'", fref.display(), existing.id)));
    }
    let host = system::host_arch();
    if !preview.packages.iter().any(|p| system::arch_compatible(&p.arch, &host)) {
        let arches: BTreeSet<&str> = preview.packages.iter().map(|p| p.arch.as_str()).collect();
        return Err(not_found(format!(
            "{} ships no package for {host} (found: {})",
            fref.display(),
            arches.into_iter().collect::<Vec<_>>().join(", ")
        )));
    }

    let id = id.unwrap_or_else(|| preview.suggested_id.clone());
    config::ensure_new_id(&ctx.paths, &id)?;
    let names: Vec<String> =
        preview.packages.iter().map(|p| p.name.clone()).collect::<BTreeSet<_>>().into_iter().collect();
    let src = SourceConfig {
        id: id.clone(),
        name: name.unwrap_or_else(|| preview.suggested_name.clone()),
        forge: fref.forge,
        host: fref.host.clone(),
        project: fref.project.clone(),
        api_url: fref.api_url.clone(),
        channel,
        asset_pattern: sel.asset_pattern.clone().filter(|p| !p.is_empty()),
        packages: names.clone(),
        keep_versions: keep.unwrap_or(ctx.global.defaults.keep_versions),
        enabled: true,
        allow_obsoletes: vec![],
        package_gpgkey: None,
        added_at: Some(chrono::Utc::now().to_rfc3339()),
    };
    src.validate()?;

    if ctx.interactive() {
        output::print_preview(&preview);
        eprintln!();
    }
    ctx.confirm(&format!("Track {} as '{}' and let dnf update {}?", fref.display(), id, names.join(", ")))?;

    let mut details = admin::ensure_setup(ctx, true)?;
    config::save_source(&ctx.paths, &src)?;
    repofile::write(&ctx.paths, &ctx.global, &src)?;
    details.push(format!("wrote {} and {}", ctx.paths.source_file(&id).display(), ctx.paths.repo_file(&id).display()));

    // Build the repository now so errors surface here, not at the next dnf run.
    match ctx.daemon_refresh(Some(&id)).await {
        Ok(r) if r.errors.is_empty() => details.push("quaid built the repository".into()),
        Ok(r) => {
            rollback(ctx, &id);
            let msg = r.errors.into_iter().map(|(_, e)| e).collect::<Vec<_>>().join("; ");
            return Err(anyhow::anyhow!("quaid could not build the repository: {msg}"));
        }
        Err(e) if system::has_systemd() => {
            rollback(ctx, &id);
            return Err(e);
        }
        Err(e) => details.push(format!("quaid not reachable yet: {e:#}")),
    }
    match dnf_makecache(ctx, &[id.clone()], false) {
        Ok(Some(d)) => details.push(d),
        Ok(None) => details.push("dnf not found: skipped the metadata check".into()),
        Err(e) => {
            rollback(ctx, &id);
            return Err(e);
        }
    }

    let mut reboot = false;
    if install {
        let (d, r) = install_packages(ctx, &names)?;
        details.extend(d);
        reboot = r;
    } else if preview.packages.iter().any(|p| p.installed.is_some()) {
        details.push("already installed: future updates now come through dnf".into());
    } else {
        details.push(format!("install with: sudo dnf install {}", installable(&names).join(" ")));
    }

    Ok(Output::Action(ActionResult {
        ok: true,
        message: format!("Now tracking {} as '{id}'", fref.display()),
        source: Some(src.to_view()),
        details,
        reboot_required: reboot,
    }))
}

pub fn remove(ctx: &Ctx, id: &str, uninstall: bool) -> Result<Output> {
    ctx.require_root("quai remove")?;
    let src = config::load_source(&ctx.paths, id)?;
    let mut details = Vec::new();
    let mut reboot = false;
    if uninstall {
        let installed: Vec<String> = system::installed_versions(&src.packages).into_keys().collect();
        if !installed.is_empty() {
            ctx.confirm(&format!("Uninstall {}?", installed.join(", ")))?;
            if system::is_atomic() {
                let mut cmd = Command::new("rpm-ostree");
                cmd.arg("uninstall").args(&installed);
                run(ctx, &mut cmd)?;
                reboot = true;
            } else {
                let mut cmd = system::dnf().ok_or_else(|| not_found("dnf is not installed"))?;
                cmd.arg("-y").arg("remove").args(&installed);
                run(ctx, &mut cmd)?;
            }
            details.push(format!("uninstalled {}", installed.join(", ")));
        }
    } else {
        let installed: Vec<String> = system::installed_versions(&src.packages).into_keys().collect();
        if !installed.is_empty() {
            details.push(format!("{} stays installed but will no longer be updated", installed.join(", ")));
        }
    }
    config::remove_source(&ctx.paths, id)?;
    repofile::remove(&ctx.paths, id)?;
    let _ = std::fs::remove_dir_all(ctx.paths.repo_cache_dir(id));
    let _ = std::fs::remove_file(ctx.paths.forge_cache(id));
    Ok(Output::Action(ActionResult {
        ok: true,
        message: format!("Stopped tracking '{id}'"),
        source: Some(src.to_view()),
        details,
        reboot_required: reboot,
    }))
}

async fn save_and_refresh(ctx: &Ctx, src: &SourceConfig, message: String) -> Result<Output> {
    config::save_source(&ctx.paths, src)?;
    repofile::write(&ctx.paths, &ctx.global, src)?;
    let mut details = vec![];
    if src.enabled {
        match ctx.daemon_refresh(Some(&src.id)).await {
            Ok(r) => {
                for (_, e) in r.errors {
                    details.push(format!("quaid: {e}"));
                }
            }
            Err(e) => details.push(format!("{e:#}")),
        }
    }
    Ok(Output::Action(ActionResult { ok: true, message, source: Some(src.to_view()), details, reboot_required: false }))
}

pub async fn set_enabled(ctx: &Ctx, id: &str, enabled: bool) -> Result<Output> {
    ctx.require_root(if enabled { "quai enable" } else { "quai disable" })?;
    let mut src = config::load_source(&ctx.paths, id)?;
    src.enabled = enabled;
    save_and_refresh(ctx, &src, format!("'{id}' {}", if enabled { "enabled" } else { "disabled" })).await
}

pub async fn set(ctx: &Ctx, id: &str, key: &str, value: &str) -> Result<Output> {
    ctx.require_root("quai set")?;
    let mut src = config::load_source(&ctx.paths, id)?;
    let value = value.trim();
    match key {
        "name" => {
            if value.is_empty() {
                return Err(usage("name cannot be empty"));
            }
            src.name = value.to_string();
        }
        "channel" => src.channel = Channel::parse(value).ok_or_else(|| usage("channel is stable or prerelease"))?,
        "asset-pattern" => {
            config::compile_pattern(Some(value))?;
            src.asset_pattern = (!value.is_empty()).then(|| value.to_string());
        }
        "keep-versions" => src.keep_versions = value.parse().map_err(|_| usage("keep-versions is a number"))?,
        "allow-package" => {
            config::validate_package_name(value)?;
            if !src.packages.iter().any(|p| p == value) {
                src.packages.push(value.to_string());
                src.packages.sort();
            }
        }
        "disallow-package" => {
            src.packages.retain(|p| p != value);
            if src.packages.is_empty() {
                return Err(usage("a source needs at least one package; use `quai remove` instead"));
            }
        }
        "allow-obsoletes" => {
            config::validate_package_name(value)?;
            if !src.allow_obsoletes.iter().any(|p| p == value) {
                src.allow_obsoletes.push(value.to_string());
            }
        }
        "disallow-obsoletes" => src.allow_obsoletes.retain(|p| p != value),
        "package-gpgkey" => {
            if !value.is_empty() && !(value.starts_with("https://") || value.starts_with("file:///")) {
                return Err(usage("package-gpgkey must be an https:// or file:/// URL"));
            }
            src.package_gpgkey = (!value.is_empty()).then(|| value.to_string());
        }
        other => {
            return Err(usage(format!(
                "unknown key '{other}'; expected name, channel, asset-pattern, keep-versions, allow-package, \
                 disallow-package, allow-obsoletes, disallow-obsoletes or package-gpgkey"
            )))
        }
    }
    src.validate()?;
    save_and_refresh(ctx, &src, format!("'{id}': {key} updated")).await
}

pub fn export(ctx: &Ctx, output: Option<&Path>) -> Result<Output> {
    let (sources, bad) = config::load_sources(&ctx.paths);
    for (id, e) in &bad {
        ctx.note(format!("skipping unreadable source '{id}': {e:#}"));
    }
    let file = ExportFile { sources };
    let text = format!(
        "# quai sources, exported {}.\n# Restore with: sudo quai import <this file> [--install]\n\n{}",
        chrono::Utc::now().format("%Y-%m-%d"),
        toml::to_string_pretty(&file)?
    );
    match output {
        Some(path) => {
            std::fs::write(path, &text).with_context(|| format!("writing {}", path.display()))?;
            Ok(Output::Action(ActionResult {
                ok: true,
                message: format!("Exported {} source(s) to {}", file.sources.len(), path.display()),
                source: None,
                details: vec![],
                reboot_required: false,
            }))
        }
        None => Ok(Output::Text(text)),
    }
}

pub async fn import(ctx: &Ctx, path: &Path, install: bool, replace: bool) -> Result<Output> {
    ctx.require_root("quai import")?;
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file: ExportFile = toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    if file.sources.is_empty() {
        return Err(usage("the file lists no [[source]]"));
    }
    let mut details = admin::ensure_setup(ctx, true)?;
    let mut imported = Vec::new();
    for src in &file.sources {
        src.validate().with_context(|| format!("source '{}'", src.id))?;
        if ctx.paths.source_file(&src.id).exists() && !replace {
            details.push(format!("'{}' already exists, skipped (use --replace)", src.id));
            continue;
        }
        config::save_source(&ctx.paths, src)?;
        imported.push(src.clone());
    }
    let (all, _) = config::load_sources(&ctx.paths);
    repofile::sync(&ctx.paths, &ctx.global, &all)?;
    if !imported.is_empty() {
        match ctx.daemon_refresh(None).await {
            Ok(r) => {
                for (id, e) in r.errors {
                    details.push(format!("{id}: {e}"));
                }
            }
            Err(e) => details.push(format!("{e:#}")),
        }
        let ids: Vec<String> = imported.iter().map(|s| s.id.clone()).collect();
        if let Ok(Some(d)) = dnf_makecache(ctx, &ids, false) {
            details.push(d);
        }
    }
    let mut reboot = false;
    if install && !imported.is_empty() {
        let names: Vec<String> = imported.iter().flat_map(|s| s.packages.clone()).collect();
        let (d, r) = install_packages(ctx, &names)?;
        details.extend(d);
        reboot = r;
    }
    Ok(Output::Action(ActionResult {
        ok: true,
        message: format!("Imported {} source(s)", imported.len()),
        source: None,
        details,
        reboot_required: reboot,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_suggestion_drops_the_version() {
        let p = suggest_pattern("app-1.2.3-el9.x86_64.rpm", "1.2.3");
        let re = regex::Regex::new(&p).unwrap();
        assert!(re.is_match("app-1.2.3-el9.x86_64.rpm"));
        assert!(re.is_match("app-1.3.0-el9.x86_64.rpm"));
        assert!(!re.is_match("app-1.3.0-fc40.x86_64.rpm"));
    }
}
