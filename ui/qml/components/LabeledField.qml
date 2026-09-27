import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ".."

ColumnLayout {
    id: root
    property string label: ""
    property alias text: input.text
    property string placeholder: ""
    property bool numeric: false
    property int echoMode: TextInput.Normal

    spacing: 4

    Text {
        text: root.label
        color: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: 12
        elide: Text.ElideRight
        Layout.fillWidth: true
    }

    TextField {
        id: input
        Layout.fillWidth: true
        placeholderText: root.placeholder
        color: Theme.text
        placeholderTextColor: Theme.faint
        font.family: Theme.fontFamily
        inputMethodHints: root.numeric ? Qt.ImhDigitsOnly : Qt.ImhNone
        echoMode: root.echoMode
        selectedTextColor: Theme.selectionText
        selectionColor: Theme.selectionBg
        background: Rectangle {
            radius: 8
            color: Theme.inputBg
            border.color: input.activeFocus ? Theme.focus : Theme.border
        }
    }
}
