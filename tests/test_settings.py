from pathlib import Path
import tempfile
import unittest

from app.config import AppSettings, SettingsStore


class SettingsTest(unittest.TestCase):
    def test_lichess_token_round_trip(self):
        with tempfile.TemporaryDirectory() as tmp:
            store = SettingsStore(Path(tmp) / "settings.json")
            store.save(AppSettings(lichess_username="User", lichess_api_token="secret_token", theme="lichess"))
            loaded = store.load()
            self.assertEqual(loaded.lichess_username, "User")
            self.assertEqual(loaded.lichess_api_token, "secret_token")
            self.assertEqual(loaded.theme, "light")


if __name__ == "__main__":
    unittest.main()
