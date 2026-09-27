import QtQuick
import QtQuick.Controls
import ".."

CheckBox {
    id: control

    implicitHeight: 32
    spacing: 8

    indicator: Rectangle {
        implicitWidth: 22
        implicitHeight: 22
        x: control.leftPadding
        y: (control.height - height) / 2
        radius: 5
        color: control.checked ? Theme.accent : Theme.inputBg
        border.color: control.checked ? Theme.accentHover : Theme.borderStrong

        Text {
            anchors.centerIn: parent
            text: control.checked ? "✓" : ""
            color: Theme.accentText
            font.family: Theme.fontFamily
            font.pixelSize: 16
            font.bold: true
        }
    }

    contentItem: Text {
        text: control.text
        color: Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: 13
        verticalAlignment: Text.AlignVCenter
        leftPadding: control.indicator.width + control.spacing
        elide: Text.ElideRight
    }
}
