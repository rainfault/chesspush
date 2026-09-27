import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import ".."
import "../components"

Item {
    id: page
    objectName: "parserPage"
    property int panel: 0
    property bool advanced: false
    Connections { target: boardController; function onChanged() { openingSearch.clear() } }
    readonly property real boardSize: Math.floor(Math.min(width - 372, height - 152, 800) / 8) * 8
    FileDialog { id: sourceDialog; title: "Открыть базу"; nameFilters: ["Шахматные базы (*.zst *.pgn)"]; onAccepted: parserController.selectSource(selectedFile.toString()) }
    FileDialog { id: importDialog; title: "Импорт PGN"; nameFilters: ["PGN (*.pgn)"]; onAccepted: parserController.importPgn(selectedFile.toString()) }
    RowLayout {
        anchors.top: parent.top; anchors.horizontalCenter: parent.horizontalCenter
        height: parent.height; spacing: Theme.spaceL
        ColumnLayout {
            Layout.preferredWidth: page.boardSize; Layout.alignment: Qt.AlignTop
            spacing: Theme.spaceS
            UiField {
                id: openingSearch
                Layout.fillWidth: true
                placeholderText: "Дебют / ECO"
                enabled: !parserController.busy
                onTextEdited: {
                    openingList.model = boardController.searchOpenings(text)
                    if (openingList.count > 0) openingPopup.open(); else openingPopup.close()
                }
                Keys.onEscapePressed: openingPopup.close()
                Popup {
                    id: openingPopup
                    y: parent.height; width: parent.width; padding: 1
                    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
                    height: Math.min(openingList.contentHeight + 2, 310)
                    background: Rectangle { color: Theme.surface; border.color: Theme.border; radius: Theme.radius }
                    contentItem: ListView {
                        id: openingList; clip: true
                        ScrollBar.vertical: ScrollBar {}
                        delegate: ItemDelegate {
                            required property var modelData
                            width: openingList.width; height: 42
                            contentItem: UiLabel { text: modelData.label; elide: Text.ElideRight }
                            background: Rectangle { color: parent.hovered ? Theme.buttonHover : Theme.surface }
                            onClicked: {
                                boardController.loadPgn(modelData.pgn)
                                openingSearch.text = modelData.label
                                openingPopup.close()
                            }
                        }
                    }
                }
            }
            ChessgroundBoard { Layout.preferredWidth: page.boardSize; Layout.preferredHeight: page.boardSize; enabled: !parserController.busy }
            RowLayout {
                Layout.fillWidth: true; spacing: Theme.spaceXS
                UiButton { text: "↻"; font.pixelSize: 22; flatStyle: true; Accessible.name: "Перевернуть доску"; onClicked: boardController.flip() }
                Item { Layout.fillWidth: true }
                UiButton { text: "|‹"; font.pixelSize: 20; flatStyle: true; enabled: boardController.cursor > 0 && !parserController.busy; Accessible.name: "В начало"; onClicked: boardController.seek(0) }
                UiButton { text: "‹"; font.pixelSize: 28; flatStyle: true; enabled: boardController.cursor > 0 && !parserController.busy; Accessible.name: "Назад"; onClicked: boardController.seek(boardController.cursor - 1) }
                UiButton { text: "›"; font.pixelSize: 28; flatStyle: true; enabled: boardController.cursor < boardController.total && !parserController.busy; Accessible.name: "Вперёд"; onClicked: boardController.seek(boardController.cursor + 1) }
                UiButton { text: "›|"; font.pixelSize: 20; flatStyle: true; enabled: boardController.cursor < boardController.total && !parserController.busy; Accessible.name: "В конец"; onClicked: boardController.seek(boardController.total) }
                Item { Layout.fillWidth: true }
                UiButton { text: "PGN"; flatStyle: true; enabled: !parserController.busy; onClicked: { pgnInput.text = boardController.pgn; pgnDialog.open() } }
                UiButton { text: "Сброс"; flatStyle: true; enabled: !parserController.busy; onClicked: { boardController.reset(); openingSearch.clear() } }
            }
            Flickable {
                Layout.fillWidth: true; Layout.preferredHeight: 54
                clip: true; contentHeight: moveFlow.height
                ScrollBar.vertical: ScrollBar {}
                Flow {
                    id: moveFlow; width: parent.width; spacing: 0
                    Repeater {
                        model: boardController.moves
                        UiButton {
                            required property var modelData
                            text: modelData.text; implicitHeight: 28; flatStyle: true
                            selected: modelData.ply === boardController.cursor
                            enabled: !parserController.busy
                            onClicked: boardController.seek(modelData.ply)
                        }
                    }
                }
            }
        }
        ColumnLayout {
            Layout.preferredWidth: 348; Layout.fillHeight: true
            spacing: Theme.spaceM
            RowLayout {
                Layout.fillWidth: true; spacing: Theme.spaceS
                UiButton {
                    text: parserController.sourceName || "Открыть базу…"
                    Layout.fillWidth: true; enabled: !parserController.busy
                    onClicked: sourceDialog.open()
                }
            }
            Rectangle {
                Layout.fillWidth: true; Layout.fillHeight: true
                color: Theme.surface; radius: Theme.radius
                ColumnLayout {
                    anchors.fill: parent; spacing: 0
                    RowLayout {
                        Layout.fillWidth: true; spacing: 0
                        UiButton { text: "Фильтры"; Layout.fillWidth: true; flatStyle: true; selected: page.panel === 0; onClicked: page.panel = 0 }
                        UiButton { text: "Файлы"; Layout.fillWidth: true; flatStyle: true; selected: page.panel === 1; onClicked: { page.panel = 1; parserController.refreshFiles() } }
                    }
                    ScrollView {
                        visible: page.panel === 0
                        Layout.fillWidth: true; Layout.fillHeight: true
                        contentWidth: availableWidth; clip: true
                        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                        ColumnLayout {
                            width: parent.width
                            spacing: Theme.spaceM
                            enabled: !parserController.busy
                            GridLayout {
                                Layout.fillWidth: true; Layout.margins: Theme.spaceM
                                columns: 2; columnSpacing: Theme.spaceS; rowSpacing: Theme.spaceS
                                UiLabel { text: "Сторона" }
                                UiLabel { text: "Контроль" }
                                UiSelect { id: side; Layout.fillWidth: true; model: ["Любая","Белые","Чёрные"] }
                                UiSelect { id: perf; Layout.fillWidth: true; model: ["Любой","Rapid","Blitz","Classical","Bullet"] }
                                UiLabel { text: separate.checked ? "Рейтинг белых" : "Рейтинг"; Layout.columnSpan: 2; Layout.topMargin: Theme.spaceS }
                                UiField { id: minRating; Layout.fillWidth: true; text: String(settingsController.defaultMinRating); validator: IntValidator { bottom: 0; top: 4000 } Accessible.name: "Минимальный рейтинг" }
                                UiField { id: maxRating; Layout.fillWidth: true; text: String(settingsController.defaultMaxRating); validator: IntValidator { bottom: 0; top: 4000 } Accessible.name: "Максимальный рейтинг" }
                                UiLabel { text: "Партий"; Layout.columnSpan: 2; Layout.topMargin: Theme.spaceS }
                                UiField { id: limit; Layout.fillWidth: true; Layout.columnSpan: 2; text: String(settingsController.defaultLimit); validator: IntValidator { bottom: 1; top: 1000000 } Accessible.name: "Количество партий" }
                                UiButton { text: page.advanced ? "Дополнительно  ⌃" : "Дополнительно  ⌄"; flatStyle: true; Layout.columnSpan: 2; Layout.fillWidth: true; Layout.topMargin: Theme.spaceS; onClicked: page.advanced = !page.advanced }
                                UiLabel { text: "Результат"; visible: page.advanced; Layout.columnSpan: 2 }
                                UiSelect { id: result; Layout.fillWidth: true; Layout.columnSpan: 2; visible: page.advanced; model: ["Любой","Победа выбранной стороны","Поражение выбранной стороны","Победа или ничья","Без быстрых поражений"] }
                                UiCheck { id: separate; text: "Раздельный рейтинг"; visible: page.advanced; Layout.columnSpan: 2 }
                                UiLabel { text: "Рейтинг чёрных"; visible: page.advanced && separate.checked; Layout.columnSpan: 2 }
                                UiField { id: minBlack; text: minRating.text; visible: page.advanced && separate.checked; Layout.fillWidth: true; validator: IntValidator { bottom: 0; top: 4000 } }
                                UiField { id: maxBlack; text: maxRating.text; visible: page.advanced && separate.checked; Layout.fillWidth: true; validator: IntValidator { bottom: 0; top: 4000 } }
                                UiLabel { text: "Минимум ходов"; visible: page.advanced }
                                UiLabel { text: "Пропустить партий"; visible: page.advanced }
                                UiField { id: minMoves; text: String(settingsController.defaultMinMoves); visible: page.advanced; Layout.fillWidth: true; validator: IntValidator { bottom: 0; top: 1000 } }
                                UiField { id: startIndex; text: "0"; visible: page.advanced; Layout.fillWidth: true; validator: RegularExpressionValidator { regularExpression: /[0-9]{1,18}/ } }
                                UiLabel { text: "ECO"; visible: page.advanced; Layout.columnSpan: 2 }
                                UiField { id: eco; visible: page.advanced; Layout.columnSpan: 2; Layout.fillWidth: true; enabled: boardController.cursor === 0 }
                                UiCheck { id: excludeBullet; text: "Исключить Bullet"; checked: settingsController.excludeBullet; visible: page.advanced; enabled: perf.currentIndex !== 4; Layout.columnSpan: 2 }
                                UiCheck { id: timeout; text: "Включить просрочку времени"; visible: page.advanced; Layout.columnSpan: 2 }
                            }
                        }
                    }
                    ColumnLayout {
                        visible: page.panel === 1; Layout.fillWidth: true; Layout.fillHeight: true
                        spacing: Theme.spaceS
                        UiButton { text: "Импорт PGN"; Layout.margins: Theme.spaceM; Layout.fillWidth: true; enabled: !parserController.busy; onClicked: importDialog.open() }
                        ListView {
                            Layout.fillWidth: true; Layout.fillHeight: true; clip: true
                            model: parserController.files
                            ScrollBar.vertical: ScrollBar {}
                            delegate: Item {
                                required property var modelData
                                width: ListView.view.width; height: 72
                                Rectangle { anchors.fill: parent; color: fileMouse.containsMouse ? Theme.surfaceMid : "transparent" }
                                Column {
                                    anchors.left: parent.left; anchors.leftMargin: Theme.spaceM
                                    anchors.right: useFile.left; anchors.rightMargin: Theme.spaceS
                                    anchors.verticalCenter: parent.verticalCenter; spacing: Theme.spaceXS
                                    UiLabel { width: parent.width; text: modelData.name; color: modelData.exists ? Theme.accent : Theme.error; elide: Text.ElideMiddle }
                                    UiLabel { text: (modelData.kind === "import" ? "Импорт" : modelData.kind === "export" ? "Выборка" : "База") + (modelData.games ? " · " + modelData.games + " партий" : ""); color: Theme.muted }
                                }
                                MouseArea { id: fileMouse; anchors.fill: parent; anchors.rightMargin: 48; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: parserController.revealFile(modelData.id) }
                                UiButton { id: useFile; anchors.right: parent.right; anchors.rightMargin: Theme.spaceS; anchors.verticalCenter: parent.verticalCenter; text: "↗"; flatStyle: true; Accessible.name: "Выбрать базой"; enabled: !parserController.busy && modelData.exists; onClicked: parserController.useFile(modelData.id) }
                            }
                            UiLabel { anchors.centerIn: parent; text: "Нет файлов"; visible: parent.count === 0; color: Theme.muted }
                        }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true; Layout.margins: Theme.spaceM; spacing: Theme.spaceS
                        UiLabel { Layout.fillWidth: true; visible: boardController.error.length > 0; text: boardController.error; color: Theme.error; wrapMode: Text.Wrap }
                        UiLabel { Layout.fillWidth: true; visible: parserController.error.length > 0; text: parserController.error; color: Theme.error; wrapMode: Text.Wrap }
                        UiLabel { Layout.fillWidth: true; visible: parserController.busy || parserController.status.length > 0; text: parserController.busy ? "Проверено: " + parserController.scanned + " · " + parserController.found : parserController.status; wrapMode: Text.Wrap }
                        ProgressBar {
                            Layout.fillWidth: true; visible: parserController.busy; indeterminate: true
                            palette.highlight: Theme.accent; palette.dark: Theme.surfaceLow
                        }
                        UiButton {
                            text: parserController.busy ? "Остановить" : "Найти партии"
                            primary: !parserController.busy; Layout.fillWidth: true
                            onClicked: {
                                if (parserController.busy) { parserController.cancel(); return }
                                parserController.extract({side: ["any","white","black"][side.currentIndex], perf: ["any","rapid","blitz","classical","bullet"][perf.currentIndex], minRating: minRating.text, maxRating: maxRating.text, limit: limit.text, result: ["all","selected_wins","selected_losses","wins_draws","no_quick_losses"][result.currentIndex], separateRatings: separate.checked, minBlack: minBlack.text, maxBlack: maxBlack.text, minMoves: minMoves.text, startIndex: startIndex.text, excludeBullet: excludeBullet.checked, includeTimeout: timeout.checked, eco: eco.text})
                            }
                        }
                    }
                }
            }
        }
    }
    Popup {
        id: pgnDialog; anchors.centerIn: Overlay.overlay; width: 580; padding: Theme.spaceL; modal: true; focus: true
        background: Rectangle { color: Theme.surface; radius: Theme.radius }
        contentItem: ColumnLayout {
            spacing: Theme.spaceM
            UiLabel { text: "PGN"; font.pixelSize: Theme.title }
            ScrollView {
                Layout.fillWidth: true; Layout.preferredHeight: 170
                TextArea { id: pgnInput; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.body; wrapMode: TextEdit.Wrap; selectByMouse: true; background: Rectangle { color: Theme.inputBg; radius: Theme.radius } }
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                UiButton { text: "Отмена"; flatStyle: true; onClicked: pgnDialog.close() }
                UiButton { text: "Применить"; primary: true; onClicked: { boardController.loadPgn(pgnInput.text); if (!boardController.error) { openingSearch.clear(); pgnDialog.close() } } }
            }
            UiLabel { text: boardController.error; visible: text.length > 0; color: Theme.error; Layout.fillWidth: true; wrapMode: Text.Wrap }
        }
    }
}
