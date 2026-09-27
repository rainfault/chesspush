import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import "../components"

Item {
    ColumnLayout {
        anchors.fill: parent
        spacing: 12

        RowLayout {
            Layout.fillWidth: true
            SectionTitle { text: "Мои партии" }
            Item { Layout.fillWidth: true }
            AppButton { text: "Обновить"; onClicked: myGamesController.refresh() }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 145
            color: "#1b2029"
            border.color: "#2c3442"
            radius: 12
            GridLayout {
                anchors.fill: parent
                anchors.margins: 12
                columns: 5
                columnSpacing: 10
                rowSpacing: 8
                LabeledField { id: importPath; Layout.columnSpan: 3; Layout.fillWidth: true; label: "PGN / NDJSON / ZST файл"; placeholder: "C:/path/games.pgn" }
                AppButton { text: "Импорт"; Layout.alignment: Qt.AlignBottom; onClicked: myGamesController.importGames(importPath.text) }
                AppButton { text: "Скачать Lichess"; Layout.alignment: Qt.AlignBottom; onClicked: myGamesController.downloadLichessGames(parseInt(maxGames.text), perf.text) }
                LabeledField { id: maxGames; label: "Кол-во"; text: "80"; numeric: true }
                LabeledField { id: perf; Layout.columnSpan: 2; Layout.fillWidth: true; label: "Контроли для Lichess"; text: "rapid,blitz" }
                Text { Layout.columnSpan: 2; Layout.fillWidth: true; text: myGamesController.summary; color: "#cbd5e1"; verticalAlignment: Text.AlignVCenter }
            }
        }

        StatusLine { status: myGamesController.status; Layout.fillWidth: true }
        RowsList { Layout.fillWidth: true; Layout.fillHeight: true; rows: myGamesController.rows; emptyText: "Импортируй PGN/NDJSON или скачай партии с Lichess" }
    }
}
