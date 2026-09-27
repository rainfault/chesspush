import QtQuick
import QtQuick.Window
import QtWebEngine
import QtWebChannel
Item {
    id: surface
    // Rasterize SVGs above device resolution on fractional-DPI Windows screens.
    // Qt downsamples once; this avoids stair-stepped edges on cached piece layers.
    readonly property real renderScale: Screen.devicePixelRatio < 2 ? 2 : 1
    WebEngineView {
        width: surface.width * surface.renderScale
        height: surface.height * surface.renderScale
        scale: 1 / surface.renderScale
        zoomFactor: surface.renderScale
        transformOrigin: Item.TopLeft
        objectName: "chessgroundView"
        url: boardController.pageUrl
        webChannel: WebChannel { propertyUpdateInterval: -1; registeredObjects: [bridge] }
        smooth: true
        QtObject {
            id: bridge
            WebChannel.id: "board"
            readonly property string state: boardController.state
            function dispatch(message) { boardController.dispatch(message) }
        }
        backgroundColor: "transparent"
        settings.localContentCanAccessRemoteUrls: false
        settings.localContentCanAccessFileUrls: true
        settings.focusOnNavigationEnabled: false
        settings.errorPageEnabled: false
        settings.showScrollBars: false
        onLoadingChanged: function(request) { if (request.status === WebEngineView.LoadFailedStatus) console.error("Board:", request.errorString, request.errorCode, request.url) }
        onRenderProcessTerminated: function(status, code) { console.error("Board renderer exited:", code) }
        onContextMenuRequested: function(request) { request.accepted = true }
        onNavigationRequested: function(request) {
            if (request.url.toString() !== boardController.pageUrl.toString()) request.reject()
        }
    }
}
