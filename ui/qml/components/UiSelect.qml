import QtQuick
import QtQuick.Controls
import ".."
ComboBox {
    id: control
    implicitHeight: Theme.controlHeight; leftPadding: 10; rightPadding: 26
    font.family: Theme.fontFamily; font.pixelSize: Theme.body
    contentItem: UiLabel { text: control.displayText; elide: Text.ElideRight }
    indicator: UiLabel { text: "⌄"; x: control.width - 22; y: (control.height - height)/2; color: Theme.accent }
    background: Rectangle { color: Theme.inputBg; radius: Theme.radius; border.color: control.activeFocus ? Theme.focus : Theme.border }
    delegate: ItemDelegate {
        required property var modelData
        required property int index
        width: control.width; height: Theme.controlHeight
        highlighted: control.highlightedIndex === index
        contentItem: UiLabel { text: control.textRole ? modelData[control.textRole] : modelData; color: parent.highlighted ? Theme.selectionText : Theme.text; elide: Text.ElideRight }
        background: Rectangle { color: parent.highlighted ? Theme.accent : Theme.surface }
    }
    popup: Popup {
        y: control.height; width: control.width; padding: 1
        implicitHeight: Math.min(contentItem.implicitHeight,320)
        contentItem: ListView {
            clip: true; implicitHeight: contentHeight
            model: control.popup.visible ? control.delegateModel : null
            currentIndex: control.highlightedIndex
            ScrollBar.vertical: ScrollBar {}
        }
        background: Rectangle { color: Theme.surface; border.color: Theme.border; radius: Theme.radius }
    }
}
