import QtQuick 2.15
import QtQuick.Controls 2.15

ApplicationWindow {
    id: mainWindow

    height: 600
    width: 400

    Component.onCompleted: {
        console.log("Chess push")
        show()
    }
}