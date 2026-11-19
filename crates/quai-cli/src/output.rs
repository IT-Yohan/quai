use quai_i18n::{tr, trc, trf};
use quai_types::*;

pub enum Output {
    Preview(Preview),
    Action(ActionResult),
    Sources(Vec<SourceView>),
    Status(StatusReport),
    Doctor(DoctorReport),
    Settings(Settings),
    Refresh(RefreshReply),
    /// Raw text (export).
    Text(String),
}

impl Output {
    /// Should the process exit non-zero?
    pub fn failed(&self) -> bool {
        match self {
            Output::Action(a) => !a.ok,
            Output::Doctor(d) => !d.ok,
            Output::Refresh(r) => !r.errors.is_empty(),
            _ => false,
        }
    }
}

pub fn print(out: &Output, json: bool) {
    if json {
        let v = match out {
            Output::Preview(p) => serde_json::to_string_pretty(p),
            Output::Action(a) => serde_json::to_string_pretty(a),
            Output::Sources(s) => serde_json::to_string_pretty(s),
            Output::Status(s) => serde_json::to_string_pretty(s),
            Output::Doctor(d) => serde_json::to_string_pretty(d),
            Output::Settings(s) => serde_json::to_string_pretty(s),
            Output::Refresh(r) => serde_json::to_string_pretty(r),
            Output::Text(t) => serde_json::to_string_pretty(&serde_json::json!({ "text": t })),
        };
        println!("{}", v.unwrap_or_else(|e| format!("{{\"error\":\"{e}\",\"kind\":\"internal\"}}")));
        return;
    }
    match out {
        Output::Preview(p) => print_preview(p),
        Output::Action(a) => print_action(a),
        Output::Sources(s) => print_sources(s),
        Output::Status(s) => print_status(s),
        Output::Doctor(d) => print_doctor(d),
        Output::Settings(s) => print_settings(s),
        Output::Refresh(r) => print_refresh(r),
        Output::Text(t) => print!("{t}"),
    }
}

pub fn print_error(e: &anyhow::Error, json: bool) {
    let kind = quai_core::error::kind_of(e).as_str().to_string();
    if json {
        let report = ErrorReport { error: format!("{e:#}"), kind };
        println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
    } else {
        eprintln!("quai: {e:#}");
    }
}

fn mark(sev: Severity) -> &'static str {
    match sev {
        Severity::Info => "ℹ",
        Severity::Warning => "⚠",
        Severity::Danger => "✗",
    }
}

/// An RFC 3339 timestamp as local date and time, "2026-11-19 15:34".
fn local_time(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| rfc3339.to_string())
}

fn human_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1_048_576.0;
    if mb >= 1.0 {
        // Translators: a file size in mebibytes.
        trf("{size} MiB", &[("size", &format!("{mb:.1}"))])
    } else {
        // Translators: a file size in kibibytes.
        trf("{size} KiB", &[("size", &format!("{:.0}", bytes as f64 / 1024.0))])
    }
}

/// Print `label  value` rows with the labels aligned, whatever their length
/// in the current language.
fn print_rows(indent: &str, rows: &[(String, String)]) {
    let width = rows.iter().map(|(l, _)| l.chars().count()).max().unwrap_or(0);
    for (label, value) in rows {
        let pad = width - label.chars().count();
        println!("{indent}{label}{}  {value}", " ".repeat(pad));
    }
}

pub fn print_preview(p: &Preview) {
    println!("{} ({})", p.suggested_name, p.web_url);
    let release = if p.release.prerelease {
        trf("{tag} (pre-release)", &[("tag", &p.release.tag)])
    } else {
        p.release.tag.clone()
    };
    let mut rows = vec![(tr("release"), release), (tr("source id"), p.suggested_id.clone())];
    if let Some(pat) = &p.asset_pattern {
        rows.push((tr("assets"), trf("matching /{pattern}/", &[("pattern", pat)])));
    }
    print_rows("  ", &rows);
    println!("  {}", tr("packages:"));
    for pkg in &p.packages {
        let here = if quai_core::system::arch_compatible(&pkg.arch, &p.host_arch) {
            String::new()
        } else {
            format!("  ({})", tr("other architecture"))
        };
        let size = pkg.size.map(human_size).unwrap_or_default();
        println!("    {} {} {}  {}{}", pkg.name, pkg.evr, pkg.arch, size, here);
        if !pkg.summary.is_empty() {
            println!("      {}", pkg.summary);
        }
        if let Some(i) = &pkg.installed {
            println!("      {}", trf("installed: {version} (quai will keep it updated)", &[("version", i)]));
        }
        if let Some(r) = &pkg.other_repo {
            println!("      ⚠ {}", trf("also provided by repository '{repo}'", &[("repo", r)]));
        }
    }
    for v in &p.variants {
        println!(
            "  ⚠ {}",
            trf(
                "several files provide {package} for {arch}: {files}",
                &[("package", &v.name), ("arch", &v.arch), ("files", &v.assets.join(", "))],
            )
        );
    }
    for w in &p.warnings {
        println!("  {} {}", mark(w.severity), w.message);
    }
}

