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
            SectionTitle { text: "Поиск модельных партий" }
            Item { Layout.fillWidth: true }
            Text { text: modelFinderController.engineInfo; color: "#9ca3af"; font.pixelSize: 12 }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 245
            color: "#1b2029"
            border.color: "#2c3442"
            radius: 12
            GridLayout {
                anchors.fill: parent
                anchors.margins: 12
                columns: 6
                columnSpacing: 10
                rowSpacing: 8
                LabeledField { id: dbPath; Layout.columnSpan: 3; Layout.fillWidth: true; label: "База PGN/ZST"; text: settingsController.zstDatabasePath }
                LabeledField { id: fen; Layout.columnSpan: 3; Layout.fillWidth: true; label: "FEN позиции"; placeholder: "опционально" }
                LabeledField { id: eco; label: "ECO"; placeholder: "B12" }
                LabeledField { id: opening; Layout.columnSpan: 2; Layout.fillWidth: true; label: "Opening contains"; placeholder: "London" }
                AppComboBox { id: perf; model: ["any", "rapid", "blitz", "classical"]; Layout.fillWidth: true }
                LabeledField { id: minRating; label: "Min rating"; text: settingsController.defaultMinRating.toString(); numeric: true }
                LabeledField { id: maxRating; label: "Max rating"; text: settingsController.defaultMaxRating.toString(); numeric: true }
                LabeledField { id: limit; label: "Лимит"; text: settingsController.defaultLimit.toString(); numeric: true }
                LabeledField { id: scanLimit; label: "Scan limit"; text: "0"; numeric: true }
                AppButton { text: "Найти"; Layout.columnSpan: 2; Layout.fillWidth: true; onClicked: modelFinderController.searchModelGames(dbPath.text, fen.text, eco.text, opening.text, parseInt(minRating.text), parseInt(maxRating.text), perf.currentText, parseInt(limit.text), parseInt(scanLimit.text)) }
                AppButton { text: "Экспорт PGN"; Layout.columnSpan: 2; Layout.fillWidth: true; onClicked: modelFinderController.exportLast("") }
            }
        }

        StatusLine { status: modelFinderController.status; Layout.fillWidth: true }
        RowsList { Layout.fillWidth: true; Layout.fillHeight: true; rows: modelFinderController.rows; emptyText: "Введи ECO/название/FEN и запусти поиск" }
    }
}
