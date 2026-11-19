use quai_types::{Severity, SourceState, SourceStatus};

pub fn human_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1_048_576.0;
    if mb >= 1.0 {
        format!("{mb:.1} MB")
    } else {
        format!("{:.0} kB", bytes as f64 / 1024.0)
    }
}

/// "3 minutes ago" style rendering of an RFC 3339 timestamp.
pub fn relative_time(rfc3339: &str) -> String {
    let Ok(t) = gtk::glib::DateTime::from_iso8601(rfc3339, None) else {
        return rfc3339.to_string();
    };
    let Ok(now) = gtk::glib::DateTime::now_utc() else {
        return rfc3339.to_string();
    };
    let secs = now.difference(&t).as_seconds();
    match secs {
        s if s < 60 => "just now".to_string(),
        s if s < 3600 => plural(s / 60, "minute"),
        s if s < 86_400 => plural(s / 3600, "hour"),
        s => plural(s / 86_400, "day"),
    }
}

fn plural(n: i64, unit: &str) -> String {
    if n == 1 {
        format!("1 {unit} ago")
    } else {
        format!("{n} {unit}s ago")
    }
}

pub fn state_icon(state: SourceState) -> &'static str {
    match state {
        SourceState::UpToDate => "emblem-ok-symbolic",
        SourceState::UpdateAvailable => "go-up-symbolic",
        SourceState::NotInstalled => "folder-download-symbolic",
        SourceState::NeverChecked => "content-loading-symbolic",
        SourceState::Error => "dialog-error-symbolic",
        SourceState::Disabled => "media-playback-pause-symbolic",
    }
}

pub fn state_label(state: SourceState) -> &'static str {
    match state {
        SourceState::UpToDate => "Up to date",
        SourceState::UpdateAvailable => "Update available",
        SourceState::NotInstalled => "Not installed",
        SourceState::NeverChecked => "Not checked yet",
        SourceState::Error => "Error",
        SourceState::Disabled => "Disabled",
    }
}

pub fn state_css(state: SourceState) -> &'static str {
    match state {
        SourceState::UpToDate => "success",
        SourceState::UpdateAvailable => "accent",
        SourceState::Error => "error",
        _ => "dim-label",
    }
}

pub fn severity_icon(s: Severity) -> &'static str {
    match s {
        Severity::Info => "dialog-information-symbolic",
        Severity::Warning => "dialog-warning-symbolic",
        Severity::Danger => "dialog-error-symbolic",
    }
}

pub fn severity_css(s: Severity) -> &'static str {
    match s {
        Severity::Info => "dim-label",
        Severity::Warning => "warning",
        Severity::Danger => "error",
    }
}

/// One-line summary under a source's name in the list.
pub fn source_subtitle(s: &SourceStatus) -> String {
    let mut parts = vec![format!("{}/{}", s.source.host, s.source.project)];
    let main = s.packages.first();
    match main {
        Some(p) if p.update_available => {
            parts.push(format!("{} → {}", p.installed.as_deref().unwrap_or("?"), p.available.as_deref().unwrap_or("?")))
        }
        Some(p) if p.installed.is_some() => parts.push(p.installed.clone().unwrap_or_default()),
        Some(p) if p.available.is_some() => {
            parts.push(format!("{} available", p.available.clone().unwrap_or_default()))
        }
        _ => {}
    }
    parts.join(" · ")
}

/// Markup-safe text for row titles and subtitles.
pub fn esc(s: &str) -> String {
    gtk::glib::markup_escape_text(s).to_string()
}
