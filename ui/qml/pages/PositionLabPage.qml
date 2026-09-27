pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import ".."
import "../components"

Item {
    id: page
    objectName: "positionLabPage"

    property string annotationColor: "green"
    property int pendingDeleteId: 0

    function modeIndex(mode) {
        if (mode === "collection")
            return 1
        if (mode === "training")
            return 2
        return 0
    }

    function modeName(index) {
        return ["games", "collection", "training"][Math.max(0, Math.min(2, index))]
    }

    function valueOf(record, key, fallbackValue) {
        if (!record)
            return fallbackValue
        var value = record[key]
        if (value === undefined || value === null || String(value).length === 0)
            return fallbackValue
        return String(value)
    }

    function recordId(record) {
        if (!record || record.id === undefined || record.id === null)
            return 0
        return Number(record.id)
    }

    function colorLabel(value) {
        if (value === "white")
            return "белые"
        if (value === "black")
            return "чёрные"
        return "не указан"
    }

    function sideLabel(value) {
        return value === "black" ? "ход чёрных" : "ход белых"
    }

    function swatchColor(value) {
        if (value === "red")
            return "#df3933"
        if (value === "blue")
            return "#2879df"
        if (value === "yellow")
            return "#e9ae20"
        return "#34a94d"
    }

    function syncSelectedPositionEditor() {
        var position = positionLabController.selectedPosition
        collectionTags.text = page.valueOf(position, "tags_text", "")
        collectionComment.text = page.valueOf(position, "comment", "")
    }

    function ensureCollectionSelection() {
        if (positionLabController.mode !== "collection")
            return
        if (page.recordId(positionLabController.selectedPosition) > 0)
            return
        var rows = positionLabController.positionRows
        if (rows && rows.length > 0)
            positionLabController.openPosition(Number(rows[0].id))
    }

    function applyCollectionFilters() {
        positionLabController.applyPositionFilters(
            collectionSearch.text,
            collectionFilterTags.text,
            collectionEco.text,
            collectionOpening.text,
            String(collectionColor.currentValue || "any"),
            collectionSource.text,
            collectionDateFrom.text,
            collectionDateTo.text
        )
        Qt.callLater(page.ensureCollectionSelection)
    }

    function resetCollectionFilters() {
        collectionSearch.text = ""
        collectionFilterTags.text = ""
        collectionEco.text = ""
        collectionOpening.text = ""
        collectionColor.currentIndex = 0
        collectionSource.text = ""
        collectionDateFrom.text = ""
        collectionDateTo.text = ""
        page.applyCollectionFilters()
    }

    Component.onCompleted: {
        modeTabs.currentIndex = page.modeIndex(positionLabController.mode)
        page.syncSelectedPositionEditor()
        Qt.callLater(page.ensureCollectionSelection)
    }

    Timer {
        id: filterDebounce
        interval: 260
        repeat: false
        onTriggered: page.applyCollectionFilters()
    }

    FileDialog {
        id: externalPgnDialog
        title: "Открыть внешний PGN в Position Lab"
        nameFilters: ["PGN files (*.pgn)", "All files (*)"]
        onAccepted: positionLabController.loadPgn(
            positionLabController.pathFromUrl(selectedFile)
        )
    }

    Dialog {
        id: deleteDialog
        title: "Удалить позицию?"
        modal: true
        standardButtons: Dialog.Yes | Dialog.No
        x: Math.max(0, (page.width - width) / 2)
        y: Math.max(0, (page.height - height) / 2)

        Text {
            width: 330
            text: "Позиция, комментарий и история её тренировок будут удалены."
            color: Theme.text
            font.family: Theme.fontFamily
            wrapMode: Text.WordWrap
        }

        onAccepted: {
            if (page.pendingDeleteId > 0)
                positionLabController.deletePosition(page.pendingDeleteId)
            page.pendingDeleteId = 0
        }
        onRejected: page.pendingDeleteId = 0
    }

    Connections {
        target: positionLabController

        function onModeChanged() {
            modeTabs.currentIndex = page.modeIndex(positionLabController.mode)
            Qt.callLater(page.ensureCollectionSelection)
        }

        function onSelectedPositionChanged() {
            page.syncSelectedPositionEditor()
        }

        function onCollectionChanged() {
            Qt.callLater(page.ensureCollectionSelection)
        }

        function onBoardChanged() {
            if (positionLabController.currentPly > 0
                    && positionLabController.currentPly <= moveList.count) {
                moveList.positionViewAtIndex(
                    positionLabController.currentPly - 1,
                    ListView.Contain
                )
            }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 9

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 42
            spacing: 12

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0

                SectionTitle { text: "Position Lab" }
                Text {
                    Layout.fillWidth: true
                    text: "От партии — к сохранённой идее и повторению"
                    color: Theme.muted
                    font.family: Theme.fontFamily
                    font.pixelSize: 11
                    elide: Text.ElideRight
                }
            }

            Text {
                text: positionLabController.sourceLabel
                visible: positionLabController.mode === "games" && text.length > 0
                color: Theme.muted
                font.family: Theme.fontFamily
                font.pixelSize: 12
                elide: Text.ElideLeft
                Layout.maximumWidth: 390
            }
        }

        TabBar {
            id: modeTabs
            objectName: "positionLabModeTabs"
            Layout.fillWidth: true
            Layout.preferredHeight: 40
            background: Rectangle { color: Theme.progressBg; radius: 7 }

            ModeTab { text: "Просмотр партий" }
            ModeTab { text: "Коллекция позиций" }
            ModeTab { text: "Тренировка" }

            onCurrentIndexChanged: {
                var requestedMode = page.modeName(currentIndex)
                if (positionLabController.mode !== requestedMode)
                    positionLabController.setMode(requestedMode)
            }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: modeTabs.currentIndex

            Item {
                id: gamesMode

                RowLayout {
                    anchors.fill: parent
                    spacing: 10

                    Rectangle {
                        Layout.preferredWidth: 225
                        Layout.minimumWidth: 205
                        Layout.fillHeight: true
                        color: Theme.panelBg
                        border.color: Theme.border
                        radius: 8

                        ColumnLayout {
                            anchors.fill: parent
                            anchors.margins: 9
                            spacing: 7

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 6

                                PanelTitle {
                                    text: "Партии"
                                    Layout.fillWidth: true
                                }
                                Text {
                                    text: String(positionLabController.gameRows.length)
                                    color: Theme.faint
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 12
                                }
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 6

                                AppButton {
                                    text: "Локальные"
                                    Layout.fillWidth: true
                                    onClicked: positionLabController.refreshLocalGames()
                                }
                                AppButton {
                                    text: "PGN"
                                    Layout.preferredWidth: 66
                                    onClicked: externalPgnDialog.open()
                                }
                            }

                            ListView {
                                id: gameList
                                objectName: "positionLabGameList"
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                clip: true
                                spacing: 5
                                boundsBehavior: Flickable.StopAtBounds
                                model: positionLabController.gameRows
                                ScrollBar.vertical: ScrollBar { }

                                delegate: Rectangle {
                                    id: gameDelegate

                                    required property int index
                                    required property var modelData

                                    width: gameList.width
                                    height: 62
                                    radius: 7
                                    color: index === positionLabController.currentGameIndex
                                           ? Theme.selectionBg
                                           : (gameMouse.containsMouse ? Theme.buttonHover : Theme.rowBg)
                                    border.color: index === positionLabController.currentGameIndex
                                                  ? Theme.focus : Theme.borderStrong

                                    Column {
                                        anchors.fill: parent
                                        anchors.margins: 8
                                        spacing: 3

                                        Text {
                                            width: parent.width
                                            text: page.valueOf(gameDelegate.modelData, "title", "Партия")
                                            color: gameDelegate.index === positionLabController.currentGameIndex
                                                   ? Theme.selectionText : Theme.textStrong
                                            font.family: Theme.fontFamily
                                            font.pixelSize: 12
                                            font.bold: true
                                            elide: Text.ElideRight
                                        }
                                        Text {
                                            width: parent.width
                                            text: page.valueOf(gameDelegate.modelData, "subtitle", "")
                                            color: gameDelegate.index === positionLabController.currentGameIndex
                                                   ? Theme.selectionText : Theme.muted
                                            opacity: 0.86
                                            font.family: Theme.fontFamily
                                            font.pixelSize: 10
                                            elide: Text.ElideRight
                                        }
                                    }

                                    MouseArea {
                                        id: gameMouse
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        onClicked: positionLabController.selectGame(gameDelegate.index)
                                    }
                                }
                            }

                            Text {
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                visible: positionLabController.gameRows.length === 0
                                text: "Открой локальную библиотеку, внешний PGN или выборку из Statistics / ZST."
                                color: Theme.faint
                                font.family: Theme.fontFamily
                                font.pixelSize: 12
                                wrapMode: Text.WordWrap
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                            }
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.minimumWidth: 400
                        spacing: 6

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 42
                            color: Theme.panelBg
                            border.color: Theme.border
                            radius: 7

                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: 10
                                anchors.rightMargin: 10
                                spacing: 8

                                Text {
                                    Layout.fillWidth: true
                                    text: page.valueOf(
                                        positionLabController.currentGameInfo,
                                        "players",
                                        "Выберите партию"
                                    )
                                    color: Theme.textStrong
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 13
                                    font.bold: true
                                    elide: Text.ElideRight
                                }
                                Text {
                                    text: page.valueOf(positionLabController.currentGameInfo, "result", "")
                                    color: Theme.muted
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 12
                                }
                            }
                        }

                        Item {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            Layout.minimumHeight: 360

                            ChessBoard {
                                id: gameBoard
                                objectName: "positionLabGameBoard"
                                anchors.centerIn: parent
                                width: Math.min(parent.width, parent.height)
                                height: width
                                squares: positionLabController.boardSquares
                                arrows: positionLabController.arrows
                                flipped: positionLabController.flipped
                                annotationColor: page.annotationColor
                                editableAnnotations: positionLabController.hasCurrentGame
                                onArrowRequested: function(fromSquare, toSquare, color) {
                                    positionLabController.toggleArrow(fromSquare, toSquare, color)
                                }
                                onHighlightRequested: function(square, color) {
                                    positionLabController.toggleHighlight(square, color)
                                }
                            }

                            Text {
                                anchors.centerIn: parent
                                visible: !positionLabController.hasCurrentGame
                                text: "Нет открытой партии"
                                color: Theme.faint
                                font.family: Theme.fontFamily
                                font.pixelSize: 13
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 36
                            spacing: 5

                            AppButton {
                                text: "⏮"
                                Layout.preferredWidth: 46
                                enabled: positionLabController.hasCurrentGame
                                         && positionLabController.currentPly > 0
                                onClicked: positionLabController.firstMove()
                            }
                            AppButton {
                                text: "←"
                                Layout.preferredWidth: 46
                                enabled: positionLabController.hasCurrentGame
                                         && positionLabController.currentPly > 0
                                onClicked: positionLabController.previousMove()
                            }
                            Text {
                                Layout.fillWidth: true
                                text: "Позиция " + positionLabController.currentPly
                                      + " / " + positionLabController.maxPly
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: 12
                                horizontalAlignment: Text.AlignHCenter
                            }
                            AppButton {
                                text: "→"
                                Layout.preferredWidth: 46
                                enabled: positionLabController.hasCurrentGame
                                         && positionLabController.currentPly < positionLabController.maxPly
                                onClicked: positionLabController.nextMove()
                            }
                            AppButton {
                                text: "⏭"
                                Layout.preferredWidth: 46
                                enabled: positionLabController.hasCurrentGame
                                         && positionLabController.currentPly < positionLabController.maxPly
                                onClicked: positionLabController.lastMove()
                            }
                            AppButton {
                                text: "Перевернуть"
                                Layout.preferredWidth: 104
                                enabled: positionLabController.hasCurrentGame
                                onClicked: positionLabController.flipBoard()
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 34
                            spacing: 6

                            Text {
                                text: "ПКМ: клетка / стрелка"
                                color: Theme.faint
                                font.family: Theme.fontFamily
                                font.pixelSize: 10
                            }
                            Item { Layout.fillWidth: true }
                            PaletteSwatch { swatchColor: "green" }
                            PaletteSwatch { swatchColor: "red" }
                            PaletteSwatch { swatchColor: "blue" }
                            PaletteSwatch { swatchColor: "yellow" }
                            AppButton {
                                text: "Очистить"
                                Layout.preferredWidth: 84
                                enabled: positionLabController.hasCurrentGame
                                onClicked: positionLabController.clearAnnotations()
                            }
                        }
                    }

                    Rectangle {
                        Layout.preferredWidth: 300
                        Layout.minimumWidth: 280
                        Layout.fillHeight: true
                        color: Theme.panelBg
                        border.color: Theme.border
                        radius: 8

                        ColumnLayout {
                            anchors.fill: parent
                            anchors.margins: 9
                            spacing: 6

                            Text {
                                Layout.fillWidth: true
                                text: page.valueOf(positionLabController.currentGameInfo, "eco", "???")
                                      + " · "
                                      + page.valueOf(
                                          positionLabController.currentGameInfo,
                                          "opening",
                                          "Дебют не указан"
                                      )
                                color: Theme.textStrong
                                font.family: Theme.fontFamily
                                font.pixelSize: 12
                                font.bold: true
                                elide: Text.ElideRight
                            }
                            Text {
                                Layout.fillWidth: true
                                text: [
                                    page.valueOf(positionLabController.currentGameInfo, "date", ""),
                                    page.valueOf(positionLabController.currentGameInfo, "event", ""),
                                    page.valueOf(positionLabController.currentGameInfo, "source_kind", "")
                                ].filter(function(value) { return value.length > 0 }).join(" · ")
                                color: Theme.muted
                                font.family: Theme.fontFamily
                                font.pixelSize: 10
                                elide: Text.ElideRight
                            }

                            Rectangle {
                                Layout.fillWidth: true
                                Layout.preferredHeight: 1
                                color: Theme.border
                            }

                            RowLayout {
                                Layout.fillWidth: true

                                PanelTitle {
                                    text: "Ходы"
                                    Layout.fillWidth: true
                                }
                                Text {
                                    text: positionLabController.currentPly + " / "
                                          + positionLabController.maxPly
                                    color: Theme.faint
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 11
                                }
                            }

                            AppButton {
                                text: "0 · Начальная позиция"
                                Layout.fillWidth: true
                                enabled: positionLabController.hasCurrentGame
                                primary: positionLabController.currentPly === 0
                                onClicked: positionLabController.goToPly(0)
                            }

                            ListView {
                                id: moveList
                                objectName: "positionLabMoveList"
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                Layout.minimumHeight: 130
                                clip: true
                                spacing: 3
                                boundsBehavior: Flickable.StopAtBounds
                                model: positionLabController.moveRows
                                ScrollBar.vertical: ScrollBar { }

                                delegate: Rectangle {
                                    id: moveDelegate

                                    required property int index
                                    required property var modelData
                                    readonly property bool selected:
                                        Number(modelData.ply) === positionLabController.currentPly

                                    width: moveList.width
                                    height: 32
                                    radius: 6
                                    color: selected ? Theme.selectionBg
                                                    : (moveMouse.containsMouse ? Theme.buttonHover : Theme.rowBg)
                                    border.color: selected ? Theme.focus : "transparent"

                                    Text {
                                        anchors.fill: parent
                                        anchors.leftMargin: 9
                                        anchors.rightMargin: 9
                                        text: page.valueOf(moveDelegate.modelData, "label", "")
                                        color: moveDelegate.selected ? Theme.selectionText : Theme.text
                                        font.family: Theme.fontFamily
                                        font.pixelSize: 12
                                        verticalAlignment: Text.AlignVCenter
                                        elide: Text.ElideRight
                                    }

                                    MouseArea {
                                        id: moveMouse
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        onClicked: positionLabController.goToPly(
                                            Number(moveDelegate.modelData.ply)
                                        )
                                    }
                                }
                            }

                            Rectangle {
                                Layout.fillWidth: true
                                Layout.preferredHeight: 1
                                color: Theme.border
                            }

                            PanelTitle { text: "Сохранить эту позицию" }

                            LabField {
                                id: gameTags
                                Layout.fillWidth: true
                                placeholderText: "Теги через запятую"
                            }

                            LabArea {
                                id: gameComment
                                Layout.fillWidth: true
                                Layout.preferredHeight: 84
                                placeholderText: "Что здесь важно помнить?"
                            }

                            AppButton {
                                text: "Сохранить позицию"
                                primary: true
                                Layout.fillWidth: true
                                enabled: positionLabController.hasCurrentGame
                                onClicked: positionLabController.saveCurrentPosition(
                                    gameTags.text,
                                    gameComment.text
                                )
                            }
                        }
                    }
                }
            }

            Item {
                id: collectionMode

                RowLayout {
                    anchors.fill: parent
                    spacing: 10

                    Rectangle {
                        Layout.preferredWidth: 320
                        Layout.minimumWidth: 300
                        Layout.fillHeight: true
                        color: Theme.panelBg
                        border.color: Theme.border
                        radius: 8

                        ColumnLayout {
                            anchors.fill: parent
                            anchors.margins: 9
                            spacing: 6

                            RowLayout {
                                Layout.fillWidth: true

                                PanelTitle {
                                    text: "Коллекция"
                                    Layout.fillWidth: true
                                }
                                Text {
                                    text: String(positionLabController.positionRows.length)
                                    color: Theme.faint
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 12
                                }
                            }

                            LabField {
                                id: collectionSearch
                                Layout.fillWidth: true
                                text: page.valueOf(
                                    positionLabController.positionFilters,
                                    "search",
                                    ""
                                )
                                placeholderText: "Поиск по позиции и комментарию"
                                onTextEdited: filterDebounce.restart()
                                onAccepted: page.applyCollectionFilters()
                            }

                            GridLayout {
                                Layout.fillWidth: true
                                columns: 2
                                columnSpacing: 6
                                rowSpacing: 6

                                LabField {
                                    id: collectionFilterTags
                                    Layout.fillWidth: true
                                    text: page.valueOf(
                                        positionLabController.positionFilters,
                                        "tags_text",
                                        ""
                                    )
                                    placeholderText: "Теги"
                                    onTextEdited: filterDebounce.restart()
                                }
                                LabField {
                                    id: collectionEco
                                    Layout.fillWidth: true
                                    text: page.valueOf(
                                        positionLabController.positionFilters,
                                        "eco",
                                        ""
                                    )
                                    placeholderText: "ECO"
                                    onTextEdited: filterDebounce.restart()
                                }
                                LabField {
                                    id: collectionOpening
                                    Layout.columnSpan: 2
                                    Layout.fillWidth: true
                                    text: page.valueOf(
                                        positionLabController.positionFilters,
                                        "opening",
                                        ""
                                    )
                                    placeholderText: "Название дебюта"
                                    onTextEdited: filterDebounce.restart()
                                }
                                AppComboBox {
                                    id: collectionColor
                                    Layout.fillWidth: true
                                    model: [
                                        { label: "Все цвета", value: "any" },
                                        { label: "Белые", value: "white" },
                                        { label: "Чёрные", value: "black" },
                                        { label: "Не указан", value: "unknown" }
                                    ]
                                    textRole: "label"
                                    valueRole: "value"
                                    currentIndex: {
                                        var value = page.valueOf(
                                            positionLabController.positionFilters,
                                            "color",
                                            "any"
                                        )
                                        if (value === "white")
                                            return 1
                                        if (value === "black")
                                            return 2
                                        if (value === "unknown")
                                            return 3
                                        return 0
                                    }
                                    onActivated: page.applyCollectionFilters()
                                }
                                LabField {
                                    id: collectionSource
                                    Layout.fillWidth: true
                                    text: page.valueOf(
                                        positionLabController.positionFilters,
                                        "source",
                                        ""
                                    )
                                    placeholderText: "Источник"
                                    onTextEdited: filterDebounce.restart()
                                }
                                LabField {
                                    id: collectionDateFrom
                                    Layout.fillWidth: true
                                    text: page.valueOf(
                                        positionLabController.positionFilters,
                                        "date_from",
                                        ""
                                    )
                                    placeholderText: "Дата от: YYYY-MM-DD"
                                    onTextEdited: filterDebounce.restart()
                                }
                                LabField {
                                    id: collectionDateTo
                                    Layout.fillWidth: true
                                    text: page.valueOf(
                                        positionLabController.positionFilters,
                                        "date_to",
                                        ""
                                    )
                                    placeholderText: "Дата до: YYYY-MM-DD"
                                    onTextEdited: filterDebounce.restart()
                                }
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 6

                                AppButton {
                                    text: "Применить"
                                    Layout.fillWidth: true
                                    onClicked: page.applyCollectionFilters()
                                }
                                AppButton {
                                    text: "Сбросить"
                                    Layout.fillWidth: true
                                    onClicked: page.resetCollectionFilters()
                                }
                            }

                            ListView {
                                id: positionList
                                objectName: "positionLabCollectionList"
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                Layout.minimumHeight: 150
                                clip: true
                                spacing: 5
                                boundsBehavior: Flickable.StopAtBounds
                                model: positionLabController.positionRows
                                ScrollBar.vertical: ScrollBar { }

                                delegate: Rectangle {
                                    id: positionDelegate

                                    required property int index
                                    required property var modelData
                                    readonly property bool selected:
                                        page.recordId(modelData)
                                        === page.recordId(positionLabController.selectedPosition)

                                    width: positionList.width
                                    height: 70
                                    radius: 7
                                    color: selected ? Theme.selectionBg
                                                    : (positionMouse.containsMouse ? Theme.buttonHover : Theme.rowBg)
                                    border.color: selected ? Theme.focus : Theme.borderStrong

                                    Column {
                                        anchors.fill: parent
                                        anchors.margins: 8
                                        spacing: 2

                                        Text {
                                            width: parent.width
                                            text: page.valueOf(positionDelegate.modelData, "title", "Позиция")
                                            color: positionDelegate.selected
                                                   ? Theme.selectionText : Theme.textStrong
                                            font.family: Theme.fontFamily
                                            font.pixelSize: 12
                                            font.bold: true
                                            elide: Text.ElideRight
                                        }
                                        Text {
                                            width: parent.width
                                            text: page.valueOf(positionDelegate.modelData, "subtitle", "")
                                            color: positionDelegate.selected
                                                   ? Theme.selectionText : Theme.muted
                                            opacity: 0.88
                                            font.family: Theme.fontFamily
                                            font.pixelSize: 10
                                            elide: Text.ElideRight
                                        }
                                        Text {
                                            width: parent.width
                                            text: page.valueOf(positionDelegate.modelData, "tags_text", "")
                                            color: positionDelegate.selected
                                                   ? Theme.selectionText : Theme.faint
                                            opacity: 0.82
                                            font.family: Theme.fontFamily
                                            font.pixelSize: 9
                                            elide: Text.ElideRight
                                        }
                                    }

                                    MouseArea {
                                        id: positionMouse
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        onClicked: positionLabController.openPosition(
                                            page.recordId(positionDelegate.modelData)
                                        )
                                    }
                                }
                            }

                            Text {
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                visible: positionLabController.positionRows.length === 0
                                text: "По этим фильтрам позиций нет."
                                color: Theme.faint
                                font.family: Theme.fontFamily
                                font.pixelSize: 12
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                            }
                        }
                    }

                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.minimumWidth: 400
                        spacing: 6

                        Item {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            Layout.minimumHeight: 400

                            ChessBoard {
                                id: collectionBoard
                                objectName: "positionLabCollectionBoard"
                                anchors.centerIn: parent
                                width: Math.min(parent.width, parent.height)
                                height: width
                                squares: positionLabController.boardSquares
                                arrows: positionLabController.arrows
                                flipped: positionLabController.flipped
                                annotationColor: page.annotationColor
                                editableAnnotations:
                                    page.recordId(positionLabController.selectedPosition) > 0
                                onArrowRequested: function(fromSquare, toSquare, color) {
                                    positionLabController.toggleArrow(fromSquare, toSquare, color)
                                }
                                onHighlightRequested: function(square, color) {
                                    positionLabController.toggleHighlight(square, color)
                                }
                            }

                            Text {
                                anchors.centerIn: parent
                                visible: page.recordId(positionLabController.selectedPosition) <= 0
                                text: "Выберите позицию"
                                color: Theme.faint
                                font.family: Theme.fontFamily
                                font.pixelSize: 13
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 36
                            spacing: 6

                            Text {
                                Layout.fillWidth: true
                                text: page.valueOf(
                                    positionLabController.selectedPosition,
                                    "move_label",
                                    "Позиция не выбрана"
                                )
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: 12
                                elide: Text.ElideRight
                            }
                            PaletteSwatch { swatchColor: "green" }
                            PaletteSwatch { swatchColor: "red" }
                            PaletteSwatch { swatchColor: "blue" }
                            PaletteSwatch { swatchColor: "yellow" }
                            AppButton {
                                text: "Очистить"
                                Layout.preferredWidth: 82
                                enabled: page.recordId(
                                    positionLabController.selectedPosition
                                ) > 0
                                onClicked: positionLabController.clearAnnotations()
                            }
                            AppButton {
                                text: "Перевернуть"
                                Layout.preferredWidth: 102
                                enabled: page.recordId(
                                    positionLabController.selectedPosition
                                ) > 0
                                onClicked: positionLabController.flipBoard()
                            }
                        }

                        Text {
                            Layout.fillWidth: true
                            text: positionLabController.boardFen
                            color: Theme.faint
                            font.family: "Consolas"
                            font.pixelSize: 9
                            elide: Text.ElideMiddle
                        }
                    }

                    Rectangle {
                        Layout.preferredWidth: 285
                        Layout.minimumWidth: 270
                        Layout.fillHeight: true
                        color: Theme.panelBg
                        border.color: Theme.border
                        radius: 8

                        ColumnLayout {
                            anchors.fill: parent
                            anchors.margins: 10
                            spacing: 7

                            PanelTitle {
                                Layout.fillWidth: true
                                text: page.valueOf(
                                    positionLabController.selectedPosition,
                                    "title",
                                    "Выберите позицию"
                                )
                                elide: Text.ElideRight
                            }

                            Text {
                                Layout.fillWidth: true
                                text: page.valueOf(
                                    positionLabController.selectedPosition,
                                    "opening",
                                    "Дебют не указан"
                                )
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: 12
                                wrapMode: Text.WordWrap
                                maximumLineCount: 2
                                elide: Text.ElideRight
                            }

                            Text {
                                Layout.fillWidth: true
                                text: {
                                    var position = positionLabController.selectedPosition
                                    var parts = [
                                        page.valueOf(position, "game_date", "без даты"),
                                        page.colorLabel(page.valueOf(position, "color", "unknown")),
                                        page.sideLabel(page.valueOf(position, "side_to_move", "white"))
                                    ]
                                    return parts.join(" · ")
                                }
                                color: Theme.muted
                                font.family: Theme.fontFamily
                                font.pixelSize: 10
                                wrapMode: Text.WordWrap
                            }

                            Text {
                                Layout.fillWidth: true
                                text: page.valueOf(
                                    positionLabController.selectedPosition,
                                    "source",
                                    "Источник не указан"
                                )
                                color: Theme.faint
                                font.family: Theme.fontFamily
                                font.pixelSize: 10
                                elide: Text.ElideMiddle
                            }

                            Rectangle {
                                Layout.fillWidth: true
                                Layout.preferredHeight: 1
                                color: Theme.border
                            }

                            Text {
                                text: "Теги"
                                color: Theme.muted
                                font.family: Theme.fontFamily
                                font.pixelSize: 10
                            }
                            LabField {
                                id: collectionTags
                                Layout.fillWidth: true
                                placeholderText: "Теги через запятую"
                            }

                            Text {
                                text: "Комментарий"
                                color: Theme.muted
                                font.family: Theme.fontFamily
                                font.pixelSize: 10
                            }
                            LabArea {
                                id: collectionComment
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                Layout.minimumHeight: 150
                                placeholderText: "План, идея, типичная ошибка…"
                            }

                            AppButton {
                                text: "Сохранить изменения"
                                primary: true
                                Layout.fillWidth: true
                                enabled: page.recordId(
                                    positionLabController.selectedPosition
                                ) > 0
                                onClicked: positionLabController.updatePosition(
                                    page.recordId(positionLabController.selectedPosition),
                                    collectionTags.text,
                                    collectionComment.text
                                )
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                spacing: 6

                                AppButton {
                                    text: "К партии"
                                    Layout.fillWidth: true
                                    enabled: page.recordId(
                                        positionLabController.selectedPosition
                                    ) > 0
                                    onClicked: positionLabController.returnToSourceGame(
                                        page.recordId(positionLabController.selectedPosition)
                                    )
                                }
                                AppButton {
                                    text: "Удалить"
                                    Layout.fillWidth: true
                                    enabled: page.recordId(
                                        positionLabController.selectedPosition
                                    ) > 0
                                    onClicked: {
                                        page.pendingDeleteId = page.recordId(
                                            positionLabController.selectedPosition
                                        )
                                        deleteDialog.open()
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Item {
                id: trainingMode

                RowLayout {
                    anchors.fill: parent
                    spacing: 12

                    ColumnLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        Layout.minimumWidth: 560
                        spacing: 8

                        RowLayout {
                            Layout.fillWidth: true

                            PanelTitle {
                                text: "Вспомните план или идею позиции"
                                Layout.fillWidth: true
                            }
                            Text {
                                text: positionLabController.trainingProgress
                                color: Theme.muted
                                font.family: Theme.fontFamily
                                font.pixelSize: 13
                                font.bold: true
                            }
                        }

                        Item {
                            Layout.fillWidth: true
                            Layout.fillHeight: true

                            ChessBoard {
                                id: trainingBoard
                                objectName: "positionLabTrainingBoard"
                                anchors.centerIn: parent
                                width: Math.min(parent.width, parent.height)
                                height: width
                                squares: positionLabController.boardSquares
                                arrows: positionLabController.arrows
                                flipped: positionLabController.flipped
                                annotationColor: page.annotationColor
                                editableAnnotations: false
                            }

                            Text {
                                anchors.centerIn: parent
                                visible: page.recordId(
                                    positionLabController.trainingPosition
                                ) <= 0
                                text: "В коллекции пока нет позиций"
                                color: Theme.faint
                                font.family: Theme.fontFamily
                                font.pixelSize: 14
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true

                            Text {
                                Layout.fillWidth: true
                                text: positionLabController.answerVisible
                                      ? "Ответ открыт: сравните со своей идеей"
                                      : "Комментарий и разметка скрыты"
                                color: positionLabController.answerVisible
                                       ? Theme.success : Theme.muted
                                font.family: Theme.fontFamily
                                font.pixelSize: 12
                            }
                            AppButton {
                                text: "Перевернуть"
                                enabled: page.recordId(
                                    positionLabController.trainingPosition
                                ) > 0
                                onClicked: positionLabController.flipBoard()
                            }
                        }
                    }

                    Rectangle {
                        Layout.preferredWidth: 360
                        Layout.minimumWidth: 330
                        Layout.fillHeight: true
                        color: Theme.panelBg
                        border.color: Theme.border
                        radius: 8

                        ColumnLayout {
                            anchors.fill: parent
                            anchors.margins: 14
                            spacing: 10

                            RowLayout {
                                Layout.fillWidth: true

                                PanelTitle {
                                    text: "Повторение"
                                    Layout.fillWidth: true
                                }
                                AppButton {
                                    text: "Сначала"
                                    Layout.preferredWidth: 86
                                    onClicked: positionLabController.startTraining()
                                }
                            }

                            Text {
                                Layout.fillWidth: true
                                text: page.valueOf(
                                    positionLabController.trainingPosition,
                                    "title",
                                    "Сохранённых позиций нет"
                                )
                                color: Theme.textStrong
                                font.family: Theme.fontFamily
                                font.pixelSize: 15
                                font.bold: true
                                wrapMode: Text.WordWrap
                            }

                            Text {
                                Layout.fillWidth: true
                                text: page.valueOf(
                                    positionLabController.trainingPosition,
                                    "move_label",
                                    ""
                                )
                                color: Theme.muted
                                font.family: Theme.fontFamily
                                font.pixelSize: 12
                            }

                            Rectangle {
                                Layout.fillWidth: true
                                Layout.preferredHeight: 1
                                color: Theme.border
                            }

                            Text {
                                Layout.fillWidth: true
                                visible: !positionLabController.answerVisible
                                         && page.recordId(
                                             positionLabController.trainingPosition
                                         ) > 0
                                text: "Не спешите. Сформулируйте своими словами план, ключевой манёвр или опасность в позиции. Затем откройте сохранённый ответ."
                                color: Theme.text
                                font.family: Theme.fontFamily
                                font.pixelSize: 13
                                wrapMode: Text.WordWrap
                            }

                            Item {
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                visible: !positionLabController.answerVisible
                            }

                            AppButton {
                                objectName: "positionLabShowAnswerButton"
                                text: "Показать ответ"
                                primary: true
                                Layout.fillWidth: true
                                visible: !positionLabController.answerVisible
                                enabled: page.recordId(
                                    positionLabController.trainingPosition
                                ) > 0
                                onClicked: positionLabController.showAnswer()
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                visible: positionLabController.answerVisible
                                spacing: 8

                                Text {
                                    text: "Комментарий"
                                    color: Theme.muted
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 10
                                }

                                Rectangle {
                                    Layout.fillWidth: true
                                    Layout.fillHeight: true
                                    Layout.minimumHeight: 180
                                    radius: 7
                                    color: Theme.rowBg
                                    border.color: Theme.borderStrong

                                    ScrollView {
                                        anchors.fill: parent
                                        anchors.margins: 9
                                        clip: true
                                        contentWidth: availableWidth

                                        Text {
                                            width: parent.availableWidth
                                            text: page.valueOf(
                                                positionLabController.trainingPosition,
                                                "comment",
                                                "Комментарий не добавлен."
                                            )
                                            color: Theme.text
                                            font.family: Theme.fontFamily
                                            font.pixelSize: 13
                                            wrapMode: Text.WordWrap
                                        }
                                    }
                                }

                                Text {
                                    Layout.fillWidth: true
                                    text: {
                                        var tags = page.valueOf(
                                            positionLabController.trainingPosition,
                                            "tags_text",
                                            ""
                                        )
                                        return tags ? "Теги: " + tags : "Без тегов"
                                    }
                                    color: Theme.muted
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 11
                                    wrapMode: Text.WordWrap
                                }

                                Text {
                                    Layout.fillWidth: true
                                    text: "Насколько хорошо вы вспомнили идею?"
                                    color: Theme.textStrong
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 12
                                    font.bold: true
                                }

                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 6

                                    AppButton {
                                        text: "Не вспомнил"
                                        Layout.fillWidth: true
                                        onClicked: positionLabController.rateAnswer("forgot")
                                    }
                                    AppButton {
                                        text: "Частично"
                                        Layout.fillWidth: true
                                        onClicked: positionLabController.rateAnswer("partial")
                                    }
                                    AppButton {
                                        text: "Вспомнил"
                                        primary: true
                                        Layout.fillWidth: true
                                        onClicked: positionLabController.rateAnswer("remembered")
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        StatusLine {
            Layout.fillWidth: true
            Layout.preferredHeight: 18
            status: positionLabController.status
        }
    }

    component ModeTab: TabButton {
        id: tabControl

        contentItem: Text {
            text: tabControl.text
            color: tabControl.checked ? Theme.accentText : Theme.muted
            font.family: Theme.fontFamily
            font.pixelSize: 13
            font.bold: tabControl.checked
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            radius: 7
            color: tabControl.checked ? Theme.accent
                                      : (tabControl.hovered ? Theme.buttonHover : "transparent")
        }
    }

    component PanelTitle: Text {
        color: Theme.textStrong
        font.family: Theme.fontFamily
        font.pixelSize: 13
        font.bold: true
        verticalAlignment: Text.AlignVCenter
    }

    component LabField: TextField {
        implicitHeight: 34
        leftPadding: 10
        rightPadding: 10
        color: Theme.text
        placeholderTextColor: Theme.faint
        selectedTextColor: Theme.selectionText
        selectionColor: Theme.selectionBg
        font.family: Theme.fontFamily
        font.pixelSize: 12
        background: Rectangle {
            radius: 7
            color: Theme.inputBg
            border.color: parent.activeFocus ? Theme.focus : Theme.borderStrong
        }
    }

    component LabArea: TextArea {
        leftPadding: 9
        rightPadding: 9
        topPadding: 7
        bottomPadding: 7
        color: Theme.text
        placeholderTextColor: Theme.faint
        selectedTextColor: Theme.selectionText
        selectionColor: Theme.selectionBg
        font.family: Theme.fontFamily
        font.pixelSize: 12
        wrapMode: TextEdit.Wrap
        background: Rectangle {
            radius: 7
            color: Theme.inputBg
            border.color: parent.activeFocus ? Theme.focus : Theme.borderStrong
        }
    }

    component PaletteSwatch: Button {
        id: swatch

        required property string swatchColor

        implicitWidth: 27
        implicitHeight: 27
        padding: 0
        onClicked: page.annotationColor = swatch.swatchColor
        background: Rectangle {
            radius: width / 2
            color: page.swatchColor(swatch.swatchColor)
            border.width: page.annotationColor === swatch.swatchColor ? 3 : 1
            border.color: page.annotationColor === swatch.swatchColor
                          ? Theme.textStrong : Theme.borderStrong
            scale: swatch.down ? 0.90 : (swatch.hovered ? 1.08 : 1.0)
        }
    }
}
