import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
Rectangle {
    id: root
    property int type: 0
    property string text
    property list<QtObject> actions
    property bool showCloseButton: false
    visible: false
    implicitHeight: row.implicitHeight + 12
    implicitWidth: 300
    radius: 4
    color: type === 3 ? "#f6d0d6" : type === 2 ? "#fbe7c7" : "#d6e8f6"
    RowLayout {
        id: row
        anchors.fill: parent
        anchors.margins: 6
        QQC2.Label {
            Layout.fillWidth: true
            text: root.text
            wrapMode: Text.WordWrap
        }
        Repeater {
            model: root.actions
            QQC2.Button {
                required property var modelData
                action: modelData
            }
        }
    }
}
