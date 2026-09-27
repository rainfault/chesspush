"""Isolated, bounded measurement of Chromium RAF and Qt swap pacing."""
import json
import os
import sys
import time
from pathlib import Path
os.environ.setdefault('QT_QUICK_CONTROLS_STYLE', 'Basic')
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
if '--baseline' not in sys.argv:
    from app.rendering import configure_rendering
    configure_rendering()
from PySide6.QtCore import QObject, Slot, Qt, QTimer, QCoreApplication
from PySide6.QtGui import QGuiApplication
from PySide6.QtQml import QQmlApplicationEngine, QQmlExpression
from PySide6.QtQuick import QQuickItem
from PySide6.QtWebEngineQuick import QtWebEngineQuick
from controllers.board_controller import BoardController

QCoreApplication.setAttribute(Qt.AA_ShareOpenGLContexts)
QtWebEngineQuick.initialize()
app = QGuiApplication([])
engine = QQmlApplicationEngine()
board = BoardController(ROOT)
swaps = []
class Probe(QObject):
    @Slot(str)
    def report(self, raw):
        data = json.loads(raw)
        data['screenHz'] = app.primaryScreen().refreshRate()
        data['qtSwapCount'] = len(swaps)
        data['presentedFps'] = (len(swaps) - 1) / (swaps[-1] - swaps[0]) if len(swaps) > 1 else 0
        print(json.dumps(data), flush=True)
        app.quit()
probe = Probe()
engine.rootContext().setContextProperty('boardController', board)
engine.rootContext().setContextProperty('probe', probe)
engine.loadData(b'''import QtQuick
import "../ui/qml/components"
Window { width: 800; height: 800; visible: true
    ChessgroundBoard { anchors.fill: parent }
}''', (ROOT/'tools/probe.qml').as_uri())
if not engine.rootObjects(): sys.exit(1)
window = engine.rootObjects()[0]
window.frameSwapped.connect(lambda: swaps.append(time.perf_counter()))
view = window.findChild(QQuickItem, 'chessgroundView')
def run(js, callback='function(r) {}'):
    expression = QQmlExpression(engine.rootContext(), view, f'runJavaScript({json.dumps(js)}, {callback})')
    expression.evaluate()
    if expression.hasError(): print(expression.error().toString())
def start():
    swaps.clear()
    run('''window.boardBenchmark = null;
        const deltas = []; let first, previous;
        const piece = document.querySelector('piece');
        function frame(t) {
            first ??= t;
            if (previous) deltas.push(t-previous);
            previous=t;
            if (piece) piece.style.translate = `${Math.sin(t/250)*30}px 0px`;
            if (t-first < 3000) requestAnimationFrame(frame);
            else {
                deltas.sort((a,b)=>a-b);
                window.boardBenchmark = {frames:deltas.length, fps:1000*deltas.length/deltas.reduce((a,b)=>a+b,0),
                    medianMs:deltas[Math.floor(deltas.length/2)], p95Ms:deltas[Math.floor(deltas.length*.95)],
                    dpr:devicePixelRatio, pieces:document.querySelectorAll('piece').length};
            }
        } requestAnimationFrame(frame);''')
    QTimer.singleShot(4000, lambda: run('window.boardBenchmark', 'function(r) { probe.report(JSON.stringify(r)) }'))
QTimer.singleShot(2500, start)
QTimer.singleShot(12000, app.quit)
app.exec()
import shiboken6
shiboken6.delete(engine)
