import QtQuick
import ".."
import "../components"
Item {
    id: home
    clip: true
    property date now: new Date()
    onVisibleChanged: if (visible) homeController.refresh()
    Rectangle {
        anchors.fill: parent; color: Theme.windowBg
        Rectangle {
            y: -76; width: parent.width; height: 300
            gradient: Gradient {
                GradientStop { position: 0; color: Theme.bodyGradient }
                GradientStop { position: 1; color: Theme.windowBg }
            }
        }
    }
    Timer { interval: 1000; running: parent.visible; repeat: true; triggeredOnStart: true; onTriggered: parent.now = new Date() }
    UiLabel {
        id: clock
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.min(parent.height * .43, parent.height - 340) - height / 2
        text: Qt.formatDateTime(parent.now,"HH:mm"); font.pixelSize: 96; font.weight: Font.Light
    }
    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        y: clock.y + clock.height + Theme.spaceXL
        spacing: Math.min(370, home.width * .27)
        HomeRating { symbol: "\ue002"; speedName: "Rapid"; rating: homeController.rapid.rating; progress: homeController.rapid.progress; showProgress: homeController.rapid.showProgress }
        HomeRating { symbol: "\ue02f"; speedName: "Blitz"; rating: homeController.blitz.rating; progress: homeController.blitz.progress; showProgress: homeController.blitz.showProgress }
    }
}
