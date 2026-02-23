import QtQuick 2.15
import QtQuick.Layouts
import QtQuick.Controls 2.15

ApplicationWindow {
    id: mainWindow

    visible: false

    height: 720
    width: 1280

    Component.onCompleted: {
        console.log("Chess push")
        show()
    }

    StackLayout {  
        id: mainStackLayout

        anchors.fill: parent
        
        LoginPage {
            Layout.fillHeight: true
            Layout.fillWidth: true

            onLoginReady: {
                mainStackLayout.currentIndex = 1
            }
        }

        ParserPage {
            Layout.fillHeight: true
            Layout.fillWidth: true
        }

        Component.onCompleted: {
            currentIndex = 0
        }
    }
}