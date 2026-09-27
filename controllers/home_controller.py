"""Non-blocking public profile polling. No token is needed or sent."""
import json
import time
from PySide6.QtCore import QObject, Property, Signal, Slot, QTimer, QUrl
from PySide6.QtNetwork import QNetworkAccessManager, QNetworkReply, QNetworkRequest
from backend.lichess_api import build_user_profile_url, profile_ratings, rating_goal


class HomeController(QObject):
    changed = Signal()

    def __init__(self, context, *, network=None):
        super().__init__()
        self.context = context
        self.network = network or QNetworkAccessManager(self)
        self._username = ''
        self._generation = 0
        self._reply = None
        self._ratings = {'rapid': None, 'blitz': None}
        self._last_request = -float('inf')
        self._retry_at = 0
        self._timer = QTimer(self)
        self._timer.setInterval(60000)
        self._timer.timeout.connect(self.refresh)
        self._timer.start()
        self.configure()

    @Property('QVariantMap', notify=changed)
    def rapid(self): return self._view('rapid')

    @Property('QVariantMap', notify=changed)
    def blitz(self): return self._view('blitz')

    def _view(self, speed):
        rating = self._ratings[speed]
        goal = rating_goal(rating)
        return {'rating': str(rating) if rating is not None else '',
                'showProgress': goal['visible'], 'progress': goal['value']}

    @Slot()
    def configure(self):
        username = self.context.settings.lichess_username.strip()
        if username.casefold() == self._username.casefold(): return
        self._generation += 1
        reply, self._reply = self._reply, None
        if reply: reply.abort()
        self._username = username
        self._last_request = -float('inf')
        self._ratings = {'rapid': None, 'blitz': None}
        self.changed.emit()
        self.refresh()

    @Slot()
    def refresh(self):
        now = time.monotonic()
        if not self._username or self._reply is not None or now < self._retry_at or now - self._last_request < 15:
            return
        self._last_request = now
        request = QNetworkRequest(QUrl(build_user_profile_url(self._username)))
        request.setRawHeader(b'Accept', b'application/json')
        request.setRawHeader(b'User-Agent', b'ChessPush desktop')
        request.setTransferTimeout(8000)
        reply = self.network.get(request)
        self._reply = reply
        generation = self._generation
        reply.finished.connect(lambda: self._finished(reply, generation))

    def _finished(self, reply, generation):
        try:
            if generation != self._generation: return
            if self._reply is reply: self._reply = None
            status = reply.attribute(QNetworkRequest.HttpStatusCodeAttribute)
            if status == 429:
                try: delay = int(bytes(reply.rawHeader('Retry-After')))
                except ValueError: delay = 60
                self._retry_at = time.monotonic() + max(60, delay)
            ratings = {'rapid': None, 'blitz': None}
            if reply.error() == QNetworkReply.NoError and status == 200:
                try: ratings = profile_ratings(json.loads(bytes(reply.readAll())))
                except (ValueError, TypeError): pass
            self._ratings = ratings
            self.changed.emit()
        finally:
            reply.deleteLater()

    @Slot()
    def shutdown(self):
        self._timer.stop()
        self._generation += 1
        reply, self._reply = self._reply, None
        if reply: reply.abort()
