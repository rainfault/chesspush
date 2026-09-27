"""Isolated Qt/WebEngine smoke test. Never reads the user's profile or games."""
import os
os.environ.setdefault('QT_QUICK_CONTROLS_STYLE', 'Basic')
import sys
from pathlib import Path
import tempfile
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from app.rendering import configure_rendering
configure_rendering()
from PySide6.QtCore import Qt, QCoreApplication, QTimer, QPointF, qInstallMessageHandler
from PySide6.QtGui import QGuiApplication, QFontDatabase
from PySide6.QtQml import QQmlApplicationEngine, QQmlExpression
from PySide6.QtQuick import QQuickItem
from PySide6.QtWebEngineQuick import QtWebEngineQuick
from PySide6.QtTest import QTest
from app.app_context import AppContext
from controllers.board_controller import BoardController
from controllers.parser_controller import ParserController
from controllers.settings_controller import SettingsController
from controllers.home_controller import HomeController

QCoreApplication.setAttribute(Qt.ApplicationAttribute.AA_ShareOpenGLContexts)
QtWebEngineQuick.initialize()
app = QGuiApplication([])
QFontDatabase.addApplicationFont(str(ROOT / 'ui/web/vendor/NotoSans.ttf'))
QFontDatabase.addApplicationFont(str(ROOT / 'ui/web/vendor/lichess.ttf'))
workspace = tempfile.TemporaryDirectory()
context = AppContext(Path(workspace.name))
board = BoardController(ROOT)
parser = ParserController(context, board)
settings = SettingsController(context)
home = HomeController(context)
engine = QQmlApplicationEngine()
for key, value in dict(boardController=board,parserController=parser,settingsController=settings,homeController=home).items():
    engine.rootContext().setContextProperty(key, value)
engine.addImportPath(str(ROOT / 'ui/qml'))
engine.load(str(ROOT / 'ui/qml/Main.qml'))
if not engine.rootObjects(): sys.exit(1)
window = engine.rootObjects()[0]
output = ROOT / 'outputs' / 'ui-check'
output.mkdir(exist_ok=True)
exit_code = 0

