use quai_i18n::{tr, trc, trf, trn};
use quai_types::{Severity, SourceState, SourceStatus};

/// A size in the user's language and number format ("1,2 Mo"), by GLib.
pub fn human_size(bytes: u64) -> String {
    gtk::glib::format_size(bytes).to_string()
}

/// "3 minutes ago" style rendering of an RFC 3339 timestamp.
pub fn relative_time(rfc3339: &str) -> String {
    let Ok(t) = gtk::glib::DateTime::from_iso8601(rfc3339, None) else {
        return rfc3339.to_string();
    };
    let Ok(now) = gtk::glib::DateTime::now_utc() else {
        return rfc3339.to_string();
    };
    let secs = now.difference(&t).as_seconds().max(0) as u64;
    match secs {
        s if s < 60 => tr("just now"),
        s if s < 3600 => trn("{n} minute ago", "{n} minutes ago", s / 60),
        s if s < 86_400 => trn("{n} hour ago", "{n} hours ago", s / 3600),
        s => trn("{n} day ago", "{n} days ago", s / 86_400),
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

pub fn state_label(state: SourceState) -> String {
    match state {
        SourceState::UpToDate => trc("source state", "Up to date"),
        SourceState::UpdateAvailable => trc("source state", "Update available"),
        SourceState::NotInstalled => trc("source state", "Not installed"),
        SourceState::NeverChecked => trc("source state", "Not checked yet"),
        SourceState::Error => trc("source state", "Error"),
        SourceState::Disabled => trc("source state", "Disabled"),
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
            parts.push(trf("{version} available", &[("version", &p.available.clone().unwrap_or_default())]))
        }
        _ => {}
    }
    parts.join(" · ")
}

/// Markup-safe text for row titles and subtitles.
pub fn esc(s: &str) -> String {
    gtk::glib::markup_escape_text(s).to_string()
}
