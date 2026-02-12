import sys
from pathlib import Path
from PySide6.QtGui import QGuiApplication
from PySide6.QtQml import QQmlApplicationEngine

def main(): 
    app = QGuiApplication(sys.argv)
    engine = QQmlApplicationEngine()

    base = Path(__file__).resolve().parent
    engine.addImportPath(str(base / "qml"))
    engine.loadFromModule("ChessPush", "Main")

    if not engine.rootObjects(): 
        sys.exit(-1)
    sys.exit(app.exec())


if __name__ == '__main__':
    main()