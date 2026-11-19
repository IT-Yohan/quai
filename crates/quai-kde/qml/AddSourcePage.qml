// "Add a Source": type a project, see what it ships, confirm.
import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import io.github.quai.kde

Kirigami.ScrollablePage {
    id: page

    required property var app

    /** Last `quai preview` document, null while on the first step. */
    property var preview: null
    /** Asset pattern chosen among the variants, or "". */
    property string pattern: ""
    property bool working: false
    property string errorText: ""

    readonly property var forges: ["", "github", "gitlab", "forgejo"]
    readonly property var variant: preview && preview.variants.length > 0 ? preview.variants[0] : null
    /** Packages to show: those of the chosen variant only. */
    readonly property var shownPackages: {
        if (!preview) {
            return [];
        }
        if (!variant) {
            return preview.packages;
        }
        const index = variant.patterns.indexOf(pattern);
        const chosen = index >= 0 ? variant.assets[index] : variant.assets[0];
        return preview.packages.filter(p => variant.assets.indexOf(p.asset_name) < 0 || p.asset_name === chosen);
    }

    title: I18n.tr("Add a Source")

    function selectionArgs() {
        const args = [project.text.trim()];
        const forge = forges[forgeBox.currentIndex];
        if (forge !== "") {
            args.push("--forge", forge);
        }
        args.push("--channel", channelBox.currentIndex === 1 ? "prerelease" : "stable");
        if (pattern !== "") {
            args.push("--asset-pattern", pattern);
        }
        return args;
    }

    /** Prefer a Fedora build among variants (".fc43", "fedora"), else the first file. */
    function defaultPattern(v) {
        for (let i = 0; i < v.assets.length; i++) {
            const name = v.assets[i].toLowerCase();
            if (name.indexOf("fedora") >= 0 || /\.fc\d/.test(name)) {
                return v.patterns[i];
            }
        }
        return v.patterns[0];
    }

    function analyze() {
        working = true;
        errorText = "";
        pattern = "";
        app.call(["preview"].concat(selectionArgs()), false, "", (ok, data) => {
            working = false;
            if (!ok) {
                errorText = data;
                return;
            }
            preview = data;
            if (data.variants.length > 0) {
                pattern = defaultPattern(data.variants[0]);
            }
            nameField.text = data.suggested_name;
            idField.text = data.suggested_id;
            const hostPackages = data.packages.filter(p => p.arch === data.host_arch || p.arch === "noarch");
            installBox.checked = !hostPackages.every(p => p.installed);
        });
    }

    function add() {
        const args = ["add"].concat(selectionArgs());
        args.push("--id", idField.text.trim(), "--name", nameField.text.trim());
        if (installBox.checked && installBox.enabled) {
            args.push("--install");
        }
        working = true;
        errorText = "";
        app.call(args, true, "", (ok, data) => {
            working = false;
            if (!ok) {
                errorText = data;
                return;
            }
            if (!data.ok) {
                errorText = app.sentence(data.message);
                return;
            }
            app.notify(data.message + (data.reboot_required ? " — " + I18n.tr("restart to finish installing") : ""));
            app.reload();
            app.pageStack.layers.pop();
        });
    }

    footer: QQC2.ToolBar {
        contentItem: RowLayout {
            QQC2.Button {
                text: I18n.tr("Back")
                icon.name: "go-previous"
                visible: page.preview !== null
                enabled: !page.working
                onClicked: {
                    page.preview = null;
                    page.errorText = "";
                }
            }
            Item {
                Layout.fillWidth: true
            }
            QQC2.BusyIndicator {
                running: page.working
                visible: running
                implicitHeight: Kirigami.Units.iconSizes.medium
                implicitWidth: implicitHeight
            }
            QQC2.Button {
                text: page.preview === null ? I18n.tr("Analyze") : I18n.trc("button", "Add")
                icon.name: page.preview === null ? "search" : "list-add"
                enabled: !page.working && project.text.trim() !== ""
                highlighted: true
                onClicked: page.preview === null ? page.analyze() : page.add()
            }
        }
    }

    ColumnLayout {
        spacing: Kirigami.Units.largeSpacing

        Kirigami.InlineMessage {
            Layout.fillWidth: true
            visible: page.errorText !== ""
            type: Kirigami.MessageType.Error
            text: page.errorText
        }

        // Step 1: the project.
        Kirigami.FormLayout {
            Layout.fillWidth: true
            visible: page.preview === null

            QQC2.TextField {
                id: project
                objectName: "projectField"
                Kirigami.FormData.label: I18n.tr("Project:")
                placeholderText: I18n.tr("owner/repo or project URL")
                focus: true
                onAccepted: if (text.trim() !== "" && !page.working) page.analyze()
            }

            QQC2.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 24
                text: I18n.tr("A GitHub, GitLab or Codeberg project that attaches .rpm files to its releases, e.g. stablyai/orca.")
                wrapMode: Text.WordWrap
                font: Kirigami.Theme.smallFont
                opacity: 0.7
            }

            QQC2.ComboBox {
                id: forgeBox
                Kirigami.FormData.label: I18n.tr("Forge:")
                model: [I18n.tr("Detect automatically"), "GitHub", "GitLab", "Forgejo / Gitea"]
            }

            QQC2.ComboBox {
                id: channelBox
                Kirigami.FormData.label: I18n.tr("Channel:")
                model: [I18n.tr("Stable releases"), I18n.tr("Pre-releases too")]
            }
        }

        // Step 2: what the project ships.
        Kirigami.FormLayout {
            Layout.fillWidth: true
            visible: page.preview !== null

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: page.preview ? I18n.trf("Release {tag}", {"tag": page.preview.release.tag}) : ""
            }

            QQC2.Label {
                Kirigami.FormData.label: I18n.tr("Project:")
                text: page.preview ? page.preview.host + "/" + page.preview.project : ""
            }

            Repeater {
                model: page.shownPackages
                delegate: QQC2.Label {
                    required property var modelData
                    Kirigami.FormData.label: modelData.name + " " + modelData.evr + ":"
                    Layout.maximumWidth: Kirigami.Units.gridUnit * 24
                    wrapMode: Text.WordWrap
                    text: {
                        const p = modelData;
                        let line = p.arch + (p.size ? " · " + page.app.humanSize(p.size) : "");
                        if (p.summary) {
                            line += " · " + p.summary;
                        }
                        if (p.installed) {
                            line += "\n" + I18n.trf("Installed {version}, will be kept up to date", {"version": p.installed});
                        }
                        return line;
                    }
                }
            }

            QQC2.ComboBox {
                id: variantBox
                Kirigami.FormData.label: I18n.tr("Variant:")
                visible: page.variant !== null
                model: page.variant ? page.variant.assets : []
                currentIndex: page.variant ? Math.max(0, page.variant.patterns.indexOf(page.pattern)) : 0
                onActivated: index => page.pattern = page.variant.patterns[index]
            }

            QQC2.Label {
                visible: page.variant !== null
                Layout.maximumWidth: Kirigami.Units.gridUnit * 24
                wrapMode: Text.WordWrap
                font: Kirigami.Theme.smallFont
                opacity: 0.7
                text: page.variant ? I18n.trf("Several files provide {package} for {arch}. quai follows the one you pick in every future release.",
                                              {"package": page.variant.name, "arch": page.variant.arch}) : ""
            }

            Kirigami.Separator {
                Kirigami.FormData.isSection: true
                Kirigami.FormData.label: I18n.tr("Options")
            }

            QQC2.TextField {
                id: nameField
                Kirigami.FormData.label: I18n.tr("Display name:")
            }

            QQC2.TextField {
                id: idField
                Kirigami.FormData.label: I18n.tr("Source id:")
            }

            QQC2.CheckBox {
                id: installBox
                Kirigami.FormData.label: I18n.tr("Install:")
                text: enabled ? I18n.tr("Install now") : I18n.tr("Already installed")
                enabled: page.preview !== null
                         && !page.preview.packages.filter(p => p.arch === page.preview.host_arch || p.arch === "noarch").every(p => p.installed)
            }
        }

        // Warnings, below the form so they read as notes on what was found.
        Repeater {
            model: page.preview ? page.preview.warnings.filter(w => w.code !== "multiple_variants") : []
            delegate: Kirigami.InlineMessage {
                required property var modelData
                Layout.fillWidth: true
                visible: true
                type: modelData.severity === "danger" ? Kirigami.MessageType.Error
                      : modelData.severity === "warning" ? Kirigami.MessageType.Warning
                      : Kirigami.MessageType.Information
                text: page.app.sentence(modelData.message)
            }
        }
    }
}
