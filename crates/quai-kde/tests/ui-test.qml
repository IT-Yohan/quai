// Drives the real quai-kde pages against a running quai, with the Kirigami stub.
// Run by tests/kde-ui-test.sh.
import QtQuick

QtObject {
    id: driver

    property var win: null
    property int step: 0
    property Timer ticker: Timer {
        interval: 2000
        repeat: true
        running: true
        onTriggered: driver.next()
    }

    function find(item, name) {
        if (!item) {
            return null;
        }
        if (item.objectName === name) {
            return item;
        }
        const kids = item.children || [];
        for (let i = 0; i < kids.length; i++) {
            const r = find(kids[i], name);
            if (r) {
                return r;
            }
        }
        return null;
    }

    function expect(cond, what) {
        if (!cond) {
            console.log("UI TEST FAILED: " + what);
            Qt.exit(1);
        } else {
            console.log("ok - " + what);
        }
    }

    function next() {
        step++;
        const ps = win ? win.pageStack : null;
        switch (step) {
        case 1: {
            const c = Qt.createComponent("qrc:/qt/qml/io/github/quai/kde/qml/Main.qml");
            if (c.status !== Component.Ready) {
                console.log("UI TEST FAILED: " + c.errorString());
                Qt.exit(1);
                return;
            }
            win = c.createObject(null);
            expect(win !== null, "main window created");
            break;
        }
        case 2:
            expect(win.report !== null && win.report.sources.length === 2, "sources list loaded");
            expect(ps.depth === 1 && ps.currentItem.title === "Quai", "sources page is the initial page");
            win.openSource("app");
            break;
        case 3: {
            const d = ps.currentItem;
            expect(ps.depth === 2 && d.s !== null && d.title === "App", "detail page bound to source App");
            expect(d.installed === true, "detail page knows the package is installed");
            win.openSource("variants");
            break;
        }
        case 4:
            expect(ps.depth === 2 && ps.currentItem.sourceId === "variants", "opening another source replaces the detail page");
            win.openAdd();
            break;
        case 5: {
            const page = ps.layers.currentItem;
            expect(page !== null && page.title === "Add a Source", "add page opened as a layer");
            const field = find(page, "projectField");
            expect(field !== null, "project field found");
            field.text = "test/newtool";
            page.analyze();
            break;
        }
        case 6: {
            const page = ps.layers.currentItem;
            expect(page.errorText === "", "preview succeeded");
            expect(page.preview !== null && page.preview.variants.length === 1, "variants detected");
            expect(page.pattern.indexOf("fc43") >= 0, "Fedora variant preselected (" + page.pattern + ")");
            expect(page.shownPackages.length === 1, "only the chosen variant is listed");
            ps.layers.pop();
            win.globalDrawer.actions[3].trigger();
            break;
        }
        case 7: {
            const page = ps.layers.currentItem;
            expect(page.title === "Configure Quai" && page.settings !== null, "preferences loaded");
            expect(page.settings.tokens.length >= 3, "token hosts listed");
            ps.layers.pop();
            win.globalDrawer.actions[2].trigger();
            break;
        }
        case 8: {
            const page = ps.layers.currentItem;
            expect(page.title === "Diagnostics" && page.report !== null && page.report.checks.length > 3, "diagnostics loaded");
            ps.layers.pop();
            win.globalDrawer.actions[4].trigger();
            break;
        }
        case 9:
            expect(ps.layers.currentItem.aboutData.displayName === "Quai", "about page shows the app");
            ps.layers.pop();
            win.refresh("app");
            break;
        case 10:
            expect(win.lastNotification !== "", "refresh reported: " + win.lastNotification);
            console.log("UI TEST PASSED");
            Qt.quit();
            break;
        }
    }
}
