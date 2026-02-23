import QtQuick 2.15
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    signal loginReady()

    Rectangle {
        id: background

        color: Colors.loginScreenBackground
        
        anchors.fill: parent
    }

    
    ColumnLayout {
        
        anchors.centerIn: parent
        spacing: 8

        Text {
            // anchors.centerIn: parent
            Layout.fillWidth: true

            text: "ChessPush"
            
            font {
                family: "Galmuri11"
                weight: Font.Normal
                pointSize: 48
            }

            color: "white"
        }

        TextField {
            id: loginInput
            Layout.preferredWidth: root.width / 6

            Layout.alignment: Qt.AlignCenter

            placeholderText: "Введите логин"

            font.family: "Galmuri11"

            Keys.onPressed: (event) => {
                if (event.key == Qt.Key_Enter || event.key == Qt.Key_Return) {
                    // NOTE: запускаю страницу со скриптом
                    root.loginReady()
                }
            }
        }

    }

    Text {
        text: "Version: 0.2.0"
        
        anchors {
            bottom: parent.bottom
            horizontalCenter: parent.horizontalCenter
            bottomMargin: 16
        }

        font {
            family: "Inter"
            weight: Font.Normal
            pointSize: 8
        }

        color: "white"
    }
}
