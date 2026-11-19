// Main window of quai-kde. Every page receives `app` (this window) and talks to
// quai through app.call / app.act; nothing here touches the system directly.
import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import io.github.quai.kde

Kirigami.ApplicationWindow {
    id: root

    title: "Quai"
    width: Kirigami.Units.gridUnit * 44
    height: Kirigami.Units.gridUnit * 36
    minimumWidth: Kirigami.Units.gridUnit * 22
    minimumHeight: Kirigami.Units.gridUnit * 20

    /** Last `quai status` document, null until loaded. */
    property var report: null
    property string loadError: ""
    /** Number of quai commands in flight. */
    property int busy: 0
    property var pending: ({})
    property int nextRequest: 1

    QuaiBackend {
        id: backend
        onFinished: (request, ok, payload) => {
            const callback = root.pending[request];
            delete root.pending[request];
            root.busy = Math.max(0, root.busy - 1);
            if (!callback) {
                return;
            }
            if (!ok) {
                callback(false, payload);
                return;
            }
            let data = null;
            try {
                data = JSON.parse(payload);
            } catch (e) {
                callback(false, "Unexpected answer from quai: " + e);
                return;
            }
            callback(true, data);
        }
    }

    /** Run `quai <args> --json`; callback(ok, documentOrErrorMessage). */
    function call(args, privileged, stdin, callback) {
        const request = nextRequest++;
        pending[request] = callback;
        busy += 1;
        backend.run(request, JSON.stringify(args), privileged, stdin || "");
    }

    function reload() {
        call(["status"], false, "", (ok, data) => {
            if (ok) {
                report = data;
                loadError = "";
            } else {
                loadError = data;
            }
        });
    }

    /** Run a command that returns an ActionResult (privileged unless told otherwise), notify, reload. */
    function act(args, privileged, stdin) {
        const asRoot = privileged === undefined ? true : privileged;
        call(args, asRoot, stdin || "", (ok, data) => {
            if (!ok) {
                showPassiveNotification(data, "long");
            } else if (data.ok) {
                showPassiveNotification(data.message + (data.reboot_required ? " — restart to apply" : ""));
            } else {
                showPassiveNotification(data.message, "long");
            }
            reload();
        });
    }

    /** Ask quaid to check the forges now (one source, or all when id is empty). */
    function refresh(id) {
        call(id ? ["refresh", id] : ["refresh"], false, "", (ok, data) => {
            if (!ok) {
                showPassiveNotification(data, "long");
            } else if (data.errors.length > 0) {
                showPassiveNotification(data.errors.map(e => e[0] + ": " + e[1]).join("\n"), "long");
            } else if (data.refreshed.length === 0) {
                showPassiveNotification("Checked less than 30 seconds ago");
            } else {
                showPassiveNotification(data.refreshed.length === 1 ? "Checked for new releases"
                                                                    : "Checked " + data.refreshed.length + " sources");
            }
            reload();
        });
    }

    function sourceById(id) {
        if (!report) {
            return null;
        }
        for (const s of report.sources) {
            if (s.source.id === id) {
                return s;
            }
        }
        return null;
    }

    function stateLabel(state) {
        switch (state) {
        case "up_to_date": return "Up to date";
        case "update_available": return "Update available";
        case "not_installed": return "Not installed";
        case "never_checked": return "Not checked yet";
        case "error": return "Error";
        case "disabled": return "Disabled";
        }
        return state;
    }

    function stateIcon(state) {
        switch (state) {
        case "up_to_date": return "emblem-success";
        case "update_available": return "update-high";
        case "not_installed": return "download";
        case "never_checked": return "view-refresh";
        case "error": return "emblem-error";
        case "disabled": return "media-playback-pause";
        }
        return "package-x-generic";
    }

    function stateColor(state) {
        switch (state) {
        case "up_to_date": return Kirigami.Theme.positiveTextColor;
        case "update_available": return Kirigami.Theme.linkColor;
        case "error": return Kirigami.Theme.negativeTextColor;
        }
        return Kirigami.Theme.disabledTextColor;
    }

    /** "host/project · 1.0-1 → 1.1-1" */
    function subtitle(s) {
        const parts = [s.source.host + "/" + s.source.project];
        const p = s.packages.length > 0 ? s.packages[0] : null;
        if (p && p.update_available) {
            parts.push((p.installed || "?") + " → " + (p.available || "?"));
        } else if (p && p.installed) {
            parts.push(p.installed);
        } else if (p && p.available) {
            parts.push(p.available + " available");
        }
        return parts.join(" · ");
    }

    function relativeTime(iso) {
        if (!iso) {
            return "never";
        }
        const seconds = Math.max(0, (Date.now() - Date.parse(iso)) / 1000);
        const plural = (n, unit) => n === 1 ? "1 " + unit + " ago" : n + " " + unit + "s ago";
        if (seconds < 60) {
            return "just now";
        }
        if (seconds < 3600) {
            return plural(Math.floor(seconds / 60), "minute");
        }
        if (seconds < 86400) {
            return plural(Math.floor(seconds / 3600), "hour");
        }
        return plural(Math.floor(seconds / 86400), "day");
    }

    function humanSize(bytes) {
        if (bytes === null || bytes === undefined) {
            return "";
        }
        const mb = bytes / 1048576;
        return mb >= 1 ? mb.toFixed(1) + " MB" : Math.round(bytes / 1024) + " kB";
    }

    function openSource(id) {
        while (pageStack.depth > 1) {
            pageStack.pop();
        }
        pageStack.push(sourcePage, { sourceId: id });
    }

    function openAdd() {
        pageStack.layers.push(addPage);
    }

    globalDrawer: Kirigami.GlobalDrawer {
        isMenu: true
        actions: [
            Kirigami.Action {
                text: "Add Source…"
                icon.name: "list-add"
                shortcut: StandardKey.New
                onTriggered: root.openAdd()
            },
            Kirigami.Action {
                text: "Check All Sources Now"
                icon.name: "view-refresh"
                shortcut: StandardKey.Refresh
                onTriggered: root.refresh("")
            },
            Kirigami.Action {
                text: "Diagnostics"
                icon.name: "tools-report-bug"
                onTriggered: root.pageStack.layers.push(diagnosticsPage)
            },
            Kirigami.Action {
                text: "Configure Quai…"
                icon.name: "configure"
                shortcut: StandardKey.Preferences
                onTriggered: root.pageStack.layers.push(preferencesPage)
            },
            Kirigami.Action {
                text: "About Quai"
                icon.name: "help-about"
                onTriggered: root.pageStack.layers.push(aboutPage)
            }
        ]
    }

    pageStack.initialPage: SourcesPage {
        app: root
    }
    pageStack.globalToolBar.showNavigationButtons: Kirigami.ApplicationHeaderStyle.ShowBackButton

    Component {
        id: sourcePage
        SourcePage {
            app: root
        }
    }

    Component {
        id: addPage
        AddSourcePage {
            app: root
        }
    }

    Component {
        id: preferencesPage
        PreferencesPage {
            app: root
        }
    }

    Component {
        id: diagnosticsPage
        DiagnosticsPage {
            app: root
        }
    }

    Component {
        id: aboutPage
        Kirigami.AboutPage {
            aboutData: {
                "displayName": "Quai",
                "productName": "quai",
                "componentName": "quai-kde",
                "shortDescription": "Keeps RPM packages published as GitHub, GitLab or Forgejo releases up to date through dnf.",
                "homepage": "https://github.com/quai-rpm/quai",
                "bugAddress": "https://github.com/quai-rpm/quai/issues",
                "version": Qt.application.version,
                "otherText": "",
                "authors": [{ "name": "The quai contributors", "task": "", "emailAddress": "", "webAddress": "https://github.com/quai-rpm/quai", "ocsUsername": "" }],
                "credits": [],
                "translators": [],
                "licenses": [{ "name": "Apache License 2.0", "text": "Licensed under the Apache License, Version 2.0.", "spdx": "Apache-2.0" }],
                "copyrightStatement": "© The quai contributors",
                "desktopFileName": "io.github.quai.Quai"
            }
        }
    }

    Component.onCompleted: reload()
}
