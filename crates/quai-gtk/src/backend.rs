//! Talks to the `quai` CLI. Read-only commands run as the user; commands that
//! change the system run through `pkexec`, which shows the polkit prompt.

use gtk::{gio, glib};
use quai_types::ErrorReport;
use serde::de::DeserializeOwned;
use std::ffi::OsStr;

fn quai_binary() -> String {
    if let Ok(p) = std::env::var("QUAI_BIN") {
        return p;
    }
    glib::find_program_in_path("quai")
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/usr/bin/quai".to_string())
}

fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() == 0 }
}

/// Run `quai <args> --json` and decode its answer.
///
/// `privileged` commands go through pkexec (skipped when already root, or when
/// `QUAI_GTK_NO_PKEXEC` is set for development). `stdin` is fed to the command.
pub async fn quai<T: DeserializeOwned>(args: &[String], privileged: bool, stdin: Option<String>) -> Result<T, String> {
    let mut argv: Vec<String> = Vec::new();
    let use_pkexec = privileged && !is_root() && std::env::var_os("QUAI_GTK_NO_PKEXEC").is_none();
    if use_pkexec {
        argv.push("pkexec".into());
    }
    argv.push(quai_binary());
    argv.extend(args.iter().cloned());
    argv.push("--json".into());
    if privileged {
        argv.push("--yes".into());
    }
    let os: Vec<&OsStr> = argv.iter().map(OsStr::new).collect();
    let mut flags = gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE;
    if stdin.is_some() {
        flags |= gio::SubprocessFlags::STDIN_PIPE;
    }
    let proc = gio::Subprocess::newv(&os, flags).map_err(|e| format!("cannot start quai: {e}"))?;
    let (out, err) = proc.communicate_utf8_future(stdin).await.map_err(|e| format!("quai did not answer: {e}"))?;
    let out = out.map(|s| s.to_string()).unwrap_or_default();
    let err = err.map(|s| s.to_string()).unwrap_or_default();
    let code = if proc.has_exited() { proc.exit_status() } else { -1 };

    if use_pkexec && (code == 126 || code == 127) && out.trim().is_empty() {
        return Err("Authentication was cancelled or refused.".to_string());
    }
    if let Ok(report) = serde_json::from_str::<ErrorReport>(&out) {
        return Err(report.error);
    }
    match serde_json::from_str::<T>(&out) {
        Ok(v) => Ok(v),
        Err(e) if code == 0 => Err(format!("unexpected answer from quai: {e}")),
        Err(_) => {
            let msg = err.trim();
            Err(if msg.is_empty() { format!("quai failed (exit code {code})") } else { msg.to_string() })
        }
    }
}

pub fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}