fn print_action(a: &ActionResult) {
    println!("{} {}", if a.ok { "✓" } else { "✗" }, a.message);
    for d in &a.details {
        println!("  {d}");
    }
    if a.reboot_required {
        println!("  {}", tr("Reboot to apply the change (rpm-ostree)."));
    }
}

fn no_sources() {
    println!("{}", tr("No sources. Add one with: sudo quai add owner/repo"));
}

fn print_sources(list: &[SourceView]) {
    if list.is_empty() {
        no_sources();
        return;
    }
    let w = list.iter().map(|s| s.id.chars().count()).max().unwrap_or(2).max(2);
    for s in list {
        let disabled = if s.enabled { String::new() } else { format!(" ({})", trc("source state", "disabled")) };
        println!("{:<w$}  {}{}  {}  [{}]", s.id, s.web_url, disabled, s.packages.join(", "), s.channel.as_str());
    }
}

fn state_label(s: SourceState) -> String {
    let (mark, text) = match s {
        SourceState::UpToDate => ("✓", trc("source state", "up to date")),
        SourceState::UpdateAvailable => ("↑", trc("source state", "update available")),
        SourceState::NotInstalled => ("·", trc("source state", "not installed")),
        SourceState::NeverChecked => ("…", trc("source state", "not checked yet")),
        SourceState::Error => ("✗", trc("source state", "error")),
        SourceState::Disabled => ("–", trc("source state", "disabled")),
    };
    format!("{mark} {text}")
}

fn print_status(r: &StatusReport) {
    if r.sources.is_empty() {
        no_sources();
    }
    for s in &r.sources {
        println!("{}  {}  ({})", s.source.id, state_label(s.state), s.source.web_url);
        for p in &s.packages {
            let avail = p.available.as_deref().unwrap_or("?");
            match &p.installed {
                Some(i) if p.update_available => println!("  {}  {i} → {avail}", p.name),
                Some(i) => println!("  {}  {i}", p.name),
                None => println!("  {}  {}", p.name, trf("available {version}", &[("version", &avail)])),
            }
        }
        if let Some(t) = &s.last_check {
            println!("  {}", trf("last check {time}", &[("time", &local_time(t))]));
        }
        if let Some(e) = &s.last_error {
            println!("  ✗ {e}");
        }
        for x in &s.excluded {
            println!(
                "  ⚠ {}",
                trf(
                    "{package} ({file}) not served: {reason}",
                    &[("package", &x.name), ("file", &x.file), ("reason", &x.reason)],
                )
            );
        }
        if !s.repo_file_in_sync {
            println!(
                "  ⚠ {}",
                trf("/etc/yum.repos.d/quai-{id}.repo is out of date: run `sudo quai sync`", &[("id", &s.source.id)],)
            );
        }
    }
    if !r.daemon_reachable && !r.sources.is_empty() {
        println!("⚠ {}", tr("quaid is not reachable: run `quai doctor`"));
    }
}

fn print_doctor(d: &DoctorReport) {
    for c in &d.checks {
        println!("{} {}: {}", if c.ok { "✓" } else { "✗" }, c.title, c.detail);
        if let (false, Some(fix)) = (c.ok, &c.fix) {
            println!("    {}", trf("fix: {command}", &[("command", fix)]));
        }
    }
    println!("{}", if d.ok { tr("Everything looks fine.") } else { tr("Some checks failed.") });
}

fn print_settings(s: &Settings) {
    // The setting names are the keys of `quai config KEY VALUE`: not translated.
    let mut rows = vec![
        ("listen".to_string(), s.listen.clone()),
        ("check-interval-mins".to_string(), s.check_interval_mins.to_string()),
        ("keep-versions".to_string(), s.keep_versions.to_string()),
        ("metadata-expire".to_string(), s.metadata_expire.clone()),
        ("sign-metadata".to_string(), s.sign_metadata.to_string()),
    ];
    for t in &s.tokens {
        let state = if t.configured { trc("token", "set") } else { trc("token", "not set") };
        rows.push((tr("token"), format!("{} ({state})", t.host)));
    }
    print_rows("", &rows);
}

fn print_refresh(r: &RefreshReply) {
    for id in &r.refreshed {
        println!("✓ {}", trf("{id} checked", &[("id", id)]));
    }
    for id in &r.skipped {
        println!("· {}", trf("{id} skipped (disabled or checked less than 30 s ago)", &[("id", id)]));
    }
    for (id, e) in &r.errors {
        println!("✗ {id}: {e}");
    }
}
