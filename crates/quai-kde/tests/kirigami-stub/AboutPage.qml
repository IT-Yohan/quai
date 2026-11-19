import QtQuick
import QtQuick.Controls as QQC2
Page {
    property var aboutData
    QQC2.Label {
        text: aboutData ? aboutData.displayName + " " + aboutData.version : ""
    }
}
