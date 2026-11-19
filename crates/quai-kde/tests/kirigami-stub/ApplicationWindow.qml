import QtQuick
import QtQuick.Controls as QQC2
QQC2.ApplicationWindow {
    id: win
    property QtObject globalDrawer: null
    property alias pageStack: stack
    property string lastNotification: ""
    function showPassiveNotification(message, timeout) {
        lastNotification = message;
        console.log("NOTIFICATION: " + message);
    }
    PageRow {
        id: stack
        anchors.fill: parent
    }
}
