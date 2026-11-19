//! The QML side of quai-kde's Rust code:
//!
//! * `QuaiBackend` runs `quai --json` on a worker thread and hands the result
//!   back to QML through the `finished` signal;
//! * `I18n` is the translation singleton: the same gettext catalogue and the
//!   same functions as the Rust code (`I18n.tr("…")`, `I18n.trf("… {x}", {x: v})`,
//!   `I18n.trn("{n} …", "{n} …", n)`), so `po/extract.py` reads QML too.

use cxx_qt::Threading;
use cxx_qt_lib::{QMap, QMapPair_QString_QVariant, QString};
use quai_i18n::trf;
use quai_types::frontend;
use std::fmt::Display;
use std::io::Write;
use std::pin::Pin;
use std::process::{Command, Stdio};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qmap.h");
        type QMap_QString_QVariant = cxx_qt_lib::QMap<cxx_qt_lib::QMapPair_QString_QVariant>;
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

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        type I18n = super::I18nRust;

        /// A fixed message.
        #[qinvokable]
        fn tr(self: &I18n, msgid: &QString) -> QString;

        /// A message whose meaning depends on `context`.
        #[qinvokable]
        fn trc(self: &I18n, context: &QString, msgid: &QString) -> QString;

        /// A message with `{name}` placeholders, filled from the `args` object.
        #[qinvokable]
        fn trf(self: &I18n, msgid: &QString, args: &QMap_QString_QVariant) -> QString;

        /// A message that depends on the count `n`, which fills `{n}`.
        #[qinvokable]
        fn trn(self: &I18n, singular: &QString, plural: &QString, n: i32) -> QString;

        /// [`trn`] with more placeholders than `{n}`.
        #[qinvokable]
        fn trnf(self: &I18n, singular: &QString, plural: &QString, n: i32, args: &QMap_QString_QVariant) -> QString;
    }
}

#[derive(Default)]
pub struct QuaiBackendRust;

#[derive(Default)]
pub struct I18nRust;

/// QML's `{key: value}` object as placeholder values.
fn placeholders(args: &QMap<QMapPair_QString_QVariant>) -> Vec<(String, String)> {
    args.iter().map(|(k, v)| (k.to_string(), v.value::<QString>().map(|s| s.to_string()).unwrap_or_default())).collect()
}

fn with_placeholders<R>(args: &QMap<QMapPair_QString_QVariant>, f: impl FnOnce(&[(&str, &dyn Display)]) -> R) -> R {
    let owned = placeholders(args);
    let refs: Vec<(&str, &dyn Display)> = owned.iter().map(|(k, v)| (k.as_str(), v as &dyn Display)).collect();
    f(&refs)
}

fn count(n: i32) -> u64 {
    u64::try_from(n).unwrap_or(0)
}

impl qobject::I18n {
    pub fn tr(&self, msgid: &QString) -> QString {
        QString::from(&quai_i18n::tr(&msgid.to_string()))
    }

    pub fn trc(&self, context: &QString, msgid: &QString) -> QString {
        QString::from(&quai_i18n::trc(&context.to_string(), &msgid.to_string()))
    }

    pub fn trf(&self, msgid: &QString, args: &QMap<QMapPair_QString_QVariant>) -> QString {
        QString::from(&with_placeholders(args, |a| quai_i18n::trf(&msgid.to_string(), a)))
    }

    pub fn trn(&self, singular: &QString, plural: &QString, n: i32) -> QString {
        QString::from(&quai_i18n::trn(&singular.to_string(), &plural.to_string(), count(n)))
    }

    pub fn trnf(
        &self,
        singular: &QString,
        plural: &QString,
        n: i32,
        args: &QMap<QMapPair_QString_QVariant>,
    ) -> QString {
        let text =
            with_placeholders(args, |a| quai_i18n::trnf(&singular.to_string(), &plural.to_string(), count(n), a));
        QString::from(&text)
    }
}

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
    let mut child = cmd.spawn().map_err(|e| trf("cannot start quai: {error}", &[("error", &e)]))?;
    if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let _ = pipe.write_all(input.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| trf("quai did not answer: {error}", &[("error", &e)]))?;
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
                Err(e) => Err(format!("invalid arguments: {e}")), // a bug in the QML, not for users
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
