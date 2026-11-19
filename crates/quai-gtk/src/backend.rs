//! Talks to the `quai` CLI. Read-only commands run as the user; commands that
//! change the system run through `pkexec`, which shows the polkit prompt.

use gtk::gio;
use quai_i18n::trf;
use quai_types::frontend;
use serde::de::DeserializeOwned;
use std::ffi::OsStr;

fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() == 0 }
}

/// Run `quai <args> --json` and decode its answer.
///
/// `privileged` commands go through pkexec (skipped when already root, or when
/// `QUAI_GUI_NO_PKEXEC` is set for development). `stdin` is fed to the command.
pub async fn quai<T: DeserializeOwned>(args: &[String], privileged: bool, stdin: Option<String>) -> Result<T, String> {
    let (argv, used_pkexec) = frontend::command_line(args, privileged, is_root());
    let os: Vec<&OsStr> = argv.iter().map(OsStr::new).collect();
    let mut flags = gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE;
    if stdin.is_some() {
        flags |= gio::SubprocessFlags::STDIN_PIPE;
    }
    let proc = gio::Subprocess::newv(&os, flags).map_err(|e| trf("cannot start quai: {error}", &[("error", &e)]))?;
    let (out, err) =
        proc.communicate_utf8_future(stdin).await.map_err(|e| trf("quai did not answer: {error}", &[("error", &e)]))?;
    let out = out.map(|s| s.to_string()).unwrap_or_default();
    let err = err.map(|s| s.to_string()).unwrap_or_default();
    let code = if proc.has_exited() { proc.exit_status() } else { -1 };
    let json = frontend::interpret(&out, &err, code, used_pkexec)?;
    serde_json::from_str::<T>(&json).map_err(|e| trf("unexpected answer from quai: {error}", &[("error", &e)]))
}

pub fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}
