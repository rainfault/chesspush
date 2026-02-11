import sys
from PySide6.QtWidgets import QApplication
from ui.mainwindow import MainWindow
from config import VERSION

def main():
    print(f"Chess push version: {VERSION}")

    app = QApplication()
    
    mainWindow = MainWindow()
    mainWindow.show()

    sys.exit(app.exec())

if __name__ == '__main__':
    main()


    