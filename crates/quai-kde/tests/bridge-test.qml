// Exercises QuaiBackend with a real Qt event loop, without Kirigami.
// Run by tests/kde-bridge.sh: QUAI_KDE_MAIN=<this file> quai-kde
import QtQuick
import io.github.quai.kde

QtObject {
    id: test

    property int remaining: 4
    property var backend: QuaiBackend {
        onFinished: (request, ok, payload) => test.check(request, ok, payload)
    }

    function check(request, ok, payload) {
        if (request === 1) {
            // status: a StatusReport document.
            const doc = JSON.parse(payload);
            console.log("status ok=" + ok + " sources=" + doc.sources.length + " daemon=" + doc.daemon_reachable);
            if (!ok || doc.sources.length < 1) fail("status");
        } else if (request === 2) {
            // An unknown source: quai's ErrorReport becomes ok=false + message.
            console.log("missing ok=" + ok + " message=" + payload);
            if (ok || payload.indexOf("no source named") < 0) fail("error mapping");
        } else if (request === 3) {
            // doctor exits non-zero but its document must still arrive.
            const doc = JSON.parse(payload);
            console.log("doctor ok=" + ok + " checks=" + doc.checks.length);
            if (!ok || doc.checks.length < 3) fail("doctor");
        } else if (request === 4) {
            // stdin is passed through (token set reads it), privileged path without pkexec as root.
            console.log("token ok=" + ok + " payload=" + payload.replace(/\s+/g, " "));
            if (!ok || JSON.parse(payload).ok !== true) fail("stdin");
        }
        remaining -= 1;
        if (remaining === 0) {
            console.log("BRIDGE TEST PASSED");
            Qt.quit();
        }
    }

    function fail(what) {
        console.log("BRIDGE TEST FAILED: " + what);
        Qt.exit(1);
    }

    Component.onCompleted: {
        backend.run(1, JSON.stringify(["status"]), false, "");
        backend.run(2, JSON.stringify(["status", "does-not-exist"]), false, "");
        backend.run(3, JSON.stringify(["doctor"]), false, "");
        backend.run(4, JSON.stringify(["token", "set", "--host", "example.org"]), true, "test-token\n");
    }
}