def run_checks():
    global exit_code
    try:
        view = window.findChild(QQuickItem, 'chessgroundView')
        assert view and view.width() > 400
        # Verify that the renderer has already loaded while Home covers it.
        assert view.property('loadProgress') == 100
        window.grabWindow().save(str(output / 'home-offline.png'))
        home._ratings = {'rapid': 1950, 'blitz': 1986}
        home.changed.emit()
        QTest.qWait(350)
        window.grabWindow().save(str(output / 'home-ratings-light.png'))
        settings.saveProfile('', '', 'dark')
        QTest.qWait(350)
        window.grabWindow().save(str(output / 'home-ratings-dark.png'))
        if '--home-only' in sys.argv:
            print('PASS: offline Home and rating layout in both themes', flush=True)
            return
        settings.saveProfile('', '', 'light')
        window.setProperty('page', 1)
        QTest.qWait(100)
        def point(file, rank):
            return view.mapToScene(QPointF((file + .5) * view.width()/8, (7-rank+.5)*view.height()/8)).toPoint()
        # These events travel through Qt -> Chromium -> real Chessground -> WebChannel.
        QTest.mouseClick(window, Qt.LeftButton, Qt.NoModifier, point(4,1))
        QTest.qWait(100)
        QTest.mouseClick(window, Qt.LeftButton, Qt.NoModifier, point(4,3))
        QTest.qWait(700)
        assert board.cursor == 1, f'Click-to-move failed: {board.state}'
        QTest.mousePress(window, Qt.LeftButton, Qt.NoModifier, point(4,6))
        QTest.qWait(200)
        QTest.mouseMove(window, point(4,5), 150)
        QTest.qWait(150)
        QTest.mouseMove(window, point(4,4), 150)
        QTest.qWait(150)
        QTest.mouseRelease(window, Qt.LeftButton, Qt.NoModifier, point(4,4))
        QTest.qWait(700)
        assert board.cursor == 2, 'Drag/drop failed'
        QTest.mousePress(window, Qt.RightButton, Qt.NoModifier, point(6,0))
        QTest.qWait(200)
        QTest.mouseMove(window, point(5,2), 200)
        QTest.qWait(200)
        QTest.mouseRelease(window, Qt.RightButton, Qt.NoModifier, point(5,2))
        QTest.qWait(400)
        assert board.session.annotations.get(2), 'Arrow drawing failed'
        QTest.mousePress(window, Qt.RightButton, Qt.NoModifier, point(3,3))
        QTest.qWait(100)
        QTest.mouseRelease(window, Qt.RightButton, Qt.NoModifier, point(3,3))
        QTest.qWait(400)
        assert len(board.session.annotations.get(2, [])) == 2, f'Circle drawing failed: {board.session.annotations}'
        window.grabWindow().save(str(output / 'parser-light.png'))
        settings.saveProfile('', '', 'dark')
        QTest.qWait(500)
        window.grabWindow().save(str(output / 'parser-dark.png'))
        # Native keyboard navigation and real promotion overlay.
        QTest.keyClick(window, Qt.Key_Left)
        QTest.qWait(350)
        assert board.cursor == 1, 'Keyboard back failed'
        QTest.keyClick(window, Qt.Key_Right)
        QTest.qWait(350)
        assert board.cursor == 2, 'Keyboard forward failed'
        board.session.load_fen('7k/P7/8/8/8/8/8/7K w - - 0 1')
        board.publish()
        QTest.qWait(400)
        QTest.mouseClick(window, Qt.LeftButton, Qt.NoModifier, point(0,6))
        QTest.qWait(100)
        QTest.mouseClick(window, Qt.LeftButton, Qt.NoModifier, point(0,7))
        QTest.qWait(400)
        assert board.cursor == 0, 'Promotion must wait for the user'
        QTest.mouseClick(window, Qt.LeftButton, Qt.NoModifier, point(0,6))
        QTest.qWait(500)
        import chess
        assert board.session.board.piece_at(chess.A8).piece_type == chess.KNIGHT, 'Underpromotion failed'
        board.reset()
        QTest.qWait(300)
        parser.selectSource(str(ROOT / 'sample_data/sample_games.pgn'))
        parser.extract(dict(minRating='0', maxRating='4000', limit='2', minMoves='0', includeTimeout=True))
        import time
        for _ in range(100):
            time.sleep(.02)  # Let the Python worker acquire the GIL between Qt test waits.
            QTest.qWait(100)
            if not parser.busy: break
        assert not parser.busy and not parser.error, parser.error
        assert context.storage.count_games() == 2
        assert len(parser.files) == 2
        panel = window.findChild(QQuickItem, 'parserPage')
        panel.setProperty('panel', 1)
        QTest.qWait(200)
        window.grabWindow().save(str(output / 'files-dark.png'))
        panel.setProperty('panel', 0)
        panel.setProperty('advanced', True)
        window.resize(1000, 700)
        QTest.qWait(500)
        window.grabWindow().save(str(output / 'parser-compact.png'))
        window.resize(1380, 900)
        settings.saveProfile('', '', 'light')
        window.setProperty('page', 0)
        QTest.qWait(300)
        window.grabWindow().save(str(output / 'home.png'))
        button = window.findChild(QQuickItem, 'settingsButton')
        QTest.mouseClick(window, Qt.LeftButton, Qt.NoModifier, button.mapToScene(QPointF(button.width()/2, button.height()/2)).toPoint())
        QTest.qWait(300)
        window.grabWindow().save(str(output / 'settings.png'))
        print('PASS: Qt shell, click/drag moves, arrows/circles, keyboard, underpromotion, QThread extraction + SQLite/PGN, files, compact layout, light/dark, settings', flush=True)
    except Exception as exc:
        exit_code = 1
        window.grabWindow().save(str(output / 'failure.png'))
        import traceback
        traceback.print_exc()
        print(f'FAIL: {exc}', flush=True)
    finally:
        app.quit()

QTimer.singleShot(5000, run_checks)
QTimer.singleShot(45000, app.quit)
app.exec()
parser.shutdown()
home.shutdown()
import shiboken6
shiboken6.delete(engine)
workspace.cleanup()
sys.exit(exit_code)
