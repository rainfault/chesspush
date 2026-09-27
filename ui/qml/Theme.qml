pragma Singleton
import QtQuick
QtObject {
    property string name: "light"
    readonly property bool dark: name === "dark"
    function setTheme(value) { name = value === "dark" ? "dark" : "light" }
    readonly property string fontFamily: "Noto Sans"
    readonly property int body: 14
    readonly property int title: 22
    readonly property int spaceXS: 4
    readonly property int spaceS: 8
    readonly property int spaceM: 16
    readonly property int spaceL: 24
    readonly property int spaceXL: 32
    readonly property int radius: 3
    readonly property int controlHeight: 38
    function ratingProgressColor(value) {
        return Qt.hsla(88/360, .24 + .38 * value, dark ? .30 + .15 * value : .47 - .10 * value, 1)
    }
    readonly property color windowBg: dark ? Qt.hsla(37/360,.10,.08,1) : Qt.hsla(37/360,.10,.92,1)
    readonly property color bodyGradient: dark ? Qt.hsla(37/360,.12,.16,1) : Qt.hsla(37/360,.12,.84,1)
    readonly property color surface: dark ? Qt.hsla(37/360,.07,.14,1) : "#ffffff"
    readonly property color surfaceMid: dark ? Qt.hsla(37/360,.07,.16,1) : Qt.hsla(37/360,.07,.97,1)
    readonly property color surfaceLow: dark ? Qt.hsla(37/360,.07,.22,1) : "#e3e3e3"
    readonly property color inputBg: dark ? Qt.hsla(37/360,.10,.12,1) : Qt.hsla(37/360,.10,.97,1)
    readonly property color text: dark ? "#bababa" : "#4d4d4d"
    readonly property color textStrong: dark ? "#e3e3e3" : "#000000"
    readonly property color muted: dark ? "#949494" : "#787878"
    readonly property color border: dark ? "#404040" : "#d9d9d9"
    readonly property color accent: dark ? Qt.hsla(209/360,.79,.56,1) : Qt.hsla(209/360,.77,.46,1)
    readonly property color success: Qt.hsla(88/360,.62,.37,1)
    readonly property color warning: Qt.hsla(37/360,.74,dark ? .43 : .48,1)
    readonly property color error: Qt.hsla(0,.60,.50,1)
    readonly property color boardLight: "#f0d9b5"
    readonly property color boardDark: "#b58863"
    readonly property color buttonBg: surfaceMid
    readonly property color buttonHover: surfaceLow
    readonly property color buttonDown: border
    readonly property color focus: accent
    readonly property color selectionBg: accent
    readonly property color selectionText: "white"
    readonly property color accentText: "white"
    readonly property color accentHover: Qt.lighter(accent,1.1)
    readonly property color accentDown: Qt.darker(accent,1.1)
    // Compatibility with dormant pages.
    readonly property color pageBg: windowBg
    readonly property color sidebarBg: surface
    readonly property color sidebarText: text
    readonly property color sidebarMuted: muted
    readonly property color panelBg: surface
    readonly property color panelAlt: surfaceMid
    readonly property color rowBg: surface
    readonly property color rowAlt: surfaceMid
    readonly property color faint: muted
    readonly property color borderStrong: border
    readonly property color progressBg: surfaceLow
}
