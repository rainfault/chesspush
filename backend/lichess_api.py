from __future__ import annotations

import urllib.parse
import urllib.request


MAX_LICHESS_GAMES = 5_000


def build_user_profile_url(username: str) -> str:
    return 'https://lichess.org/api/user/' + urllib.parse.quote(username.strip(), safe='')


def profile_ratings(profile: dict) -> dict[str, int | None]:
    if not isinstance(profile, dict) or not isinstance(profile.get('perfs'), dict):
        raise ValueError('Invalid Lichess profile')
    result = {}
    for speed in ('rapid', 'blitz'):
        perf = profile['perfs'].get(speed, {})
        rating = perf.get('rating') if isinstance(perf, dict) else None
        games = perf.get('games', 0) if isinstance(perf, dict) else 0
        result[speed] = rating if type(rating) is int and rating > 0 and type(games) is int and games > 0 else None
    return result


def rating_goal(rating: int | None) -> dict:
    """The last 100 points to 2000; 1900 itself does not show a bar."""
    return {'visible': rating is not None and rating > 1900,
            'value': max(0.0, min(1.0, (rating - 1900) / 100)) if rating is not None else 0.0}


def normalize_max_games(max_games: int) -> int:
    return min(MAX_LICHESS_GAMES, max(1, int(max_games)))


def build_user_games_url(username: str, max_games: int) -> str:
    query = urllib.parse.urlencode(
        {
            "max": normalize_max_games(max_games),
            "opening": "true",
            "moves": "true",
            "tags": "true",
            "clocks": "false",
            "evals": "false",
            "sort": "dateDesc",
        }
    )
    encoded_username = urllib.parse.quote(username.strip(), safe="")
    return f"https://lichess.org/api/games/user/{encoded_username}?{query}"


def download_user_games_pgn(
    username: str,
    *,
    token: str = "",
    max_games: int,
    timeout: int = 60,
) -> tuple[str, str]:
    url = build_user_games_url(username, max_games)
    headers = {
        "Accept": "application/x-chess-pgn",
        "User-Agent": "ChessPush local training tool",
    }
    if token.strip():
        headers["Authorization"] = f"Bearer {token.strip()}"
    request = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(request, timeout=timeout) as response:
        body = response.read().decode("utf-8", errors="replace")
    return body, url
