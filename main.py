import sys
from pathlib import Path
from PySide6.QtGui import QGuiApplication, QFontDatabase
from PySide6.QtQml import QQmlApplicationEngine

from services.database_manager import DatabaseManager

from config import MAIN_FONT, PIXELATED_FONT

def main(): 
    app = QGuiApplication(sys.argv)
    engine = QQmlApplicationEngine()

    base = Path(__file__).resolve().parent
    print("Base folder: ", base)

    if (font_id := QFontDatabase.addApplicationFont(str(base / "qml" / "fonts" / MAIN_FONT))) != -1:
        print("Font successfuly added")
        families = QFontDatabase.applicationFontFamilies(font_id)
        # print("Loaded families:", famiDatabaseManagerlies)
    else:
        print("Error while adding font")

    if (font_id := QFontDatabase.addApplicationFont(str(base / "qml" / "fonts" / PIXELATED_FONT))) != -1:
        print("Font successfuly added")
        families = QFontDatabase.applicationFontFamilies(font_id)
        # print("Loaded families:", families)
    else:
        print("Error while adding font")

    engine.addImportPath(str(base / "qml"))
    engine.loadFromModule("ChessPush", "Main")

    # Регистрирую шрифты
    # TODO: добавить модуль для регистрации

    # Создаю базу данных

    manager = DatabaseManager()
    manager.open_connection()


    if not engine.rootObjects(): 
        sys.exit(-1)
    sys.exit(app.exec())


if __name__ == '__main__':
    main()