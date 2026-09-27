import QtQuick
import ".."

Text {
    property string status: ""
    text: status
    color: Theme.success
    font.family: Theme.fontFamily
    font.pixelSize: 12
    wrapMode: Text.WordWrap
}
