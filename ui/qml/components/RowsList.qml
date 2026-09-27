import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ".."

Rectangle {
    id: root
    property var rows: []
    property string emptyText: "Нет данных"

    color: Theme.panelBg
    border.color: Theme.border
    radius: 7

    ScrollView {
        anchors.fill: parent
        anchors.margins: 8
        clip: true

        ListView {
            id: list
            model: root.rows
            spacing: 6

            delegate: Rectangle {
                width: ListView.view.width
                height: Math.max(58, content.implicitHeight + 16)
                color: index % 2 === 0 ? Theme.panelAlt : Theme.rowAlt
                radius: 7

                ColumnLayout {
                    id: content
                    anchors.fill: parent
                    anchors.margins: 8
                    spacing: 3

                    Text {
                        text: {
                            if (modelData.opening) return (modelData.eco || "") + " · " + modelData.opening
                            if (modelData.title) return modelData.title
                            if (modelData.name) return modelData.name + ": " + modelData.value
                            return JSON.stringify(modelData)
                        }
                        color: Theme.textStrong
                        font.family: Theme.fontFamily
                        font.pixelSize: 13
                        font.bold: true
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }

                    Text {
                        text: {
                            if (modelData.white || modelData.black) return (modelData.white || "") + " — " + (modelData.black || "") + " · " + (modelData.result || "") + " · " + (modelData.perf_type || "") + " · moves: " + (modelData.move_count || "")
                            if (modelData.problem) return modelData.problem
                            if (modelData.score) return "Партий: " + modelData.games + " · score: " + modelData.score + " · PainIndex: " + modelData.pain_index + " · " + modelData.priority
                            return modelData.value || ""
                        }
                        color: Theme.muted
                        font.family: Theme.fontFamily
                        font.pixelSize: 12
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }
                }
            }
        }
    }

    Text {
        anchors.centerIn: parent
        text: root.emptyText
        color: Theme.faint
        font.family: Theme.fontFamily
        visible: !root.rows || root.rows.length === 0
    }
}
