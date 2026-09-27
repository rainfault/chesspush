import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ".."
import "../components"

Item {
    ColumnLayout {
        anchors.fill: parent
        spacing: 12

        RowLayout {
            Layout.fillWidth: true
            SectionTitle { text: "Настройки"; Layout.fillWidth: true }
            AppButton { text: "Обновить"; onClicked: settingsController.refresh() }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 430
            color: Theme.panelAlt
            border.color: Theme.border
            radius: 7

            GridLayout {
                anchors.fill: parent
                anchors.margins: 12
                columns: 4
                columnSpacing: 10
                rowSpacing: 8

                LabeledField { id: username; Layout.fillWidth: true; label: "Lichess username"; text: settingsController.lichessUsername }
                LabeledField { id: minRating; Layout.fillWidth: true; label: "Min model rating"; text: settingsController.defaultMinRating.toString(); numeric: true }
                LabeledField { id: maxRating; Layout.fillWidth: true; label: "Max model rating"; text: settingsController.defaultMaxRating.toString(); numeric: true }
                LabeledField { id: limit; Layout.fillWidth: true; label: "Default limit"; text: settingsController.defaultLimit.toString(); numeric: true }

                ColumnLayout {
                    Layout.columnSpan: 2
                    Layout.fillWidth: true
                    spacing: 4
                    Text {
                        text: "Тема приложения"
                        color: Theme.muted
                        font.family: Theme.fontFamily
                        font.pixelSize: 12
                    }
                    AppComboBox {
                        id: themeChoice
                        Layout.fillWidth: true
                        model: ["dark", "lichess"]
                        currentIndex: settingsController.theme === "lichess" ? 1 : 0
                    }
                }

                LabeledField {
                    id: minMoves
                    Layout.fillWidth: true
                    label: "Min moves"
                    text: settingsController.defaultMinMoves.toString()
                    numeric: true
                }

                AppCheckBox {
                    id: excludeBullet
                    text: "Исключать bullet"
                    checked: settingsController.excludeBullet
                    Layout.alignment: Qt.AlignBottom
                }

                LabeledField {
                    id: token
                    Layout.columnSpan: 4
                    Layout.fillWidth: true
                    label: "Lichess API token"
                    text: settingsController.lichessApiToken
                    placeholder: "optional personal access token"
                    echoMode: TextInput.Password
                }

                LabeledField { id: zstPath; Layout.columnSpan: 4; Layout.fillWidth: true; label: "Путь к PGN/ZST базе"; text: settingsController.zstDatabasePath }
                LabeledField { id: exportFolder; Layout.columnSpan: 2; Layout.fillWidth: true; label: "Папка экспорта"; text: settingsController.exportFolder }
                LabeledField { id: storagePath; Layout.columnSpan: 2; Layout.fillWidth: true; label: "SQLite storage"; text: settingsController.localStoragePath }

                Item { Layout.columnSpan: 3; Layout.fillWidth: true }

                AppButton {
                    text: "Сохранить"
                    primary: true
                    Layout.fillWidth: true
                    onClicked: settingsController.saveSettings(
                        username.text,
                        token.text,
                        themeChoice.currentText,
                        zstPath.text,
                        exportFolder.text,
                        storagePath.text,
                        parseInt(minRating.text),
                        parseInt(maxRating.text),
                        parseInt(minMoves.text),
                        excludeBullet.checked,
                        parseInt(limit.text)
                    )
                }
            }
        }

        StatusLine { status: settingsController.status; Layout.fillWidth: true }
        RowsList { Layout.fillWidth: true; Layout.fillHeight: true; rows: settingsController.rows; emptyText: "Нет настроек" }
    }
}
