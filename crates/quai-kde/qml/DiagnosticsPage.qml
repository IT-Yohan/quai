// `quai doctor`, with one-click fixes for the common problems.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import io.github.quai.kde

Kirigami.ScrollablePage {
    id: page

    required property var app

    property var report: null
    property string errorText: ""

    title: I18n.tr("Diagnostics")

    function load() {
        report = null;
        errorText = "";
        app.call(["doctor"], false, "", (ok, data) => {
            if (ok) {
                report = data;
            } else {
                errorText = data;
            }
        });
    }

    function failed(ids) {
        return report !== null && report.checks.some(c => !c.ok && ids.indexOf(c.id) >= 0);
    }

    Component.onCompleted: load()

    actions: [
        Kirigami.Action {
            text: I18n.tr("Run Setup")
            icon.name: "system-run"
            visible: page.failed(["config", "signing-key", "socket", "daemon"])
            onTriggered: {
                page.app.act(["setup"]);
                page.app.pageStack.layers.pop();
            }
        },
        Kirigami.Action {
            text: I18n.tr("Regenerate Repository Files")
            icon.name: "view-refresh"
            visible: page.failed(["repo-files"])
            onTriggered: {
                page.app.act(["sync"]);
                page.app.pageStack.layers.pop();
            }
        },
        Kirigami.Action {
            text: I18n.tr("Run Again")
            icon.name: "view-refresh"
            onTriggered: page.load()
        }
    ]

    ListView {
        id: list

        model: page.report ? page.report.checks : []

        delegate: QQC2.ItemDelegate {
            required property var modelData

            width: ListView.view.width
            hoverEnabled: false

            contentItem: RowLayout {
                spacing: Kirigami.Units.largeSpacing

                Kirigami.Icon {
                    Layout.alignment: Qt.AlignTop
                    source: modelData.ok ? "emblem-success" : "emblem-warning"
                    implicitWidth: Kirigami.Units.iconSizes.smallMedium
                    implicitHeight: Kirigami.Units.iconSizes.smallMedium
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0

                    QQC2.Label {
                        Layout.fillWidth: true
                        text: modelData.title
                        font.bold: !modelData.ok
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        text: modelData.detail + (!modelData.ok && modelData.fix
                            ? "\n" + I18n.trf("Fix: {command}", {"command": modelData.fix}) : "")
                        wrapMode: Text.WordWrap
                        font: Kirigami.Theme.smallFont
                        opacity: 0.7
                    }
                }
            }
        }

        QQC2.BusyIndicator {
            anchors.centerIn: parent
            visible: page.report === null && page.errorText === ""
            running: visible
        }

        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.gridUnit * 4
            visible: page.errorText !== ""
            icon.name: "dialog-error"
            text: I18n.tr("Diagnostics Failed")
            explanation: page.errorText
        }
    }
}
