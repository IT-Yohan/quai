// List of tracked sources.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import io.github.quai.kde

Kirigami.ScrollablePage {
    id: page

    required property var app

    title: "Quai"

    actions: [
        Kirigami.Action {
            text: I18n.tr("Add Source…")
            icon.name: "list-add"
            onTriggered: page.app.openAdd()
        },
        Kirigami.Action {
            text: I18n.tr("Check Now")
            icon.name: "view-refresh"
            enabled: page.app.busy === 0
            onTriggered: page.app.refresh("")
        }
    ]

    header: Kirigami.InlineMessage {
        visible: page.app.report !== null && page.app.report.sources.length > 0 && !page.app.report.daemon_reachable
        type: Kirigami.MessageType.Warning
        text: I18n.tr("The quai service is not answering, so dnf and Discover cannot see your sources.")
        actions: [
            Kirigami.Action {
                text: I18n.tr("Set Up")
                icon.name: "system-run"
                onTriggered: page.app.act(["setup"])
            }
        ]
    }

    ListView {
        id: list

        model: page.app.report ? page.app.report.sources : []
        currentIndex: -1

        delegate: QQC2.ItemDelegate {
            id: row

            required property var modelData

            width: ListView.view.width
            onClicked: page.app.openSource(row.modelData.source.id)

            contentItem: RowLayout {
                spacing: Kirigami.Units.largeSpacing

                Kirigami.Icon {
                    source: page.app.stateIcon(row.modelData.state)
                    implicitWidth: Kirigami.Units.iconSizes.medium
                    implicitHeight: Kirigami.Units.iconSizes.medium
                }

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0

                    QQC2.Label {
                        Layout.fillWidth: true
                        text: row.modelData.source.name
                        elide: Text.ElideRight
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        text: page.app.subtitle(row.modelData)
                        elide: Text.ElideRight
                        font: Kirigami.Theme.smallFont
                        opacity: 0.7
                    }
                }

                QQC2.Label {
                    text: page.app.stateLabel(row.modelData.state)
                    color: page.app.stateColor(row.modelData.state)
                }

                Kirigami.Icon {
                    source: "go-next"
                    implicitWidth: Kirigami.Units.iconSizes.small
                    implicitHeight: Kirigami.Units.iconSizes.small
                }
            }
        }

        QQC2.BusyIndicator {
            anchors.centerIn: parent
            visible: page.app.report === null && page.app.loadError === ""
            running: visible
        }

        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.gridUnit * 4
            visible: page.app.report !== null && list.count === 0
            icon.name: "download"
            text: I18n.tr("No Sources Yet")
            explanation: I18n.tr("Add a GitHub, GitLab or Codeberg project that publishes RPM packages. dnf and Discover will then keep it up to date.")
            helpfulAction: Kirigami.Action {
                text: I18n.tr("Add a Source")
                icon.name: "list-add"
                onTriggered: page.app.openAdd()
            }
        }

        Kirigami.PlaceholderMessage {
            anchors.centerIn: parent
            width: parent.width - Kirigami.Units.gridUnit * 4
            visible: page.app.loadError !== ""
            icon.name: "dialog-error"
            text: I18n.tr("Cannot Read the Sources")
            explanation: page.app.loadError
        }
    }
}
