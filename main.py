from __future__ import annotations

import os
import sys
from pathlib import Path

os.environ.setdefault("QT_QUICK_CONTROLS_STYLE", "Basic")
from app.rendering import configure_rendering
configure_rendering()

from PySide6.QtCore import QCoreApplication, Qt
from PySide6.QtGui import QGuiApplication, QIcon, QFontDatabase
from PySide6.QtQml import QQmlApplicationEngine
from PySide6.QtWebEngineQuick import QtWebEngineQuick

from app.app_context import AppContext
from controllers.board_controller import BoardController
from controllers.parser_controller import ParserController
from controllers.settings_controller import SettingsController
from controllers.home_controller import HomeController


def main() -> int:
    QCoreApplication.setOrganizationName("ChessPush")
    QCoreApplication.setApplicationName("ChessPush")
    QCoreApplication.setAttribute(Qt.ApplicationAttribute.AA_ShareOpenGLContexts)
    QtWebEngineQuick.initialize()
    app = QGuiApplication(sys.argv)
    base_dir = Path(__file__).resolve().parent
    QFontDatabase.addApplicationFont(str(base_dir / "ui/web/vendor/NotoSans.ttf"))
    QFontDatabase.addApplicationFont(str(base_dir / "ui/web/vendor/lichess.ttf"))
    app.setWindowIcon(QIcon(str(base_dir / "ui" / "qml" / "assets" / "app_icon_pawn.svg")))
    context = AppContext(base_dir=base_dir)

    settings_controller = SettingsController(context)
    board_controller = BoardController(base_dir)
    parser_controller = ParserController(context, board_controller)
    home_controller = HomeController(context)
    settings_controller.settingsChanged.connect(home_controller.configure)
    app.aboutToQuit.connect(home_controller.shutdown)
    app.aboutToQuit.connect(parser_controller.shutdown)

    engine = QQmlApplicationEngine()
    qml_context = engine.rootContext()
    qml_context.setContextProperty("settingsController", settings_controller)
    qml_context.setContextProperty("boardController", board_controller)
    qml_context.setContextProperty("parserController", parser_controller)
    qml_context.setContextProperty("homeController", home_controller)

    engine.addImportPath(str(base_dir / "ui" / "qml"))
    engine.load(str(base_dir / "ui" / "qml" / "Main.qml"))

    if not engine.rootObjects():
        return -1
    result = app.exec()
    # Dispose the view while its context objects still exist.
    del engine
    return result


if __name__ == "__main__":
    raise SystemExit(main())
