import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ".."

Rectangle {
    id: root
    property string title: ""
    property string value: ""
    property string hint: ""

    color: Theme.panelAlt
    border.color: Theme.border
    radius: 7
    implicitHeight: 100

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 14
        spacing: 6
        Text { text: root.title; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12; elide: Text.ElideRight; Layout.fillWidth: true }
        Text { text: root.value; color: Theme.textStrong; font.family: Theme.fontFamily; font.pixelSize: 22; font.bold: true; elide: Text.ElideRight; Layout.fillWidth: true }
        Text { text: root.hint; color: Theme.faint; font.family: Theme.fontFamily; font.pixelSize: 11; visible: root.hint.length > 0; elide: Text.ElideRight; Layout.fillWidth: true }
    }
}
