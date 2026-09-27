import unittest
from urllib.parse import parse_qs, urlparse

from backend.lichess_api import build_user_games_url


class LichessApiUrlTest(unittest.TestCase):
    def test_builds_latest_games_url_with_encoded_username(self):
        url = build_user_games_url("User name/with?reserved", 73)

        parsed = urlparse(url)
        query = parse_qs(parsed.query)

        self.assertEqual(parsed.scheme, "https")
        self.assertEqual(parsed.netloc, "lichess.org")
        self.assertEqual(
            parsed.path,
            "/api/games/user/User%20name%2Fwith%3Freserved",
        )
        self.assertEqual(query["max"], ["73"])
        self.assertEqual(query["sort"], ["dateDesc"])
        self.assertEqual(query["opening"], ["true"])
        self.assertEqual(query["tags"], ["true"])
        self.assertEqual(query["moves"], ["true"])
        self.assertNotIn("since", query)
        self.assertNotIn("until", query)

    def test_non_positive_max_games_is_normalized_to_one(self):
        for max_games in (0, -1, -100):
            with self.subTest(max_games=max_games):
                query = parse_qs(
                    urlparse(build_user_games_url("SomeUser", max_games)).query
                )
                self.assertEqual(query["max"], ["1"])

    def test_max_games_is_capped_for_a_bounded_download(self):
        query = parse_qs(
            urlparse(build_user_games_url("SomeUser", 999_999)).query
        )

        self.assertEqual(query["max"], ["5000"])


if __name__ == "__main__":
    unittest.main()
