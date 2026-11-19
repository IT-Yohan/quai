import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
ColumnLayout {
    id: root
    component IconGroup: QtObject {
        property string name
        property var source
    }
    property string text
    property string explanation
    property QtObject helpfulAction: null
    property IconGroup icon: IconGroup {}
    QQC2.Label {
        text: root.text
        font.bold: true
    }
    QQC2.Label {
        Layout.fillWidth: true
        text: root.explanation
        wrapMode: Text.WordWrap
    }
    QQC2.Button {
        visible: root.helpfulAction !== null
        action: root.helpfulAction
    }
}
