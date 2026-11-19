// Global settings and forge tokens.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.ScrollablePage {
    id: page

    required property var app

    property var settings: null
    property string tokenHost: ""

    title: "Configure Quai"

    function load() {
        app.call(["config"], false, "", (ok, data) => {
            if (ok) {
                settings = data;
            } else {
                app.showPassiveNotification(data, "long");
            }
        });
    }

    Component.onCompleted: load()

    Kirigami.FormLayout {
        enabled: page.settings !== null

        Kirigami.Separator {
            Kirigami.FormData.isSection: true
            Kirigami.FormData.label: "Checking for releases"
        }

        QQC2.SpinBox {
            id: interval
            Kirigami.FormData.label: "Minimum interval (minutes):"
            from: 1
            to: 1440
            stepSize: 5
            value: page.settings ? page.settings.check_interval_mins : 10
        }

        QQC2.SpinBox {
            id: keep
            Kirigami.FormData.label: "Versions kept for new sources:"
            from: 1
            to: 20
            value: page.settings ? page.settings.keep_versions : 3
        }

        QQC2.Button {
            text: "Apply"
            icon.name: "dialog-ok-apply"
            enabled: page.settings !== null
                     && (interval.value !== page.settings.check_interval_mins || keep.value !== page.settings.keep_versions)
            onClicked: {
                if (interval.value !== page.settings.check_interval_mins) {
                    page.app.act(["config", "check-interval-mins", String(interval.value)]);
                }
                if (keep.value !== page.settings.keep_versions) {
                    page.app.act(["config", "keep-versions", String(keep.value)]);
                }
                page.load();
            }
        }

        Kirigami.Separator {
            Kirigami.FormData.isSection: true
            Kirigami.FormData.label: "Access tokens"
        }

        QQC2.Label {
            Layout.maximumWidth: Kirigami.Units.gridUnit * 24
            wrapMode: Text.WordWrap
            font: Kirigami.Theme.smallFont
            opacity: 0.7
            text: "Without a token GitHub allows 60 requests per hour; a token raises that to 5,000 and needs no permissions for public projects. Tokens are readable by root and quaid only."
        }

        Repeater {
            model: page.settings ? page.settings.tokens : []
            delegate: RowLayout {
                required property var modelData
                Kirigami.FormData.label: modelData.host + ":"

                QQC2.Label {
                    text: modelData.configured ? "Token set" : "Optional"
                    opacity: 0.7
                }
                QQC2.Button {
                    text: "Set…"
                    onClicked: {
                        page.tokenHost = modelData.host;
                        tokenField.text = "";
                        tokenDialog.open();
                    }
                }
                QQC2.Button {
                    icon.name: "edit-delete"
                    display: QQC2.AbstractButton.IconOnly
                    text: "Remove token"
                    QQC2.ToolTip.text: text
                    QQC2.ToolTip.visible: hovered
                    onClicked: page.app.act(["token", "clear", "--host", modelData.host])
                }
            }
        }

        Kirigami.Separator {
            Kirigami.FormData.isSection: true
            Kirigami.FormData.label: "Service"
        }

        QQC2.Label {
            Kirigami.FormData.label: "quaid address:"
            text: page.settings ? page.settings.listen : ""
        }

        QQC2.Label {
            Kirigami.FormData.label: "Signed metadata:"
            text: page.settings && page.settings.sign_metadata ? "On: dnf verifies every repomd.xml" : "Off"
        }
    }

    Kirigami.PromptDialog {
        id: tokenDialog

        title: "Token for " + page.tokenHost
        standardButtons: Kirigami.Dialog.Ok | Kirigami.Dialog.Cancel

        // A content item replaces PromptDialog's subtitle, so the explanation lives here.
        ColumnLayout {
            spacing: Kirigami.Units.smallSpacing

            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                text: "Paste a personal access token. It is stored in /etc/quai/private/tokens.toml."
            }
            QQC2.TextField {
                id: tokenField
                Layout.fillWidth: true
                echoMode: TextInput.Password
                placeholderText: "Token"
            }
        }

        onAccepted: {
            if (tokenField.text.trim() !== "") {
                page.app.act(["token", "set", "--host", page.tokenHost], true, tokenField.text.trim() + "\n");
                page.load();
            }
        }
    }
}
