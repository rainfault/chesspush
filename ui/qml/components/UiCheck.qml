import QtQuick
import QtQuick.Controls
import ".."
CheckBox {
    id: control
    implicitHeight: 34; padding: 0
    font.family: Theme.fontFamily; font.pixelSize: Theme.body
    indicator: Rectangle {
        x: 0; y: (control.height - height)/2; width: 18; height: 18; radius: Theme.radius
        color: control.checked ? Theme.success : Theme.inputBg
        border.color: control.activeFocus ? Theme.focus : control.checked ? Theme.success : Theme.border
        Text { anchors.centerIn: parent; text: "✓"; color: "white"; visible: control.checked; font.pixelSize: 14 }
    }
    contentItem: UiLabel { text: control.text; leftPadding: 28 }
}
