import QtQuick
import QtQuick.Controls
import ".."

ComboBox {
    id: control

    implicitHeight: 38
    leftPadding: 12
    rightPadding: 34

    delegate: ItemDelegate {
        width: control.width
        height: 34
        text: control.textRole && typeof modelData === "object" ? modelData[control.textRole] : modelData
        highlighted: control.highlightedIndex === index
        contentItem: Text {
            text: parent.text
            color: parent.highlighted ? Theme.selectionText : Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: 13
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
        background: Rectangle {
            color: parent.highlighted ? Theme.selectionBg : (parent.hovered ? Theme.buttonHover : Theme.inputBg)
        }
    }

    indicator: Text {
        x: control.width - width - 12
        y: (control.height - height) / 2
        text: "⌄"
        color: Theme.focus
        font.family: Theme.fontFamily
        font.pixelSize: 18
    }

    contentItem: Text {
        leftPadding: 0
        rightPadding: control.indicator.width + control.spacing
        text: control.displayText
        color: Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: 13
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    background: Rectangle {
        radius: 8
        color: Theme.inputBg
        border.color: control.activeFocus ? Theme.focus : Theme.borderStrong
    }

    popup: Popup {
        y: control.height + 4
        width: control.width
        implicitHeight: Math.min(contentItem.implicitHeight + 8, 280)
        padding: 4
        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: control.popup.visible ? control.delegateModel : null
            currentIndex: control.highlightedIndex
            ScrollIndicator.vertical: ScrollIndicator { }
        }
        background: Rectangle {
            radius: 8
            color: Theme.inputBg
            border.color: Theme.borderStrong
        }
    }
}
