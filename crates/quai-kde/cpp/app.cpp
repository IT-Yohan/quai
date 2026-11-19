#include "quai-kde/cpp/app.h"

#include <QApplication>
#include <QIcon>
#include <QLibraryInfo>
#include <QLocale>
#include <QQmlApplicationEngine>
#include <QQuickStyle>
#include <QTranslator>
#include <QUrl>

std::int32_t run_app(const QString& version)
{
    // QApplication (not QGuiApplication): the Plasma style for Qt Quick Controls
    // (org.kde.desktop) draws through QStyle, which needs QtWidgets.
    static int argc = 1;
    static char arg0[] = "quai-kde";
    static char* argv[] = { arg0, nullptr };
    QApplication app(argc, argv);

    QCoreApplication::setOrganizationName(QStringLiteral("quai"));
    QCoreApplication::setApplicationName(QStringLiteral("quai-kde"));
    QCoreApplication::setApplicationVersion(version);
    QGuiApplication::setApplicationDisplayName(QStringLiteral("Quai"));
    QGuiApplication::setDesktopFileName(QStringLiteral("io.github.quai.Quai"));
    QApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("io.github.quai.Quai")));

    // quai's own texts come from gettext (the I18n singleton). Qt's built-in
    // texts come from Qt's catalogues, which also turn the layout right-to-left
    // for languages written that way. Kirigami loads its own catalogue itself.
    static QTranslator qtTranslator;
    if (qtTranslator.load(QLocale(), QStringLiteral("qtbase"), QStringLiteral("_"),
                          QLibraryInfo::path(QLibraryInfo::TranslationsPath))) {
        QCoreApplication::installTranslator(&qtTranslator);
    }

    // Same rule as every Kirigami application: Breeze controls unless the user chose otherwise.
    if (qEnvironmentVariableIsEmpty("QT_QUICK_CONTROLS_STYLE")) {
        QQuickStyle::setStyle(QStringLiteral("org.kde.desktop"));
    }

    QQmlApplicationEngine engine;
    // QUAI_KDE_MAIN loads another QML file (development and tests).
    const QString override = qEnvironmentVariable("QUAI_KDE_MAIN");
    engine.load(override.isEmpty() ? QUrl(QStringLiteral("qrc:/qt/qml/io/github/quai/kde/qml/Main.qml"))
                                   : QUrl::fromLocalFile(override));
    if (engine.rootObjects().isEmpty()) {
        return 1;
    }
    return app.exec();
}
