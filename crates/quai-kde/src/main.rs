//! quai-kde: KDE Plasma frontend for quai, written with Kirigami.
//!
//! Like quai-gtk it never touches the system itself: QML asks [`backend`] to run
//! `quai ... --json` (through pkexec for changes) and renders the JSON it gets back.

mod backend;

fn main() {
    quai_i18n::init();
    cxx_qt::init_crate!(cxx_qt_lib);
    cxx_qt::init_qml_module!("io.github.quai.kde");
    let version = cxx_qt_lib::QString::from(env!("CARGO_PKG_VERSION"));
    std::process::exit(backend::qobject::run_app(&version));
}
