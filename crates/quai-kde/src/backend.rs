//! The `QuaiBackend` QML element: runs `quai --json` on a worker thread and
//! hands the result back to QML through the `finished` signal.

use cxx_qt::Threading;
use cxx_qt_lib::QString;
use quai_types::frontend;
use std::io::Write;
use std::pin::Pin;
use std::process::{Command, Stdio};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "C++" {
        include!("quai-kde/cpp/app.h");
        fn run_app(version: &QString) -> i32;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        type QuaiBackend = super::QuaiBackendRust;

        /// Run `quai <args> --json`. `args_json` is a JSON array of strings;
        /// `stdin` is fed to the command when not empty. Answers with `finished(request, ...)`.
        #[qinvokable]
        fn run(self: Pin<&mut QuaiBackend>, request: i32, args_json: &QString, privileged: bool, stdin: &QString);

        /// `ok` is true when `payload` is quai's JSON document, false when it is an error message.
        #[qsignal]
        fn finished(self: Pin<&mut QuaiBackend>, request: i32, ok: bool, payload: QString);
    }

    impl cxx_qt::Threading for QuaiBackend {}
}

#[derive(Default)]
pub struct QuaiBackendRust;

fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions.
    unsafe { libc::geteuid() == 0 }
}

/// Blocking: run quai and interpret its output.
fn run_quai(args: &[String], privileged: bool, stdin: Option<String>) -> Result<String, String> {
    let (argv, used_pkexec) = frontend::command_line(args, privileged, is_root());
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });
    let mut child = cmd.spawn().map_err(|e| format!("cannot start {}: {e}", argv[0]))?;
    if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let _ = pipe.write_all(input.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| format!("quai did not answer: {e}"))?;
    let code = out.status.code().unwrap_or(-1);
    frontend::interpret(&String::from_utf8_lossy(&out.stdout), &String::from_utf8_lossy(&out.stderr), code, used_pkexec)
}

impl qobject::QuaiBackend {
    pub fn run(self: Pin<&mut Self>, request: i32, args_json: &QString, privileged: bool, stdin: &QString) {
        let args: Result<Vec<String>, _> = serde_json::from_str(&args_json.to_string());
        let stdin = stdin.to_string();
        let thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = match args {
                Ok(args) => run_quai(&args, privileged, (!stdin.is_empty()).then_some(stdin)),
                Err(e) => Err(format!("invalid arguments: {e}")),
            };
            let (ok, payload) = match result {
                Ok(json) => (true, json),
                Err(msg) => (false, msg),
            };
            // The window may already be gone; nothing to report to then.
            let _ = thread.queue(move |obj| obj.finished(request, ok, QString::from(&payload)));
        });
    }
}
