import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import ".."
import "../components"

Item {
    id: page

    function latestGameCount() {
        var value = parseInt(latestGames.text)
        value = isNaN(value) || value < 1 ? 100 : Math.min(value, 5000)
        latestGames.text = String(value)
        return value
    }

    Component.onCompleted: statisticsController.refresh()

    FileDialog {
        id: pgnDialog
        title: "Загрузить PGN для статистики"
        nameFilters: ["PGN files (*.pgn)", "All files (*)"]
        onAccepted: statisticsController.loadPgn(statisticsController.pathFromUrl(selectedFile))
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 12

        ColumnLayout {
            Layout.fillWidth: true
            spacing: 2

            SectionTitle { text: "Статистика по дебютам" }
            Text {
                Layout.fillWidth: true
                text: "Одна строка на ECO · худший винрейт сверху"
                color: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: 12
                elide: Text.ElideRight
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: statisticsController.busy ? 132 : 112
            radius: 8
            color: Theme.panelBg
            border.color: Theme.border

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 14
                spacing: 10

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 12

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Text {
                            text: "Игрок из настроек"
                            color: Theme.muted
                            font.family: Theme.fontFamily
                            font.pixelSize: 11
                        }
                        Text {
                            Layout.fillWidth: true
                            text: settingsController.lichessUsername || "username не указан"
                            color: settingsController.lichessUsername ? Theme.textStrong : Theme.faint
                            font.family: Theme.fontFamily
                            font.pixelSize: 15
                            font.bold: true
                            elide: Text.ElideRight
                        }
                    }

                    LabeledField {
                        id: latestGames
                        Layout.preferredWidth: 150
                        label: "Последние N партий"
                        text: "100"
                        numeric: true
                    }

                    AppButton {
                        text: statisticsController.busy ? "Загрузка..." : "Загрузить с Lichess"
                        primary: true
                        Layout.preferredWidth: 190
                        enabled: !statisticsController.busy
                                 && settingsController.lichessUsername.trim().length > 0
                        onClicked: statisticsController.fetchLatestGames(page.latestGameCount())
                    }

                    AppButton {
                        text: "Загрузить PGN"
                        Layout.preferredWidth: 160
                        enabled: !statisticsController.busy
                                 && settingsController.lichessUsername.trim().length > 0
                        onClicked: pgnDialog.open()
                    }
                }

                ProgressBar {
                    Layout.fillWidth: true
                    Layout.preferredHeight: visible ? 8 : 0
                    visible: statisticsController.busy
                    indeterminate: true
                    background: Rectangle {
                        radius: 4
                        color: Theme.progressBg
                        border.color: Theme.borderStrong
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 10
                    Text {
                        Layout.fillWidth: true
                        text: statisticsController.status
                        color: Theme.success
                        font.family: Theme.fontFamily
                        font.pixelSize: 12
                        elide: Text.ElideRight
                    }
                    Text {
                        text: statisticsController.source
                        visible: text.length > 0
                        color: Theme.muted
                        font.family: Theme.fontFamily
                        font.pixelSize: 12
                        elide: Text.ElideLeft
                        Layout.maximumWidth: 360
                    }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            radius: 8
            color: Theme.panelBg
            border.color: Theme.border

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 12
                spacing: 6

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 38
                    radius: 6
                    color: Theme.panelAlt
                    border.color: Theme.borderStrong

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 10
                        anchors.rightMargin: 10
                        spacing: 12

                        HeaderCell { text: "ECO"; widthHint: 90 }
                        HeaderCell { text: "Дебют"; fill: true }
                        HeaderCell { text: "Винрейт"; widthHint: 120; horizontalAlignment: Text.AlignRight }
                        HeaderCell { text: ""; widthHint: 172 }
                    }
                }

                ListView {
                    id: openingTable
                    objectName: "statisticsOpeningTable"
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    spacing: 4
                    boundsBehavior: Flickable.StopAtBounds
                    model: statisticsController.rows
                    visible: statisticsController.rows && statisticsController.rows.length > 0

                    delegate: Rectangle {
                        id: openingRow
                        required property int index
                        required property var modelData

                        width: ListView.view.width
                        height: 48
                        radius: 6
                        color: index % 2 === 0 ? Theme.rowBg : Theme.rowAlt
                        border.color: Theme.borderStrong

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 10
                            anchors.rightMargin: 10
                            spacing: 12

                            ValueCell { text: openingRow.modelData.eco || "???"; widthHint: 90; strong: true }
                            ValueCell { text: openingRow.modelData.opening || "Без названия дебюта"; fill: true }
                            ValueCell { text: openingRow.modelData.winrate || "0.0%"; widthHint: 120; strong: true; horizontalAlignment: Text.AlignRight }
                            AppButton {
                                id: openGamesButton
                                text: "Открыть мои партии"
                                Layout.preferredWidth: 172
                                onClicked: openGamesMenu.open()

                                Menu {
                                    id: openGamesMenu
                                    y: openGamesButton.height
                                    width: 190

                                    OpenMenuItem {
                                        text: "За оба цвета"
                                        onTriggered: statisticsController.openMyGames(
                                            openingRow.modelData.eco || "", "any"
                                        )
                                    }
                                    OpenMenuItem {
                                        text: "Только белыми"
                                        onTriggered: statisticsController.openMyGames(
                                            openingRow.modelData.eco || "", "white"
                                        )
                                    }
                                    OpenMenuItem {
                                        text: "Только чёрными"
                                        onTriggered: statisticsController.openMyGames(
                                            openingRow.modelData.eco || "", "black"
                                        )
                                    }

                                    background: Rectangle {
                                        radius: 7
                                        color: Theme.inputBg
                                        border.color: Theme.borderStrong
                                    }
                                }
                            }
                        }
                    }
                }

                Text {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    visible: !statisticsController.rows || statisticsController.rows.length === 0
                    text: "Загрузите последние партии с Lichess или выберите PGN-файл."
                    color: Theme.faint
                    font.family: Theme.fontFamily
                    font.pixelSize: 13
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    wrapMode: Text.WordWrap
                }
            }
        }
    }

    component HeaderCell: Text {
        property int widthHint: 100
        property bool fill: false
        Layout.preferredWidth: widthHint
        Layout.fillWidth: fill
        color: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: 12
        font.bold: true
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
    }

    component ValueCell: Text {
        property int widthHint: 100
        property bool fill: false
        property bool strong: false
        Layout.preferredWidth: widthHint
        Layout.fillWidth: fill
        color: strong ? Theme.textStrong : Theme.text
        font.family: Theme.fontFamily
        font.pixelSize: 13
        font.bold: strong
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
    }

    component OpenMenuItem: MenuItem {
        id: menuItem
        implicitHeight: 36
        contentItem: Text {
            text: menuItem.text
            color: Theme.text
            font.family: Theme.fontFamily
            font.pixelSize: 12
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            color: menuItem.highlighted ? Theme.buttonHover : "transparent"
        }
    }
}
