import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import ".."
import "../components"

Item {
    id: page

    property string selectedEco: ""
    property string selectedOpening: ""
    property string selectedPrefix: ""
    property bool boardDragging: false
    property string boardDragFrom: ""
    property string boardDragAsset: ""
    property real boardDragX: 0
    property real boardDragY: 0

    function integerValue(value, fallbackValue) {
        var parsed = parseInt(value)
        return isNaN(parsed) ? fallbackValue : parsed
    }

    function boardSquareAt(x, y) {
        if (x < 0 || y < 0 || x >= boardGrid.width || y >= boardGrid.height)
            return ""
        var file = Math.floor(x / (boardGrid.width / 8))
        var row = Math.floor(y / (boardGrid.height / 8))
        return String.fromCharCode(97 + file) + (8 - row).toString()
    }

    function boardSquareData(name) {
        for (var i = 0; i < databaseLoaderController.boardSquares.length; i++) {
            var square = databaseLoaderController.boardSquares[i]
            if (square.name === name)
                return square
        }
        return null
    }

    function applyVariant(index) {
        page.selectedEco = databaseLoaderController.openingVariantValue(index, "eco")
        page.selectedOpening = databaseLoaderController.openingVariantValue(index, "opening_contains")
        page.selectedPrefix = databaseLoaderController.openingVariantValue(index, "move_prefix")
        databaseLoaderController.setBoardFromPgn(page.selectedPrefix)
        databaseLoaderController.syncOpeningSelection(page.selectedEco, page.selectedOpening, page.selectedPrefix)
        familySearch.text = page.selectedOpening
        variantSearch.text = ""
        familyList.currentIndex = 0
        variantList.currentIndex = 0
        syncEcoCombo()
    }

    function applyBoardMatch(index) {
        page.selectedEco = databaseLoaderController.boardMatchValue(index, "eco")
        page.selectedOpening = databaseLoaderController.boardMatchValue(index, "opening_contains")
        page.selectedPrefix = databaseLoaderController.boardMatchValue(index, "move_prefix")
        databaseLoaderController.syncOpeningSelection(page.selectedEco, page.selectedOpening, page.selectedPrefix)
        familySearch.text = page.selectedOpening
        variantSearch.text = ""
        familyList.currentIndex = 0
        variantList.currentIndex = 0
        syncEcoCombo()
    }

    function syncEcoCombo() {
        ecoChoice.currentIndex = 0
        for (var i = 0; i < databaseLoaderController.openingEcoOptions.length; i++) {
            if (databaseLoaderController.openingEcoOptions[i].eco === page.selectedEco) {
                ecoChoice.currentIndex = i
                return
            }
        }
    }

    function extractGames() {
        databaseLoaderController.extractGames(
            dbPath.text,
            color.currentText,
            resultMode.currentText,
            page.integerValue(wMin.text, settingsController.defaultMinRating),
            page.integerValue(wMax.text, settingsController.defaultMaxRating),
            page.integerValue(bMin.text, settingsController.defaultMinRating),
            page.integerValue(bMax.text, settingsController.defaultMaxRating),
            page.selectedEco,
            page.selectedOpening,
            page.selectedPrefix,
            perf.currentText,
            startIndex.text,
            Math.max(1, page.integerValue(limit.text, settingsController.defaultLimit)),
            outFolder.text,
            excludeBullet.checked,
            includeTimeout.checked
        )
    }

    FileDialog {
        id: databaseFileDialog
        title: "Выбрать ZST-базу"
        nameFilters: ["Lichess ZST databases (*.pgn.zst *.zst)", "All files (*)"]
        onAccepted: {
            dbPath.text = databaseLoaderController.pathFromUrl(selectedFile)
            databaseLoaderController.rememberDatabasePath(dbPath.text)
        }
    }

    FolderDialog {
        id: exportFolderDialog
        title: "Выбрать папку экспорта"
        onAccepted: outFolder.text = databaseLoaderController.pathFromUrl(selectedFolder)
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 12

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: 48
            spacing: 12

            ColumnLayout {
                Layout.fillWidth: true
                spacing: 1
                SectionTitle { text: "Выгрузка партий из ZST" }
                Text { text: "Потоковая распаковка · фильтрация · экспорт PGN"; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12 }
            }

            AppButton {
                text: databaseLoaderController.isExtracting ? "Идёт выгрузка..." : "Собрать PGN"
                primary: true
                Layout.preferredWidth: 156
                enabled: !databaseLoaderController.isExtracting
                onClicked: page.extractGames()
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 88
            color: Theme.panelAlt
            border.color: Theme.border
            radius: 7

            GridLayout {
                anchors.fill: parent
                anchors.margins: 12
                columns: 12
                columnSpacing: 10
                rowSpacing: 6

                LabeledField {
                    id: dbPath
                    Layout.columnSpan: 7
                    Layout.fillWidth: true
                    label: "ZST-база"
                    text: settingsController.zstDatabasePath
                }
                AppButton {
                    text: "Обзор"
                    Layout.columnSpan: 1
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignBottom
                    onClicked: databaseFileDialog.open()
                }
                LabeledField {
                    id: outFolder
                    Layout.columnSpan: 3
                    Layout.fillWidth: true
                    label: "Экспорт"
                    text: settingsController.exportFolder
                }
                AppButton {
                    text: "Папка"
                    Layout.columnSpan: 1
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignBottom
                    onClicked: exportFolderDialog.open()
                }
            }
        }

        RowLayout {
            id: workArea
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: 430
            spacing: 14

            Rectangle {
                Layout.preferredWidth: 790
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                color: Theme.panelAlt
                border.color: Theme.border
                radius: 7

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 12
                    spacing: 10

                    TabBar {
                        id: openingMode
                        objectName: "openingModeTabs"
                        Layout.fillWidth: true
                        Layout.preferredHeight: 40
                        background: Rectangle { color: Theme.progressBg; radius: 7 }

                        TabButton {
                            text: "Поиск дебюта"
                            contentItem: Text { text: parent.text; color: parent.checked ? Theme.accentText : Theme.muted; font.family: Theme.fontFamily; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter; font.bold: parent.checked }
                            background: Rectangle { radius: 7; color: parent.checked ? Theme.accent : "transparent" }
                        }
                        TabButton {
                            text: "Доска"
                            contentItem: Text { text: parent.text; color: parent.checked ? Theme.accentText : Theme.muted; font.family: Theme.fontFamily; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter; font.bold: parent.checked }
                            background: Rectangle { radius: 7; color: parent.checked ? Theme.accent : "transparent" }
                        }
                    }

                    StackLayout {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        currentIndex: openingMode.currentIndex

                        RowLayout {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            spacing: 12

                            ColumnLayout {
                                Layout.preferredWidth: 310
                                Layout.fillHeight: true
                                spacing: 8

                                TextField {
                                    id: familySearch
                                    Layout.fillWidth: true
                                    Layout.preferredHeight: 38
                                    placeholderText: "Caro-Kann, London, Sicilian..."
                                    color: Theme.text
                                    placeholderTextColor: Theme.faint
                                    font.family: Theme.fontFamily
                                    selectionColor: Theme.selectionBg
                                    selectedTextColor: Theme.selectionText
                                    background: Rectangle { radius: 8; color: Theme.inputBg; border.color: parent.activeFocus ? Theme.focus : Theme.borderStrong }
                                    onAccepted: databaseLoaderController.searchOpeningFamilies(text)
                                    onTextEdited: searchDebounce.restart()
                                }
                                Timer {
                                    id: searchDebounce
                                    interval: 220
                                    repeat: false
                                    onTriggered: databaseLoaderController.searchOpeningFamilies(familySearch.text)
                                }

                                ListView {
                                    id: familyList
                                    Layout.fillWidth: true
                                    Layout.fillHeight: true
                                    clip: true
                                    spacing: 6
                                    boundsBehavior: Flickable.StopAtBounds
                                    model: databaseLoaderController.openingFamilies
                                    delegate: Rectangle {
                                        width: ListView.view.width
                                        height: 56
                                        radius: 8
                                        color: ListView.isCurrentItem ? Theme.selectionBg : (familyMouse.containsMouse ? Theme.buttonHover : Theme.rowBg)
                                        border.color: ListView.isCurrentItem ? Theme.focus : Theme.borderStrong

                                        Column {
                                            anchors.fill: parent
                                            anchors.margins: 8
                                            spacing: 2
                                            Text { text: modelData.family; color: ListView.isCurrentItem ? Theme.selectionText : Theme.textStrong; font.family: Theme.fontFamily; font.pixelSize: 13; font.bold: true; elide: Text.ElideRight; width: parent.width }
                                            Text { text: modelData.detail; color: ListView.isCurrentItem ? Theme.selectionText : Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 11; elide: Text.ElideRight; width: parent.width }
                                        }

                                        MouseArea {
                                            id: familyMouse
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            onClicked: {
                                                familyList.currentIndex = index
                                                variantSearch.text = ""
                                                databaseLoaderController.selectOpeningFamily(index)
                                            }
                                        }
                                    }
                                }
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.fillHeight: true
                                spacing: 8

                                RowLayout {
                                    Layout.fillWidth: true
                                    spacing: 8
                                    AppComboBox {
                                        id: ecoChoice
                                        Layout.preferredWidth: 150
                                        model: databaseLoaderController.openingEcoOptions
                                        textRole: "label"
                                        onActivated: databaseLoaderController.selectOpeningEco(currentIndex)
                                    }
                                    TextField {
                                        id: variantSearch
                                        Layout.fillWidth: true
                                        Layout.preferredHeight: 38
                                        placeholderText: "Фильтр варианта"
                                        color: Theme.text
                                        placeholderTextColor: Theme.faint
                                        font.family: Theme.fontFamily
                                        selectionColor: Theme.selectionBg
                                        selectedTextColor: Theme.selectionText
                                        background: Rectangle { radius: 8; color: Theme.inputBg; border.color: parent.activeFocus ? Theme.focus : Theme.borderStrong }
                                        onTextEdited: databaseLoaderController.searchOpeningVariants(text)
                                    }
                                }

                                ListView {
                                    id: variantList
                                    Layout.fillWidth: true
                                    Layout.fillHeight: true
                                    clip: true
                                    spacing: 6
                                    boundsBehavior: Flickable.StopAtBounds
                                    model: databaseLoaderController.openingVariants
                                    delegate: Rectangle {
                                        width: ListView.view.width
                                        height: 66
                                        radius: 8
                                        color: variantMouse.containsMouse ? Theme.buttonHover : Theme.rowBg
                                        border.color: Theme.borderStrong

                                        Column {
                                            anchors.fill: parent
                                            anchors.margins: 8
                                            spacing: 3
                                            Text { text: modelData.eco + " · " + modelData.name; color: Theme.textStrong; font.family: Theme.fontFamily; font.pixelSize: 13; font.bold: true; elide: Text.ElideRight; width: parent.width }
                                            Text { text: modelData.pgn; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12; elide: Text.ElideRight; width: parent.width }
                                        }

                                        MouseArea {
                                            id: variantMouse
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            onClicked: page.applyVariant(index)
                                        }
                                    }
                                }
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            Layout.fillHeight: true
                            spacing: 14

                            ColumnLayout {
                                Layout.preferredWidth: 356
                                Layout.maximumWidth: 356
                                Layout.fillHeight: true
                                spacing: 8

                                Rectangle {
                                    id: boardFrame
                                    Layout.preferredWidth: 348
                                    Layout.preferredHeight: 348
                                    color: Theme.lichess ? "#c8c0b6" : "#302e2c"
                                    border.color: Theme.border
                                    radius: 7

                                    Item {
                                        id: boardGrid
                                        anchors.fill: parent
                                        anchors.margins: 8

                                        Repeater {
                                            model: databaseLoaderController.boardSquares
                                            delegate: Rectangle {
                                                x: (index % 8) * (boardGrid.width / 8)
                                                y: Math.floor(index / 8) * (boardGrid.height / 8)
                                                width: boardGrid.width / 8
                                                height: boardGrid.height / 8
                                                color: {
                                                    if (modelData.selected) return "#aaa23b"
                                                    if (modelData.target) return modelData.dark ? "#aaa23b" : "#cdd26a"
                                                    return modelData.dark ? "#b58863" : "#f0d9b5"
                                                }

                                                Image {
                                                    anchors.centerIn: parent
                                                    width: parent.width * 0.82
                                                    height: parent.height * 0.82
                                                    source: modelData.piece_asset ? "../assets/pieces/cburnett/" + modelData.piece_asset : ""
                                                    visible: !(page.boardDragging && page.boardDragFrom === modelData.name)
                                                    fillMode: Image.PreserveAspectFit
                                                    smooth: true
                                                    mipmap: true
                                                }

                                                Text {
                                                    anchors.left: parent.left
                                                    anchors.bottom: parent.bottom
                                                    anchors.leftMargin: 3
                                                    anchors.bottomMargin: 1
                                                    visible: modelData.name.charAt(1) === "1"
                                                    text: modelData.name.charAt(0)
                                                    color: modelData.dark ? "#f0d9b5" : "#b58863"
                                                    font.pixelSize: 10
                                                    font.bold: true
                                                }

                                                Text {
                                                    anchors.right: parent.right
                                                    anchors.top: parent.top
                                                    anchors.rightMargin: 3
                                                    anchors.topMargin: 1
                                                    visible: modelData.name.charAt(0) === "h"
                                                    text: modelData.name.charAt(1)
                                                    color: modelData.dark ? "#f0d9b5" : "#b58863"
                                                    font.pixelSize: 10
                                                    font.bold: true
                                                }
                                            }
                                        }

                                        MouseArea {
                                            id: boardDragArea
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            preventStealing: true

                                            onPressed: function(mouse) {
                                                var squareName = page.boardSquareAt(mouse.x, mouse.y)
                                                var square = page.boardSquareData(squareName)
                                                if (square && square.piece_asset && square.turn_piece) {
                                                    page.boardDragging = true
                                                    page.boardDragFrom = squareName
                                                    page.boardDragAsset = square.piece_asset
                                                    page.boardDragX = mouse.x
                                                    page.boardDragY = mouse.y
                                                    mouse.accepted = true
                                                } else {
                                                    page.boardDragging = false
                                                    page.boardDragFrom = ""
                                                    page.boardDragAsset = ""
                                                }
                                            }

                                            onPositionChanged: function(mouse) {
                                                if (!page.boardDragging)
                                                    return
                                                page.boardDragX = mouse.x
                                                page.boardDragY = mouse.y
                                            }

                                            onReleased: function(mouse) {
                                                var fromSquare = page.boardDragFrom
                                                var toSquare = page.boardSquareAt(mouse.x, mouse.y)
                                                var wasDragging = page.boardDragging
                                                page.boardDragging = false
                                                page.boardDragFrom = ""
                                                page.boardDragAsset = ""
                                                if (wasDragging && fromSquare && toSquare)
                                                    databaseLoaderController.moveBoardPiece(fromSquare, toSquare)
                                            }

                                            onClicked: function(mouse) {
                                                var squareName = page.boardSquareAt(mouse.x, mouse.y)
                                                if (squareName)
                                                    databaseLoaderController.clickBoardSquare(squareName)
                                            }
                                        }

                                        Image {
                                            visible: page.boardDragging && page.boardDragAsset
                                            width: boardGrid.width / 8 * 0.88
                                            height: boardGrid.height / 8 * 0.88
                                            x: page.boardDragX - width / 2
                                            y: page.boardDragY - height / 2
                                            source: page.boardDragAsset ? "../assets/pieces/cburnett/" + page.boardDragAsset : ""
                                            fillMode: Image.PreserveAspectFit
                                            smooth: true
                                            mipmap: true
                                            z: 10
                                        }
                                    }
                                }

                                RowLayout {
                                    Layout.fillWidth: true
                                    AppButton { text: "Сброс"; Layout.fillWidth: true; onClicked: databaseLoaderController.resetBoardOpening() }
                                    AppButton { text: "Назад"; Layout.fillWidth: true; onClicked: databaseLoaderController.undoBoardOpeningMove() }
                                }
                            }

                            ColumnLayout {
                                Layout.fillWidth: true
                                Layout.minimumWidth: 320
                                Layout.fillHeight: true
                                spacing: 8

                                Text {
                                    text: databaseLoaderController.boardPgn || "Начальная позиция"
                                    color: databaseLoaderController.boardPgn ? Theme.textStrong : Theme.muted
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 13
                                    elide: Text.ElideRight
                                    Layout.fillWidth: true
                                }

                                ListView {
                                    id: boardMatchList
                                    Layout.fillWidth: true
                                    Layout.fillHeight: true
                                    clip: true
                                    spacing: 6
                                    boundsBehavior: Flickable.StopAtBounds
                                    model: databaseLoaderController.boardMatches
                                    delegate: Rectangle {
                                        width: ListView.view.width
                                        height: 66
                                        radius: 8
                                        color: boardMouse.containsMouse ? Theme.buttonHover : Theme.rowBg
                                        border.color: Theme.borderStrong

                                        Column {
                                            anchors.fill: parent
                                            anchors.margins: 8
                                            spacing: 3
                                            Text { text: modelData.eco + " · " + modelData.name; color: Theme.textStrong; font.family: Theme.fontFamily; font.pixelSize: 13; font.bold: true; elide: Text.ElideRight; width: parent.width }
                                            Text { text: modelData.pgn; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: 12; elide: Text.ElideRight; width: parent.width }
                                        }

                                        MouseArea {
                                            id: boardMouse
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            onClicked: page.applyBoardMatch(index)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Rectangle {
                Layout.preferredWidth: 360
                Layout.minimumWidth: 340
                Layout.fillHeight: true
                clip: true
                color: Theme.panelAlt
                border.color: Theme.border
                radius: 7

                ScrollView {
                    anchors.fill: parent
                    anchors.margins: 12
                    clip: true
                    contentWidth: availableWidth

                    ColumnLayout {
                        width: parent.width
                        spacing: 10

                        Text { text: "Фильтр извлечения"; color: Theme.textStrong; font.family: Theme.fontFamily; font.pixelSize: 16; font.bold: true }

                        GridLayout {
                            Layout.fillWidth: true
                            columns: 2
                            columnSpacing: 10
                            rowSpacing: 8

                            AppComboBox { id: color; model: ["any", "white", "black"]; Layout.fillWidth: true }
                            AppComboBox { id: perf; model: ["any", "rapid", "blitz", "classical"]; Layout.fillWidth: true }
                            AppComboBox { id: resultMode; model: ["all", "selected_wins", "selected_losses", "wins_draws", "no_quick_losses"]; Layout.columnSpan: 2; Layout.fillWidth: true }
                            LabeledField { id: wMin; label: "White min"; text: settingsController.defaultMinRating.toString(); numeric: true; Layout.fillWidth: true }
                            LabeledField { id: wMax; label: "White max"; text: settingsController.defaultMaxRating.toString(); numeric: true; Layout.fillWidth: true }
                            LabeledField { id: bMin; label: "Black min"; text: settingsController.defaultMinRating.toString(); numeric: true; Layout.fillWidth: true }
                            LabeledField { id: bMax; label: "Black max"; text: settingsController.defaultMaxRating.toString(); numeric: true; Layout.fillWidth: true }
                            LabeledField { id: startIndex; label: "Стартовый индекс"; text: "0"; numeric: true; Layout.fillWidth: true }
                            LabeledField { id: limit; label: "Собрать партий"; text: settingsController.defaultLimit.toString(); numeric: true; Layout.fillWidth: true }
                            Text {
                                Layout.columnSpan: 2
                                Layout.fillWidth: true
                                text: "0 = с начала. N пропускает первые N партий; следующая проверяемая партия имеет индекс N + 1."
                                color: Theme.faint
                                font.family: Theme.fontFamily
                                font.pixelSize: 11
                                wrapMode: Text.WordWrap
                            }
                            AppButton {
                                text: "Продолжить с текущего индекса"
                                Layout.columnSpan: 2
                                Layout.fillWidth: true
                                enabled: !databaseLoaderController.isExtracting && databaseLoaderController.scannedGames > 0
                                onClicked: startIndex.text = databaseLoaderController.scannedGames.toString()
                            }
                        }

                        RowLayout {
                            Layout.fillWidth: true
                            AppCheckBox { id: excludeBullet; text: "Без bullet"; checked: settingsController.excludeBullet }
                            AppCheckBox { id: includeTimeout; text: "Timeout"; checked: false }
                        }

                        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.border }

                        Text { text: "Выбранный дебют"; color: Theme.textStrong; font.family: Theme.fontFamily; font.pixelSize: 14; font.bold: true }

                        Rectangle {
                            Layout.fillWidth: true
                            Layout.preferredHeight: 124
                            radius: 8
                            color: Theme.rowBg
                            border.color: page.selectedEco || page.selectedOpening || page.selectedPrefix ? Theme.accent : Theme.borderStrong

                            ColumnLayout {
                                anchors.fill: parent
                                anchors.margins: 10
                                spacing: 5
                                Text {
                                    Layout.fillWidth: true
                                    text: page.selectedEco || "ECO не выбран"
                                    color: page.selectedEco ? Theme.accentHover : Theme.faint
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 12
                                    font.bold: true
                                    elide: Text.ElideRight
                                }
                                Text {
                                    Layout.fillWidth: true
                                    text: page.selectedOpening || "Выбери дебют через поиск или доску"
                                    color: page.selectedOpening ? Theme.textStrong : Theme.muted
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 13
                                    elide: Text.ElideRight
                                }
                                Text {
                                    Layout.fillWidth: true
                                    Layout.fillHeight: true
                                    text: page.selectedPrefix || "Без префикса ходов: фильтр будет шире."
                                    color: page.selectedPrefix ? Theme.muted : Theme.faint
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 12
                                    wrapMode: Text.WordWrap
                                    elide: Text.ElideRight
                                }
                            }
                        }

                        AppButton {
                            text: "Очистить дебют"
                            Layout.fillWidth: true
                            onClicked: {
                                page.selectedEco = ""
                                page.selectedOpening = ""
                                page.selectedPrefix = ""
                                familySearch.text = ""
                                variantSearch.text = ""
                                familyList.currentIndex = -1
                                variantList.currentIndex = -1
                                ecoChoice.currentIndex = 0
                                databaseLoaderController.searchOpeningFamilies("")
                                databaseLoaderController.setBoardFromPgn("")
                            }
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 6
                            visible: databaseLoaderController.isExtracting || databaseLoaderController.scannedGames > 0

                            ProgressBar {
                                Layout.fillWidth: true
                                from: 0
                                to: 100
                                value: databaseLoaderController.progressPercent
                                indeterminate: databaseLoaderController.isExtracting && databaseLoaderController.progressPercent === 0
                                background: Rectangle { radius: 6; color: Theme.progressBg; border.color: Theme.borderStrong }
                            }

                            RowLayout {
                                Layout.fillWidth: true
                                Text {
                                    Layout.fillWidth: true
                                    text: "Текущий индекс: " + databaseLoaderController.scannedGames + " · найдено: " + databaseLoaderController.foundGames
                                    color: Theme.muted
                                    font.family: Theme.fontFamily
                                    font.pixelSize: 12
                                    elide: Text.ElideRight
                                }
                                AppButton {
                                    text: "Стоп"
                                    visible: databaseLoaderController.isExtracting
                                    Layout.preferredWidth: 82
                                    onClicked: databaseLoaderController.cancelExtraction()
                                }
                            }
                        }

                        StatusLine {
                            status: databaseLoaderController.status + (databaseLoaderController.outputPath ? " · " + databaseLoaderController.outputPath : "")
                            Layout.fillWidth: true
                        }

                        AppButton {
                            text: "Открыть выборку в Position Lab"
                            Layout.fillWidth: true
                            primary: true
                            enabled: !databaseLoaderController.isExtracting
                                     && databaseLoaderController.canOpenLastInPositionLab
                            onClicked: databaseLoaderController.openLastInPositionLab()
                        }
                    }
                }
            }
        }

        RowsList {
            Layout.fillWidth: true
            Layout.preferredHeight: 138
            rows: databaseLoaderController.rows
            emptyText: "Здесь появятся найденные партии"
        }
    }
}
