import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"

Item {
    ColumnLayout {
        anchors.fill: parent
        spacing: 14

        RowLayout {
            Layout.fillWidth: true
            SectionTitle { text: "Dashboard" }
            Item { Layout.fillWidth: true }
            AppButton { text: "Обновить"; onClicked: dashboardController.refresh() }
        }

        GridLayout {
            Layout.fillWidth: true
            columns: 4
            rowSpacing: 12
            columnSpacing: 12
            Repeater {
                model: dashboardController.rows
                delegate: InfoCard {
                    Layout.fillWidth: true
                    title: modelData.title
                    value: modelData.value
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 120
            color: "#1b2029"
            border.color: "#2c3442"
            radius: 12
            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 16
                Text { text: "Рекомендация дня"; color: "#9ca3af"; font.pixelSize: 12 }
                Text {
                    Layout.fillWidth: true
                    text: dashboardController.recommendation
                    color: "#f3f4f6"
                    font.pixelSize: 18
                    wrapMode: Text.WordWrap
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            color: "#151922"
            border.color: "#2c3442"
            radius: 12
            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 16
                Text { text: "Рабочий цикл"; color: "#f3f4f6"; font.pixelSize: 18; font.bold: true }
                Text { Layout.fillWidth: true; color: "#cbd5e1"; font.pixelSize: 14; wrapMode: Text.WordWrap; text: "1) Импортируй свои партии. 2) Открой статистику и найди PainIndex. 3) В Model Finder достань сильные партии по теме. 4) Создай Training Card. 5) Сыграй 2 rapid-партии с одной задачей." }
                StatusLine { status: dashboardController.status; Layout.fillWidth: true }
            }
        }
    }
}
