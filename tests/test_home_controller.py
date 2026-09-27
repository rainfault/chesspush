import json
from types import SimpleNamespace
import unittest
from PySide6.QtCore import QObject, Signal, QCoreApplication
from PySide6.QtNetwork import QNetworkReply, QNetworkRequest
from app.config import AppSettings
from backend.lichess_api import profile_ratings, rating_goal, build_user_profile_url
from controllers.home_controller import HomeController


class Reply(QObject):
    finished = Signal()
    def __init__(self):
        super().__init__()
        self.status = 200
        self.payload = b'{}'
        self.failure = QNetworkReply.NoError
        self.headers = {}
    def attribute(self, attribute): return self.status
    def readAll(self): return self.payload
    def rawHeader(self, key): return self.headers.get(key, b'')
    def error(self): return self.failure
    def abort(self):
        self.failure = QNetworkReply.OperationCanceledError
        self.finished.emit()
    def complete(self, rapid=1950, blitz=1880):
        self.payload = json.dumps({'perfs': {'rapid': {'rating': rapid, 'games': 100}, 'blitz': {'rating': blitz, 'games': 80}}}).encode()
        self.finished.emit()


class Network:
    def __init__(self): self.requests = []; self.replies = []
    def get(self, request):
        self.requests.append(request)
        reply = Reply()
        self.replies.append(reply)
        return reply


class HomeTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls): cls.app = QCoreApplication.instance() or QCoreApplication([])
    def setUp(self):
        self.context = SimpleNamespace(settings=AppSettings(lichess_username='example', lichess_api_token='private'))
        self.network = Network()
        self.home = HomeController(self.context, network=self.network)
    def tearDown(self): self.home.shutdown()

    def test_rating_boundaries(self):
        for rating in (None, 1000, 1899, 1900): self.assertFalse(rating_goal(rating)['visible'])
        self.assertEqual(rating_goal(1901)['value'], .01)
        self.assertEqual(rating_goal(1950)['value'], .5)
        self.assertEqual(rating_goal(2000)['value'], 1)
        self.assertEqual(rating_goal(2100)['value'], 1)

    def test_success_and_offline_clear_all_numbers(self):
        self.network.replies[-1].complete()
        self.assertEqual(self.home.rapid['rating'], '1950')
        self.assertTrue(self.home.rapid['showProgress'])
        self.assertFalse(self.home.blitz['showProgress'])
        self.home._last_request = -float('inf')
        self.home.refresh()
        self.network.replies[-1].failure = QNetworkReply.TimeoutError
        self.network.replies[-1].finished.emit()
        self.assertEqual(self.home.rapid['rating'], '')
        self.assertEqual(self.home.blitz['rating'], '')
        self.assertFalse(self.home.rapid['showProgress'])

    def test_public_request_never_sends_token(self):
        request = self.network.requests[0]
        self.assertEqual(request.url().host(), 'lichess.org')
        self.assertFalse(request.hasRawHeader('Authorization'))
        self.assertEqual(request.transferTimeout(), 8000)
        self.assertTrue(build_user_profile_url('a/b').endswith('a%2Fb'))

    def test_late_old_profile_cannot_overwrite_new_profile(self):
        old = self.network.replies[-1]
        self.context.settings.lichess_username = 'other'
        self.home.configure()
        self.network.replies[-1].complete(1975, 1999)
        old.complete(1100, 1200)
        self.assertEqual(self.home.rapid['rating'], '1975')

    def test_empty_profile_and_theme_change_do_not_poll(self):
        self.home.configure()
        self.assertEqual(len(self.network.requests), 1)
        self.context.settings.lichess_username = ''
        self.home.configure()
        self.home.refresh()
        self.assertEqual(len(self.network.requests), 1)
        self.assertEqual(self.home.rapid['rating'], '')

    def test_no_concurrent_requests_and_respects_rate_limit(self):
        self.home.refresh()
        self.assertEqual(len(self.network.requests), 1)
        reply = self.network.replies[-1]
        reply.status = 429
        reply.headers['Retry-After'] = b'120'
        reply.finished.emit()
        self.home._last_request = -float('inf')
        self.home.refresh()
        self.assertEqual(len(self.network.requests), 1)

    def test_invalid_data_and_unplayed_perfs_are_not_fake_ratings(self):
        self.network.replies[-1].payload = b'not JSON'
        self.network.replies[-1].finished.emit()
        self.assertEqual(self.home.rapid['rating'], '')
        self.assertEqual(profile_ratings({'perfs': {'rapid': {'rating': 1500, 'games': 0}}}), {'rapid': None, 'blitz': None})
        self.assertIsNone(profile_ratings({'perfs': {'rapid': {'rating': True, 'games': 1}}})['rapid'])
