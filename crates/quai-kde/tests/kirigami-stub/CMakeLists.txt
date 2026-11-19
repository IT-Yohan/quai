cmake_minimum_required(VERSION 3.16)
project(kirigamistub LANGUAGES CXX)
find_package(Qt6 REQUIRED COMPONENTS Qml Quick)
set(CMAKE_AUTOMOC ON)
set(QT_QML_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/qml)
set_source_files_properties(Theme.qml Units.qml
    PROPERTIES QT_QML_SINGLETON_TYPE TRUE)
qt_add_qml_module(kirigamistub
    URI org.kde.kirigami
    VERSION 2.0
    PLUGIN_TARGET kirigamistub
    SOURCES formdata.h formdata.cpp
    QML_FILES
        ApplicationWindow.qml PageRow.qml GlobalDrawer.qml Page.qml ScrollablePage.qml Action.qml
        Icon.qml Separator.qml FormLayout.qml InlineMessage.qml PlaceholderMessage.qml
        Dialog.qml PromptDialog.qml AboutPage.qml
        Theme.qml Units.qml
)
