import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "."
import "components"
import "pages"
ApplicationWindow {
    id: window
    width: 1380; height: 900; minimumWidth: 1000; minimumHeight: 700
    visible: true; title: "ChessPush"; color: Theme.windowBg
    font.family: Theme.fontFamily; font.pixelSize: Theme.body
    property int page: 0
    Component.onCompleted: Theme.setTheme(settingsController.theme)
    Connections { target: settingsController; function onSettingsChanged() { Theme.setTheme(settingsController.theme) } }
    background: Rectangle {
        color: Theme.windowBg
        Rectangle { width: parent.width; height: 300
            gradient: Gradient {
                GradientStop { position: 0; color: Theme.bodyGradient }
                GradientStop { position: 1; color: Theme.windowBg }
            }
        }
    }
    header: Item {
        height: 76
        RowLayout {
            anchors.fill: parent; anchors.leftMargin: Theme.spaceXL; anchors.rightMargin: Theme.spaceXL
            spacing: Theme.spaceL
            UiLabel { text: "ChessPush"; font.pixelSize: 30; font.weight: Font.Light; Layout.rightMargin: Theme.spaceM }
            UiButton {
                objectName: "homeButton"; flatStyle: true; selected: window.page === 0
                Accessible.name: "Главная"; onClicked: window.page = 0
                Image { anchors.centerIn: parent; source: "assets/home.svg"; width: 24; height: 24 }
            }
            UiButton { objectName: "parserButton"; text: "ZST-парсер"; flatStyle: true; selected: window.page === 1; onClicked: window.page = 1 }
            Item { Layout.fillWidth: true }
            UiButton {
                objectName: "settingsButton"; flatStyle: true; Accessible.name: "Настройки"
                onClicked: settings.open()
                Image { anchors.centerIn: parent; source: "assets/settings.svg"; width: 24; height: 24 }
            }
        }
    }
    // Keep WebEngine laid out and painted behind Home, so the first navigation
    // doesn't start a renderer or resize an uninitialized Chessground surface.
    ParserPage { anchors.fill: parent; anchors.margins: Theme.spaceL; anchors.topMargin: Theme.spaceS; enabled: window.page === 1 }
    HomePage { anchors.fill: parent; visible: window.page === 0 }
    SettingsDialog { id: settings; anchors.centerIn: Overlay.overlay }
}
