import QtQuick
import QtQuick.Controls
import ".."

Button {
    id: control

    property bool primary: false

    implicitHeight: 38
    leftPadding: 14
    rightPadding: 14

    contentItem: Text {
        text: control.text
        color: {
            if (!control.enabled) return Theme.faint
            if (control.primary) return Theme.accentText
            return Theme.text
        }
        font.family: Theme.fontFamily
        font.pixelSize: 13
        font.bold: control.primary
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    background: Rectangle {
        radius: 7
        color: {
            if (!control.enabled) return Theme.progressBg
            if (control.down) return control.primary ? Theme.accentDown : Theme.buttonDown
            if (control.hovered) return control.primary ? Theme.accentHover : Theme.buttonHover
            return control.primary ? Theme.accent : Theme.buttonBg
        }
        border.color: control.primary ? Theme.accentHover : Theme.borderStrong
    }
}
