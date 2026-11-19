// One source: packages, settings, status, actions.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.ScrollablePage {
    id: page

    required property var app
    required property string sourceId

    /** Current status of this source; follows every reload. */
    readonly property var s: app.sourceById(sourceId)
    readonly property bool installed: s !== null && s.packages.some(p => p.installed)
    readonly property bool updatable: s !== null && s.packages.some(p => p.update_available)

    title: s ? s.source.name : ""

    // The source disappeared (removed elsewhere): go back to the list.
    onSChanged: {
        if (s === null && app.report !== null) {
            app.pageStack.pop();
        }
    }

    actions: [
        Kirigami.Action {
            text: "Update"
            icon.name: "update-high"
            visible: page.updatable
            onTriggered: page.app.act(["upgrade", page.sourceId])
        },
        Kirigami.Action {
            text: "Install"
            icon.name: "download"
            visible: page.s !== null && !page.installed
            onTriggered: page.app.act(["install", page.sourceId])
        },
        Kirigami.Action {
            text: "Check Now"
            icon.name: "view-refresh"
            onTriggered: page.app.refresh(page.sourceId)
        },
        Kirigami.Action {
            text: "Remove Source…"
            icon.name: "edit-delete"
            displayHint: Kirigami.DisplayHint.AlwaysHide
            onTriggered: removeDialog.open()
        }
    ]

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: page.s !== null && page.s.last_error !== null
            type: Kirigami.MessageType.Error
            text: page.s && page.s.last_error ? "Last check failed: " + page.s.last_error : ""
        }

        Repeater {
            model: page.s ? page.s.excluded : []
            delegate: Kirigami.InlineMessage {
                required property var modelData
                Layout.fillWidth: true
                visible: true
                type: Kirigami.MessageType.Warning
                text: modelData.name + " (" + modelData.file + ") is not served: " + modelData.reason
            }
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true

            QQC2.Label {
                Kirigami.FormData.label: "Project:"
                text: page.s ? "<a href=\"" + page.s.source.web_url + "\">" + page.s.source.host + "/" + page.s.source.project + "</a>" : ""
                textFormat: Text.StyledText
                onLinkActivated: link => Qt.openUrlExternally(link)
            }

            QQC2.Label {
                Kirigami.FormData.label: "State:"
                text: page.s ? page.app.stateLabel(page.s.state) : ""
                color: page.s ? page.app.stateColor(page.s.state) : Kirigami.Theme.textColor
            }

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: "Packages"
            }

            Repeater {
                model: page.s ? page.s.packages : []
                delegate: QQC2.Label {
                    required property var modelData
                    Kirigami.FormData.label: modelData.name + ":"
                    text: {
                        const p = modelData;
                        if (p.installed && p.update_available) {
                            return "Installed " + p.installed + " · " + p.available + " available";
                        }
                        if (p.installed) {
                            return "Installed " + p.installed;
                        }
                        return p.available ? "Not installed · " + p.available + " available" : "Not installed";
                    }
                }
            }

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: "Source"
            }

            QQC2.Switch {
                Kirigami.FormData.label: "Enabled:"
                text: checked ? "dnf uses this source" : "dnf ignores this source"
                checked: page.s ? page.s.source.enabled : false
                onToggled: page.app.act([checked ? "enable" : "disable", page.sourceId])
            }

            QQC2.ComboBox {
                Kirigami.FormData.label: "Channel:"
                model: ["Stable releases", "Pre-releases too"]
                currentIndex: page.s && page.s.source.channel === "prerelease" ? 1 : 0
                onActivated: page.app.act(["set", page.sourceId, "channel", currentIndex === 1 ? "prerelease" : "stable"])
            }

            RowLayout {
                Kirigami.FormData.label: "Versions kept:"

                QQC2.SpinBox {
                    id: keep
                    from: 1
                    to: 20
                    value: page.s ? page.s.source.keep_versions : 3
                }
                QQC2.Button {
                    text: "Apply"
                    visible: page.s !== null && keep.value !== page.s.source.keep_versions
                    onClicked: page.app.act(["set", page.sourceId, "keep-versions", String(keep.value)])
                }
            }

            RowLayout {
                Kirigami.FormData.label: "Asset pattern:"

                QQC2.TextField {
                    id: pattern
                    Layout.fillWidth: true
                    placeholderText: "Regular expression, optional"
                    text: page.s && page.s.source.asset_pattern ? page.s.source.asset_pattern : ""
                }
                QQC2.Button {
                    text: "Apply"
                    visible: page.s !== null && pattern.text !== (page.s.source.asset_pattern || "")
                    onClicked: page.app.act(["set", page.sourceId, "asset-pattern", pattern.text])
                }
            }

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: "Status"
            }

            QQC2.Label {
                Kirigami.FormData.label: "Latest release:"
                text: page.s && page.s.latest_release ? page.s.latest_release : "—"
            }

            QQC2.Label {
                Kirigami.FormData.label: "Last check:"
                text: page.s ? page.app.relativeTime(page.s.last_check) : ""
            }

            QQC2.Label {
                Kirigami.FormData.label: "dnf repository:"
                text: "quai-" + page.sourceId
            }

            RowLayout {
                Kirigami.FormData.label: "Repository file:"
                visible: page.s !== null && !page.s.repo_file_in_sync

                QQC2.Label {
                    text: "out of date"
                    color: Kirigami.Theme.negativeTextColor
                }
                QQC2.Button {
                    text: "Regenerate"
                    onClicked: page.app.act(["sync"])
                }
            }
        }
    }

    Kirigami.PromptDialog {
        id: removeDialog

        title: page.s ? "Remove " + page.s.source.name + "?" : ""
        subtitle: page.installed ? "Its packages stay installed but stop receiving updates, unless you uninstall them too."
                                 : "dnf will stop seeing this source."
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: "Remove and Uninstall"
                icon.name: "edit-delete"
                visible: page.installed
                onTriggered: {
                    removeDialog.close();
                    page.app.act(["remove", page.sourceId, "--uninstall"]);
                }
            },
            Kirigami.Action {
                text: "Remove"
                icon.name: "edit-delete-remove"
                onTriggered: {
                    removeDialog.close();
                    page.app.act(["remove", page.sourceId]);
                }
            },
            Kirigami.Action {
                text: "Cancel"
                icon.name: "dialog-cancel"
                onTriggered: removeDialog.close()
            }
        ]
    }
}
