pragma ComponentBehavior: Bound

import QtQuick

Item {
    id: root

    // squares: [{ name: "e4", piece_asset: "wP.svg", highlight: "green" }]
    // arrows:  [{ from: "e2", to: "e4", color: "green" }]
    property var squares: []
    property var arrows: []
    property bool flipped: false
    property string annotationColor: "green"
    property bool editableAnnotations: true

    signal arrowRequested(string fromSquare, string toSquare, string color)
    signal highlightRequested(string square, string color)

    implicitWidth: 480
    implicitHeight: 480

    readonly property real boardSize: Math.min(width, height)
    readonly property real squareSize: boardSize / 8

    function normalizedSquare(square) {
        var value = String(square || "").toLowerCase()
        if (!/^[a-h][1-8]$/.test(value))
            return ""
        return value
    }

    function squareNameForDisplayIndex(displayIndex) {
        var column = displayIndex % 8
        var row = Math.floor(displayIndex / 8)
        var fileIndex = root.flipped ? 7 - column : column
        var rankIndex = root.flipped ? row : 7 - row
        return String.fromCharCode(97 + fileIndex) + String(rankIndex + 1)
    }

    function squareAt(x, y) {
        if (x < 0 || y < 0 || x >= boardArea.width || y >= boardArea.height)
            return ""

        var column = Math.floor(x / root.squareSize)
        var row = Math.floor(y / root.squareSize)
        var fileIndex = root.flipped ? 7 - column : column
        var rankIndex = root.flipped ? row : 7 - row
        return String.fromCharCode(97 + fileIndex) + String(rankIndex + 1)
    }

    function squareCenter(square) {
        var value = root.normalizedSquare(square)
        if (!value)
            return null

        var fileIndex = value.charCodeAt(0) - 97
        var rankIndex = parseInt(value.charAt(1)) - 1
        var column = root.flipped ? 7 - fileIndex : fileIndex
        var row = root.flipped ? rankIndex : 7 - rankIndex
        return {
            x: (column + 0.5) * root.squareSize,
            y: (row + 0.5) * root.squareSize
        }
    }

    function squareData(square) {
        var items = root.squares || []
        for (var i = 0; i < items.length; ++i) {
            if (root.normalizedSquare(items[i].name) === square)
                return items[i]
        }
        return null
    }

    function pieceAsset(squareData) {
        if (!squareData)
            return ""
        return String(squareData.piece_asset || squareData.pieceAsset || "")
    }

    function highlightName(squareData) {
        if (!squareData)
            return ""

        var value = squareData.highlight_color
        if (value === undefined || value === null || value === "")
            value = squareData.highlightColor
        if (value === undefined || value === null || value === "")
            value = squareData.highlight
        if (value === true)
            return root.annotationColor
        if (value === false || value === undefined || value === null)
            return ""
        return String(value)
    }

    function paintColor(colorName, alpha) {
        switch (String(colorName || "green").toLowerCase()) {
        case "red":
            return Qt.rgba(0.88, 0.22, 0.20, alpha)
        case "blue":
            return Qt.rgba(0.16, 0.48, 0.88, alpha)
        case "yellow":
            return Qt.rgba(0.95, 0.72, 0.12, alpha)
        case "green":
        default:
            return Qt.rgba(0.20, 0.68, 0.30, alpha)
        }
    }

    function arrowEndpoint(arrow, first) {
        if (!arrow)
            return ""
        if (first)
            return arrow.from || arrow.from_square || arrow.fromSquare || ""
        return arrow.to || arrow.to_square || arrow.toSquare || ""
    }

    function drawArrow(context, x1, y1, x2, y2, colorName) {
        var dx = x2 - x1
        var dy = y2 - y1
        var distance = Math.sqrt(dx * dx + dy * dy)
        if (distance < 2)
            return

        var unitX = dx / distance
        var unitY = dy / distance
        var headLength = Math.min(root.squareSize * 0.34, distance * 0.42)
        var headWidth = root.squareSize * 0.19
        var shaftEndX = x2 - unitX * headLength * 0.72
        var shaftEndY = y2 - unitY * headLength * 0.72
        var baseX = x2 - unitX * headLength
        var baseY = y2 - unitY * headLength
        var perpendicularX = -unitY
        var perpendicularY = unitX
        var color = root.paintColor(colorName, 0.82)

        context.save()
        context.strokeStyle = color
        context.fillStyle = color
        context.lineWidth = Math.max(4, root.squareSize * 0.12)
        context.lineCap = "round"
        context.lineJoin = "round"

        context.beginPath()
        context.moveTo(x1, y1)
        context.lineTo(shaftEndX, shaftEndY)
        context.stroke()

        context.beginPath()
        context.moveTo(x2, y2)
        context.lineTo(baseX + perpendicularX * headWidth, baseY + perpendicularY * headWidth)
        context.lineTo(baseX - perpendicularX * headWidth, baseY - perpendicularY * headWidth)
        context.closePath()
        context.fill()
        context.restore()
    }

    Item {
        id: boardArea
        width: root.boardSize
        height: root.boardSize
        anchors.centerIn: parent
        clip: true

        Repeater {
            model: 64

            delegate: Rectangle {
                id: squareDelegate

                required property int index
                readonly property int displayColumn: index % 8
                readonly property int displayRow: Math.floor(index / 8)
                readonly property string squareName: root.squareNameForDisplayIndex(index)
                readonly property var squareState: root.squareData(squareName)
                readonly property bool darkSquare: (displayColumn + displayRow) % 2 === 1

                x: displayColumn * root.squareSize
                y: displayRow * root.squareSize
                width: root.squareSize
                height: root.squareSize
                color: darkSquare ? "#b58863" : "#f0d9b5"

                Rectangle {
                    anchors.fill: parent
                    color: root.paintColor(root.highlightName(squareDelegate.squareState), 0.48)
                    visible: root.highlightName(squareDelegate.squareState).length > 0
                }

                Image {
                    anchors.centerIn: parent
                    width: parent.width * 0.84
                    height: parent.height * 0.84
                    source: {
                        var asset = root.pieceAsset(squareDelegate.squareState)
                        return asset ? "../assets/pieces/cburnett/" + asset : ""
                    }
                    fillMode: Image.PreserveAspectFit
                    smooth: true
                    mipmap: true
                }

                Text {
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.leftMargin: Math.max(2, root.squareSize * 0.06)
                    anchors.topMargin: Math.max(1, root.squareSize * 0.025)
                    visible: squareDelegate.displayColumn === 0
                    text: squareDelegate.squareName.charAt(1)
                    color: squareDelegate.darkSquare ? "#f0d9b5" : "#b58863"
                    font.pixelSize: Math.max(9, root.squareSize * 0.18)
                    font.bold: true
                }

                Text {
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.rightMargin: Math.max(2, root.squareSize * 0.06)
                    anchors.bottomMargin: Math.max(1, root.squareSize * 0.025)
                    visible: squareDelegate.displayRow === 7
                    text: squareDelegate.squareName.charAt(0)
                    color: squareDelegate.darkSquare ? "#f0d9b5" : "#b58863"
                    font.pixelSize: Math.max(9, root.squareSize * 0.18)
                    font.bold: true
                }
            }
        }

        Canvas {
            id: arrowCanvas
            anchors.fill: parent
            z: 5

            onPaint: {
                var context = getContext("2d")
                context.clearRect(0, 0, width, height)

                var items = root.arrows || []
                for (var i = 0; i < items.length; ++i) {
                    var arrow = items[i]
                    var from = root.squareCenter(root.arrowEndpoint(arrow, true))
                    var to = root.squareCenter(root.arrowEndpoint(arrow, false))
                    if (from && to)
                        root.drawArrow(context, from.x, from.y, to.x, to.y, arrow.color || root.annotationColor)
                }

                if (annotationMouse.draggingArrow) {
                    var previewFrom = root.squareCenter(annotationMouse.startSquare)
                    if (previewFrom) {
                        root.drawArrow(
                            context,
                            previewFrom.x,
                            previewFrom.y,
                            annotationMouse.pointerX,
                            annotationMouse.pointerY,
                            root.annotationColor
                        )
                    }
                }
            }

            onWidthChanged: requestPaint()
            onHeightChanged: requestPaint()
        }

        MouseArea {
            id: annotationMouse
            anchors.fill: parent
            z: 10
            enabled: root.editableAnnotations
            acceptedButtons: Qt.RightButton
            preventStealing: true

            property string startSquare: ""
            property real pointerX: 0
            property real pointerY: 0
            property bool draggingArrow: false

            onPressed: function(mouse) {
                startSquare = root.squareAt(mouse.x, mouse.y)
                pointerX = mouse.x
                pointerY = mouse.y
                draggingArrow = startSquare.length > 0
                arrowCanvas.requestPaint()
            }

            onPositionChanged: function(mouse) {
                if (!draggingArrow)
                    return
                pointerX = Math.max(0, Math.min(width, mouse.x))
                pointerY = Math.max(0, Math.min(height, mouse.y))
                arrowCanvas.requestPaint()
            }

            onReleased: function(mouse) {
                var fromSquare = startSquare
                var toSquare = root.squareAt(mouse.x, mouse.y)
                startSquare = ""
                draggingArrow = false
                arrowCanvas.requestPaint()

                if (!fromSquare || !toSquare)
                    return
                if (fromSquare === toSquare)
                    root.highlightRequested(fromSquare, root.annotationColor)
                else
                    root.arrowRequested(fromSquare, toSquare, root.annotationColor)
            }

            onCanceled: {
                startSquare = ""
                draggingArrow = false
                arrowCanvas.requestPaint()
            }
        }
    }

    Connections {
        target: root

        function onArrowsChanged() {
            arrowCanvas.requestPaint()
        }

        function onFlippedChanged() {
            arrowCanvas.requestPaint()
        }

        function onAnnotationColorChanged() {
            arrowCanvas.requestPaint()
        }
    }
}
