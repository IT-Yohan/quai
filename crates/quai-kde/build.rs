use cxx_qt_build::{CxxQtBuilder, QmlModule};

/// The pages, embedded as plain Qt resources at the module's path
/// (qrc:/qt/qml/io/github/quai/kde/qml/). Qt compiles them when the window
/// opens, instead of qmlcachegen ahead of time: AOT-compiled QML calls
/// QQmlPrivate internals that change between Qt minor versions, which Fedora
/// ships as regular updates, and a package built outside Fedora is not rebuilt
/// with them. Six small pages cost a few milliseconds at startup.
const PAGES: [&str; 6] = [
    "qml/Main.qml",
    "qml/SourcesPage.qml",
    "qml/SourcePage.qml",
    "qml/AddSourcePage.qml",
    "qml/PreferencesPage.qml",
    "qml/DiagnosticsPage.qml",
];

fn main() {
    for page in PAGES {
        println!("cargo::rerun-if-changed={page}");
    }
    CxxQtBuilder::new_qml_module(QmlModule::new("io.github.quai.kde"))
        .qt_module("Gui")
        .qt_module("Widgets")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qrc_resources(PAGES)
        .cpp_file("cpp/app.cpp")
        .files(["src/backend.rs"])
        .build();
}
