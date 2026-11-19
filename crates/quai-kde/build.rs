use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("io.github.quai.kde").qml_files([
        "qml/Main.qml",
        "qml/SourcesPage.qml",
        "qml/SourcePage.qml",
        "qml/AddSourcePage.qml",
        "qml/PreferencesPage.qml",
        "qml/DiagnosticsPage.qml",
    ]))
    .qt_module("Gui")
    .qt_module("Widgets")
    .qt_module("Quick")
    .qt_module("QuickControls2")
    .cpp_file("cpp/app.cpp")
    .files(["src/backend.rs"])
    .build();
}
