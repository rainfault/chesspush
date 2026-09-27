import QtQuick
import QtQuick.Controls
import ".."
TextField {
    id: control
    implicitHeight: Theme.controlHeight
    font.family: Theme.fontFamily; font.pixelSize: Theme.body
    color: Theme.text; placeholderTextColor: Theme.muted
    selectionColor: Theme.selectionBg; selectedTextColor: Theme.selectionText
    leftPadding: 10; rightPadding: 10; selectByMouse: true
    background: Rectangle { radius: Theme.radius; color: Theme.inputBg; border.color: control.activeFocus ? Theme.focus : Theme.border }
}
