import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ".."
import "../components"
Popup {
    id: dialog
    width: 410; padding: Theme.spaceL; modal: true; focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onOpened: {
        username.text = settingsController.lichessUsername
        token.text = settingsController.lichessApiToken
        theme.currentIndex = settingsController.theme === "dark" ? 1 : 0
        errorLabel.text = ""
    }
    onClosed: Theme.setTheme(settingsController.theme)
    background: Rectangle { color: Theme.surface; radius: Theme.radius }
    Overlay.modal: Rectangle { color: "#66000000" }
    contentItem: ColumnLayout {
        spacing: Theme.spaceM
        RowLayout {
            UiLabel { text: "Настройки"; font.pixelSize: Theme.title; Layout.fillWidth: true }
            UiButton { text: "×"; flatStyle: true; Accessible.name: "Закрыть"; onClicked: dialog.close() }
        }
        UiLabel { text: "Ник Lichess" }
        UiField { id: username; Layout.fillWidth: true; Accessible.name: "Ник Lichess" }
        UiLabel { text: "Токен" }
        UiField { id: token; Layout.fillWidth: true; echoMode: TextInput.Password; Accessible.name: "Токен" }
        UiLabel { text: "Тема" }
        UiSelect { id: theme; Layout.fillWidth: true; model: ["Светлая","Тёмная"]; onActivated: Theme.setTheme(currentIndex === 1 ? "dark" : "light") }
        UiLabel { id: errorLabel; Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.error; visible: text.length > 0 }
        UiButton {
            text: "Сохранить"; primary: true; Layout.fillWidth: true; Layout.topMargin: Theme.spaceS
            onClicked: {
                if (settingsController.saveProfile(username.text,token.text,theme.currentIndex === 1 ? "dark" : "light")) dialog.close()
                else errorLabel.text = settingsController.status
            }
        }
    }
}
