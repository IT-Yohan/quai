#pragma once

#include <QtCore/QString>
#include <cstdint>

// Creates the QApplication and the QML engine, loads Main.qml, runs the event loop.
std::int32_t run_app(const QString& version);
