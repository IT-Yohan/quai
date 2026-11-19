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

fn human_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1_048_576.0;
    if mb >= 1.0 {
        format!("{mb:.1} MiB")
    } else {
        format!("{:.0} KiB", bytes as f64 / 1024.0)
    }
}

pub fn print_preview(p: &Preview) {
    println!("{} ({})", p.suggested_name, p.web_url);
    println!("  release   {}{}", p.release.tag, if p.release.prerelease { " (pre-release)" } else { "" });
    println!("  source id {}", p.suggested_id);
    if let Some(pat) = &p.asset_pattern {
        println!("  assets    matching /{pat}/");
    }
    println!("  packages:");
    for pkg in &p.packages {
        let here = if quai_core::system::arch_compatible(&pkg.arch, &p.host_arch) { "" } else { "  (other arch)" };
        let size = pkg.size.map(human_size).unwrap_or_default();
        println!("    {} {} {}  {}{}", pkg.name, pkg.evr, pkg.arch, size, here);
        if !pkg.summary.is_empty() {
            println!("      {}", pkg.summary);
        }
        if let Some(i) = &pkg.installed {
            println!("      installed: {i} (quai will keep it updated)");
        }
        if let Some(r) = &pkg.other_repo {
            println!("      ⚠ also provided by repository '{r}'");
        }
    }
    for v in &p.variants {
        println!("  ⚠ several files provide {} for {}: {}", v.name, v.arch, v.assets.join(", "));
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
        println!("  Reboot to apply the change (rpm-ostree).");
    }
}

fn print_sources(list: &[SourceView]) {
    if list.is_empty() {
        println!("No sources. Add one with: sudo quai add owner/repo");
        return;
    }
    let w = list.iter().map(|s| s.id.len()).max().unwrap_or(2).max(2);
    for s in list {
        println!(
            "{:<w$}  {}{}  {}  [{}]",
            s.id,
            s.web_url,
            if s.enabled { "" } else { " (disabled)" },
            s.packages.join(", "),
            s.channel.as_str()
        );
    }
}

fn state_label(s: SourceState) -> &'static str {
    match s {
        SourceState::UpToDate => "✓ up to date",
        SourceState::UpdateAvailable => "↑ update available",
        SourceState::NotInstalled => "· not installed",
        SourceState::NeverChecked => "… not checked yet",
        SourceState::Error => "✗ error",
        SourceState::Disabled => "– disabled",
    }
}

fn print_status(r: &StatusReport) {
    if r.sources.is_empty() {
        println!("No sources. Add one with: sudo quai add owner/repo");
    }
    for s in &r.sources {
        println!("{}  {}  ({})", s.source.id, state_label(s.state), s.source.web_url);
        for p in &s.packages {
            let avail = p.available.as_deref().unwrap_or("?");
            match &p.installed {
                Some(i) if p.update_available => println!("  {}  {i} → {avail}", p.name),
                Some(i) => println!("  {}  {i}", p.name),
                None => println!("  {}  available {avail}", p.name),
            }
        }
        if let Some(t) = &s.last_check {
            println!("  last check {t}");
        }
        if let Some(e) = &s.last_error {
            println!("  ✗ {e}");
        }
        for x in &s.excluded {
            println!("  ⚠ {} ({}) not served: {}", x.name, x.file, x.reason);
        }
        if !s.repo_file_in_sync {
            println!("  ⚠ /etc/yum.repos.d/quai-{}.repo is out of date: run `sudo quai sync`", s.source.id);
        }
    }
    if !r.daemon_reachable && !r.sources.is_empty() {
        println!("⚠ quaid is not reachable: run `quai doctor`");
    }
}

fn print_doctor(d: &DoctorReport) {
    for c in &d.checks {
        println!("{} {}: {}", if c.ok { "✓" } else { "✗" }, c.title, c.detail);
        if let (false, Some(fix)) = (c.ok, &c.fix) {
            println!("    fix: {fix}");
        }
    }
    println!("{}", if d.ok { "Everything looks fine." } else { "Some checks failed." });
}

fn print_settings(s: &Settings) {
    println!("listen               {}", s.listen);
    println!("check-interval-mins  {}", s.check_interval_mins);
    println!("keep-versions        {}", s.keep_versions);
    println!("metadata-expire      {}", s.metadata_expire);
    println!("sign-metadata        {}", s.sign_metadata);
    for t in &s.tokens {
        println!("token                {} ({})", t.host, if t.configured { "set" } else { "not set" });
    }
}

fn print_refresh(r: &RefreshReply) {
    for id in &r.refreshed {
        println!("✓ {id} checked");
    }
    for id in &r.skipped {
        println!("· {id} skipped (disabled or checked less than 30 s ago)");
    }
    for (id, e) in &r.errors {
        println!("✗ {id}: {e}");
    }
}
