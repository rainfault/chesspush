import json
from pathlib import Path
from PySide6.QtCore import QObject, Property, Signal, Slot, QUrl
from backend.board_session import BoardSession
from backend.search.opening_catalog import opening_book, normalize_query


class BoardController(QObject):
    changed = Signal()
    stateChanged = Signal(str)
    errorChanged = Signal()

    def __init__(self, base_dir: Path):
        super().__init__()
        self.session = BoardSession()
        self._state = self.session.snapshot()
        self._state_json = json.dumps(self._state)
        self._error = ''
        self._url = QUrl.fromLocalFile(str(base_dir / 'ui/web/board.html'))

    @Property(QUrl, constant=True)
    def pageUrl(self): return self._url

    @Property(str, notify=changed)
    def state(self): return self._state_json

    @Property(str, notify=changed)
    def pgn(self): return self._state['pgn']

    @Property(str, notify=changed)
    def fen(self): return self._state['fen']

    @Property(int, notify=changed)
    def cursor(self): return self._state['cursor']

    @Property(int, notify=changed)
    def total(self): return self._state['total']

    @Property('QVariantList', notify=changed)
    def moves(self): return self._state['moves']

    @Property(str, notify=errorChanged)
    def error(self): return self._error

    def publish(self):
        self._state = self.session.snapshot()
        self._state_json = json.dumps(self._state)
        self.changed.emit()
        self.stateChanged.emit(self.state)

    @Slot(str)
    def dispatch(self, message):
        try:
            data = json.loads(message)
            action = data.get('action')
            if action == 'move':
                self.session.play(data['from'], data['to'], data.get('promotion', ''), data['revision'])
            elif action == 'shapes':
                self.session.set_shapes(data['shapes'], data['revision'])
                return
            elif action == 'seek': self.session.seek(data['cursor'])
            elif action == 'flip':
                self.session.orientation = 'black' if self.session.orientation == 'white' else 'white'
                self.session.revision += 1
            self._error = ''
        except (ValueError, KeyError, TypeError) as exc:
            self._error = str(exc)
        self.errorChanged.emit()
        self.publish()

    @Slot(int)
    def seek(self, cursor):
        self.session.seek(cursor)
        self.publish()

    @Slot()
    def flip(self): self.dispatch('{"action":"flip"}')

    @Slot()
    def reset(self): self.loadPgn('')

    @Slot(str)
    def loadPgn(self, text):
        try:
            self.session.load_pgn(text)
            self._error = ''
        except ValueError as exc: self._error = str(exc)
        self.errorChanged.emit()
        self.publish()

    @Slot(str, result='QVariantList')
    def searchOpenings(self, query):
        terms = normalize_query(query).split()
        if not terms: return []
        return [entry.to_choice() for entry in opening_book().entries
                if all(term in normalize_query(entry.eco + ' ' + entry.name) for term in terms)][:60]
