// Drives the real quai-kde window and saves screenshots of it.
// tests/screenshots/run.sh fills in @OUT@ and @SUFFIX@ and loads this file
// through QUAI_KDE_MAIN, offscreen, with the Breeze style and colours.
import QtQuick
import QtQuick.Window
import io.github.quai.kde

QtObject {
    id: driver

    readonly property string out: "@OUT@"
    readonly property string suffix: "@SUFFIX@"
    property var win: null
    property int step: 0
    property int pending: 0
    property Timer ticker: Timer {
        interval: 2500
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

    function shot(name) {
        // The window's content: Kirigami's page header and the pages, layers included
        // (the root item above it is not the QML engine's, so it cannot be grabbed).
        const root = win.contentItem;
        const dpr = win.screen ? win.screen.devicePixelRatio : Screen.devicePixelRatio;
        const path = out + "/" + name + suffix + ".png";
        pending++;
        root.grabToImage(r => {
            r.saveToFile(path);
            console.log("shot " + path);
            pending--;
        }, Qt.size(root.width * dpr, root.height * dpr));
    }

    function fail(what) {
        console.log("SHOTS FAILED: " + what);
        Qt.exit(1);
    }

    function next() {
        step++;
        const ps = win ? win.pageStack : null;
        switch (step) {
        case 1: {
            const c = Qt.createComponent("qrc:/qt/qml/io/github/quai/kde/qml/Main.qml");
            if (c.status !== Component.Ready) {
                fail(c.errorString());
                return;
            }
            win = c.createObject(null, {"width": 900, "height": 620});
            win.show();
            break;
        }
        case 2:
            if (!win.report || win.report.sources.length < 4) {
                fail("sources not loaded");
                return;
            }
            shot("plasma-sources");
            break;
        case 3:
            win.openSource("vscodium");
            break;
        case 4:
            shot("plasma-source");
            break;
        case 5: {
            win.openAdd();
            break;
        }
        case 6: {
            const page = ps.layers.currentItem;
            const field = find(page, "projectField");
            if (!field) {
                fail("no project field");
                return;
            }
            field.text = "pulsar-edit/pulsar";
            page.analyze();
            break;
        }
        case 7:
            shot("plasma-add");
            break;
        case 8:
            if (pending === 0) {
                console.log("SHOTS DONE");
                Qt.quit();
            }
            break;
        default:
            if (pending === 0) {
                console.log("SHOTS DONE");
                Qt.quit();
            }
        }
    }
}
