import QtQuick
import QtQuick.Controls
import ".."
Button {
    id: control
    property bool primary: false
    property bool flatStyle: false
    property bool selected: false
    implicitHeight: Theme.controlHeight
    implicitWidth: Math.max(38,label.implicitWidth + 24)
    padding: 8
    opacity: enabled ? 1 : .4
    font.family: Theme.fontFamily
    font.pixelSize: Theme.body
    contentItem: Text {
        id: label
        text: control.text; font: control.font
        color: control.primary ? Theme.accentText : (control.selected ? Theme.accent : Theme.text)
        horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    background: Rectangle {
        radius: Theme.radius
        color: control.primary ? (control.hovered ? Theme.accentHover : Theme.accent) : control.down ? Theme.buttonDown : control.hovered ? Theme.buttonHover : control.flatStyle ? "transparent" : Theme.buttonBg
        border.width: control.activeFocus ? 1 : 0
        border.color: Theme.focus
        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 2; color: Theme.accent; visible: control.selected }
    }
}
