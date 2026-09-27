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
            SectionTitle { text: "Training Cards" }
            Item { Layout.fillWidth: true }
            AppButton { text: "Обновить"; onClicked: trainingCardsController.refresh() }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 275
            color: "#1b2029"
            border.color: "#2c3442"
            radius: 12

            GridLayout {
                anchors.fill: parent
                anchors.margins: 12
                columns: 4
                columnSpacing: 10
                rowSpacing: 8
                LabeledField { id: title; Layout.columnSpan: 2; Layout.fillWidth: true; label: "Название"; placeholder: "Black vs London" }
                LabeledField { id: theme; Layout.columnSpan: 2; Layout.fillWidth: true; label: "Тема"; placeholder: "дебют / структура / эндшпиль" }
                LabeledField { id: fen; Layout.columnSpan: 4; Layout.fillWidth: true; label: "FEN"; placeholder: "опционально" }
                LabeledField { id: problem; Layout.columnSpan: 2; Layout.fillWidth: true; label: "Моя проблема" }
                LabeledField { id: typicalBreak; Layout.columnSpan: 2; Layout.fillWidth: true; label: "Типовой прорыв" }
                LabeledField { id: plan; Layout.columnSpan: 2; Layout.fillWidth: true; label: "План" }
                LabeledField { id: avoid; Layout.columnSpan: 2; Layout.fillWidth: true; label: "Чего избегать" }
                LabeledField { id: modelGames; Layout.columnSpan: 3; Layout.fillWidth: true; label: "Модельные партии / ссылки" }
                AppButton { text: "Создать"; Layout.alignment: Qt.AlignBottom; onClicked: trainingCardsController.addCard(title.text, fen.text, theme.text, problem.text, plan.text, typicalBreak.text, avoid.text, modelGames.text) }
            }
        }

        StatusLine { status: trainingCardsController.status; Layout.fillWidth: true }
        RowsList { Layout.fillWidth: true; Layout.fillHeight: true; rows: trainingCardsController.rows; emptyText: "Создай первую карточку по проблемной теме" }
    }
}
