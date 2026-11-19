// One source: packages, settings, status, actions.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import io.github.quai.kde

Kirigami.ScrollablePage {
    id: page

    required property var app
    required property string sourceId

    /** Current status of this source; follows every reload. */
    readonly property var s: app.sourceById(sourceId)
    readonly property bool installed: s !== null && s.packages.some(p => p.installed)
    readonly property bool updatable: s !== null && s.packages.some(p => p.update_available)

    title: s ? s.source.name : ""

    /** The source was found at least once (not merely loading). */
    property bool seen: false

    // The source disappeared (removed elsewhere): go back to the list, once the
    // change has settled (popping inside the binding update would loop on `s`).
    onSChanged: {
        if (s !== null) {
            seen = true;
        } else if (seen && app.report !== null) {
            Qt.callLater(() => {
                if (app.pageStack.currentItem === page) {
                    app.pageStack.pop();
                }
            });
        }
    }

    actions: [
        Kirigami.Action {
            text: I18n.trc("verb", "Update")
            icon.name: "update-high"
            visible: page.updatable
            onTriggered: page.app.act(["upgrade", page.sourceId])
        },
        Kirigami.Action {
            text: I18n.tr("Install")
            icon.name: "download"
            visible: page.s !== null && !page.installed
            onTriggered: page.app.act(["install", page.sourceId])
        },
        Kirigami.Action {
            text: I18n.tr("Check Now")
            icon.name: "view-refresh"
            onTriggered: page.app.refresh(page.sourceId)
        },
        Kirigami.Action {
            text: I18n.tr("Remove Source…")
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
            text: page.s && page.s.last_error ? I18n.trf("Last check failed: {error}", {"error": page.s.last_error}) : ""
        }

        Repeater {
            model: page.s ? page.s.excluded : []
            delegate: Kirigami.InlineMessage {
                required property var modelData
                Layout.fillWidth: true
                visible: true
                type: Kirigami.MessageType.Warning
                text: I18n.trf("{package} ({file}) is not served: {reason}",
                               {"package": modelData.name, "file": modelData.file, "reason": modelData.reason})
            }
        }

        Kirigami.FormLayout {
            Layout.fillWidth: true

            QQC2.Label {
                Kirigami.FormData.label: I18n.tr("Project:")
                text: page.s ? "<a href=\"" + page.s.source.web_url + "\">" + page.s.source.host + "/" + page.s.source.project + "</a>" : ""
                textFormat: Text.StyledText
                onLinkActivated: link => Qt.openUrlExternally(link)
            }

            QQC2.Label {
                Kirigami.FormData.label: I18n.tr("State:")
                text: page.s ? page.app.stateLabel(page.s.state) : ""
                color: page.s ? page.app.stateColor(page.s.state) : Kirigami.Theme.textColor
            }

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: I18n.tr("Packages")
            }

            Repeater {
                model: page.s ? page.s.packages : []
                delegate: QQC2.Label {
                    required property var modelData
                    Kirigami.FormData.label: modelData.name + ":"
                    text: {
                        const p = modelData;
                        if (p.installed && p.update_available) {
                            return I18n.trf("Installed {installed} · {available} available",
                                            {"installed": p.installed, "available": p.available});
                        }
                        if (p.installed) {
                            return I18n.trf("Installed {version}", {"version": p.installed});
                        }
                        return p.available ? I18n.trf("Not installed · {version} available", {"version": p.available})
                                           : I18n.trc("package state", "Not installed");
                    }
                }
            }

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: I18n.tr("Source")
            }

            QQC2.Switch {
                Kirigami.FormData.label: I18n.tr("Enabled:")
                text: checked ? I18n.tr("dnf uses this source") : I18n.tr("dnf ignores this source")
                checked: page.s ? page.s.source.enabled : false
                onToggled: page.app.act([checked ? "enable" : "disable", page.sourceId])
            }

            QQC2.ComboBox {
                Kirigami.FormData.label: I18n.tr("Channel:")
                model: [I18n.tr("Stable releases"), I18n.tr("Pre-releases too")]
                currentIndex: page.s && page.s.source.channel === "prerelease" ? 1 : 0
                onActivated: page.app.act(["set", page.sourceId, "channel", currentIndex === 1 ? "prerelease" : "stable"])
            }

            RowLayout {
                Kirigami.FormData.label: I18n.tr("Versions kept:")

                QQC2.SpinBox {
                    id: keep
                    from: 1
                    to: 20
                    value: page.s ? page.s.source.keep_versions : 3
                }
                QQC2.Button {
                    text: I18n.tr("Apply")
                    visible: page.s !== null && keep.value !== page.s.source.keep_versions
                    onClicked: page.app.act(["set", page.sourceId, "keep-versions", String(keep.value)])
                }
            }

            RowLayout {
                Kirigami.FormData.label: I18n.tr("Asset pattern:")

                QQC2.TextField {
                    id: pattern
                    Layout.fillWidth: true
                    placeholderText: I18n.tr("Regular expression, optional")
                    text: page.s && page.s.source.asset_pattern ? page.s.source.asset_pattern : ""
                }
                QQC2.Button {
                    text: I18n.tr("Apply")
                    visible: page.s !== null && pattern.text !== (page.s.source.asset_pattern || "")
                    onClicked: page.app.act(["set", page.sourceId, "asset-pattern", pattern.text])
                }
            }

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: I18n.tr("Status")
            }

            QQC2.Label {
                Kirigami.FormData.label: I18n.tr("Latest release:")
                text: page.s && page.s.latest_release ? page.s.latest_release : "—"
            }

            QQC2.Label {
                Kirigami.FormData.label: I18n.tr("Last check:")
                text: page.s ? page.app.relativeTime(page.s.last_check) : ""
            }

            QQC2.Label {
                Kirigami.FormData.label: I18n.tr("dnf repository:")
                text: "quai-" + page.sourceId
            }

            RowLayout {
                Kirigami.FormData.label: I18n.tr("Repository file:")
                visible: page.s !== null && !page.s.repo_file_in_sync

                QQC2.Label {
                    text: I18n.tr("out of date")
                    color: Kirigami.Theme.negativeTextColor
                }
                QQC2.Button {
                    text: I18n.tr("Regenerate")
                    onClicked: page.app.act(["sync"])
                }
            }
        }
    }

    Kirigami.PromptDialog {
        id: removeDialog

        title: page.s ? I18n.trf("Remove {name}?", {"name": page.s.source.name}) : ""
        subtitle: page.installed ? I18n.tr("Its packages stay installed but stop receiving updates, unless you uninstall them too.")
                                 : I18n.tr("dnf will stop seeing this source.")
        standardButtons: Kirigami.Dialog.NoButton
        customFooterActions: [
            Kirigami.Action {
                text: I18n.tr("Remove and Uninstall")
                icon.name: "edit-delete"
                visible: page.installed
                onTriggered: {
                    removeDialog.close();
                    page.app.act(["remove", page.sourceId, "--uninstall"]);
                }
            },
            Kirigami.Action {
                text: I18n.tr("Remove")
                icon.name: "edit-delete-remove"
                onTriggered: {
                    removeDialog.close();
                    page.app.act(["remove", page.sourceId]);
                }
            },
            Kirigami.Action {
                text: I18n.tr("Cancel")
                icon.name: "dialog-cancel"
                onTriggered: removeDialog.close()
            }
        ]
    }
}
