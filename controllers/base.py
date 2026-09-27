from __future__ import annotations

from PySide6.QtCore import QObject, Property, Signal


class BaseController(QObject):
    rowsChanged = Signal()
    statusChanged = Signal()

    def __init__(self):
        super().__init__()
        self._rows: list[dict] = []
        self._status = "Готово"

    @Property("QVariantList", notify=rowsChanged)
    def rows(self):
        return self._rows

    @Property(str, notify=statusChanged)
    def status(self):
        return self._status

    def set_rows(self, rows: list[dict]) -> None:
        self._rows = rows
        self.rowsChanged.emit()

    def set_status(self, text: str) -> None:
        self._status = text
        self.statusChanged.emit()
