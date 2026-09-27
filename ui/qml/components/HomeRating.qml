import QtQuick
import ".."
Item {
    id: root
    required property string symbol
    required property string rating
    required property real progress
    required property bool showProgress
    required property string speedName
    implicitWidth: 230
    implicitHeight: 170
    Accessible.role: Accessible.StaticText
    Accessible.name: speedName + (rating ? " " + rating : "")
    Column {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: parent.top
        spacing: Theme.spaceS
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.symbol
            color: Theme.muted
            font.family: "lichess"
            font.pixelSize: 54
            renderType: Text.NativeRendering
        }
        UiLabel {
            anchors.horizontalCenter: parent.horizontalCenter
            text: root.rating
            font.pixelSize: 34
            visible: text.length > 0
        }
    }
    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        width: 200; height: 5; radius: height / 2
        visible: root.showProgress && root.rating.length > 0
        color: Theme.surfaceLow
        Rectangle {
            height: parent.height
            width: parent.width * root.progress
            radius: parent.radius
            color: Theme.ratingProgressColor(root.progress)
            Behavior on width { NumberAnimation { duration: 250 } }
            Behavior on color { ColorAnimation { duration: 250 } }
        }
    }
}
