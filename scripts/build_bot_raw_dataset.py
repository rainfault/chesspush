from __future__ import annotations

import json
import re
import time
from collections import Counter
from pathlib import Path
from typing import Optional

import requests

from build_raw_dataset import build_record


LICHESS_API_BASE = "https://lichess.org/api/games/user"


def load_bot_usernames(path: str) -> list[str]:
    file_path = Path(path)
    if not file_path.exists():
        raise FileNotFoundError(f"Bot usernames file not found: {file_path}")

    usernames: list[str] = []
    with open(file_path, "r", encoding="utf-8") as f:
        for line in f:
            username = line.strip()
            if not username or username.startswith("#"):
                continue
            usernames.append(username)

    if not usernames:
        raise ValueError("No bot usernames found in file")

    return usernames


def export_user_games_pgn(
    username: str,
    max_games_per_user: Optional[int] = None,
    perf_type: str = "rapid",
    rated: bool = True,
    clocks: bool = True,
    evals: bool = False,
    opening: bool = True,
    moves: bool = True,
    literate: bool = False,
    finished: bool = True,
) -> str:
    """
    Скачивает партии пользователя в формате PGN одним текстом.
    """
    url = f"{LICHESS_API_BASE}/{username}"

    params: dict[str, object] = {
        "perfType": perf_type,
        "rated": str(rated).lower(),
        "clocks": str(clocks).lower(),
        "evals": str(evals).lower(),
        "opening": str(opening).lower(),
        "moves": str(moves).lower(),
        "literate": str(literate).lower(),
        "finished": str(finished).lower(),
        "pgnInJson": "false",
    }

    if max_games_per_user is not None:
        params["max"] = max_games_per_user

    headers = {
        "Accept": "application/x-chess-pgn",
        "User-Agent": "chess-dissertation-bot-dataset-builder/1.0",
    }

    response = requests.get(url, params=params, headers=headers, timeout=120)
    response.raise_for_status()
    return response.text


def split_pgn_text_into_games(pgn_text: str) -> list[str]:
    """
    Надёжно разбивает большой PGN-текст на отдельные партии
    по реальным началам блоков [Event ...].
    """
    pgn_text = pgn_text.strip()
    if not pgn_text:
        return []

    starts = list(re.finditer(r"(?m)^\[Event ", pgn_text))
    if not starts:
        return []

    games: list[str] = []

    for i, match in enumerate(starts):
        start = match.start()
        end = starts[i + 1].start() if i + 1 < len(starts) else len(pgn_text)
        game = pgn_text[start:end].strip()
        if game:
            games.append(game)

    return games


def is_allowed_rapid_time_control(
    time_control: str,
    allowed_time_controls: Optional[set[str]],
) -> bool:
    if not time_control:
        return False

    if allowed_time_controls is None:
        return True

    return time_control in allowed_time_controls


def check_bot_record_reason(
    record: dict,
    min_elo: int,
    max_elo: int,
    min_plies: int,
    allowed_time_controls: Optional[set[str]] = None,
    require_clock_data: bool = True,
) -> Optional[str]:
    """
    Проверка записи именно для bot-корпуса.

    Здесь мы не завязываемся жёстко на Event, потому что формат API-выгрузки
    может отличаться от месячных дампов.
    """
    variant = (record.get("variant") or "Standard").strip()
    white_elo = record.get("white_elo")
    black_elo = record.get("black_elo")
    time_control = (record.get("time_control") or "").strip()
    termination = (record.get("termination") or "").strip()
    ply_count = record.get("ply_count") or 0
    has_clock_data = bool(record.get("has_clock_data"))
    game_id = record.get("game_id")

    if not game_id:
        return "missing_game_id"

    if variant != "Standard":
        return "non_standard_variant"

    if white_elo is None or black_elo is None:
        return "missing_elo"

    if not (min_elo <= white_elo <= max_elo):
        return "white_elo_out_of_range"

    if not (min_elo <= black_elo <= max_elo):
        return "black_elo_out_of_range"

    if not time_control:
        return "missing_time_control"

    if not is_allowed_rapid_time_control(time_control, allowed_time_controls):
        return "time_control_not_allowed"

    if termination != "Normal":
        return "non_normal_termination"

    if ply_count < min_plies:
        return "too_short_game"

    if require_clock_data and not has_clock_data:
        return "missing_clock_data"

    return None


def build_summary(
    games_seen: int,
    games_saved: int,
    reject_stats: Counter[str],
    saved_time_controls: Counter[str],
    saved_openings: Counter[str],
    saved_by_bot: Counter[str],
    seen_events: Counter[str],
) -> dict:
    return {
        "games_seen": games_seen,
        "games_saved": games_saved,
        "reject_stats": dict(reject_stats),
        "saved_time_controls": dict(saved_time_controls),
        "top_saved_openings": dict(saved_openings.most_common(20)),
        "saved_by_bot": dict(saved_by_bot),
        "seen_events": dict(seen_events),
    }


def save_summary(summary_path: str, summary: dict) -> None:
    file_path = Path(summary_path)
    file_path.parent.mkdir(parents=True, exist_ok=True)

    with open(file_path, "w", encoding="utf-8") as f:
        json.dump(summary, f, ensure_ascii=False, indent=2)


def build_bot_raw_dataset(
    usernames_file: str,
    output_path: str,
    summary_path: str,
    min_elo: int = 1800,
    max_elo: int = 2400,
    min_plies: int = 20,
    allowed_time_controls: Optional[set[str]] = None,
    require_clock_data: bool = True,
    max_games_total: Optional[int] = 500,
    max_games_per_user: int = 100,
    max_saved_games_per_bot: Optional[int] = None,
    request_pause_sec: float = 1.0,
    print_first_game_preview: bool = True,
) -> None:
    bot_usernames = load_bot_usernames(usernames_file)

    output_file = Path(output_path)
    output_file.parent.mkdir(parents=True, exist_ok=True)

    games_seen = 0
    games_saved = 0

    reject_stats: Counter[str] = Counter()
    saved_time_controls: Counter[str] = Counter()
    saved_openings: Counter[str] = Counter()
    saved_by_bot: Counter[str] = Counter()
    seen_events: Counter[str] = Counter()

    seen_game_ids: set[str] = set()
    preview_printed_for_users: set[str] = set()

    with open(output_file, "w", encoding="utf-8") as out:
        for idx, username in enumerate(bot_usernames, start=1):
            print(f"\n=== [{idx}/{len(bot_usernames)}] Downloading games for {username} ===")

            try:
                pgn_text = export_user_games_pgn(
                    username=username,
                    max_games_per_user=max_games_per_user,
                    perf_type="rapid",
                    rated=True,
                    clocks=True,
                    evals=False,
                    opening=True,
                    moves=True,
                    literate=False,
                    finished=True,
                )
            except requests.HTTPError as e:
                print(f"HTTP error for {username}: {e}")
                continue
            except requests.RequestException as e:
                print(f"Request error for {username}: {e}")
                continue

            event_count = pgn_text.count('[Event "')
            games = split_pgn_text_into_games(pgn_text)

            print(f'Raw [Event "..."] count: {event_count}')
            print(f"Downloaded games after split: {len(games)}")

            if print_first_game_preview and username not in preview_printed_for_users and games:
                print("\nFIRST GAME RAW PREVIEW:")
                print(games[0][:1500])

                test_record = build_record(games[0])
                print("FIRST GAME PARSED FIELDS:")
                print("event =", repr(test_record.get("event")))
                print("time_control =", repr(test_record.get("time_control")))
                print("white_elo =", repr(test_record.get("white_elo")))
                print("black_elo =", repr(test_record.get("black_elo")))
                print("termination =", repr(test_record.get("termination")))
                print("variant =", repr(test_record.get("variant")))
                print("ply_count =", repr(test_record.get("ply_count")))
                print("has_clock_data =", repr(test_record.get("has_clock_data")))
                preview_printed_for_users.add(username)

            for pgn in games:
                games_seen += 1

                record = build_record(pgn)
                game_id = record.get("game_id")
                event = (record.get("event") or "").strip()

                if event:
                    seen_events[event] += 1
                else:
                    seen_events["<EMPTY_EVENT>"] += 1

                if not game_id:
                    reject_stats["missing_game_id"] += 1
                    continue

                if game_id in seen_game_ids:
                    reject_stats["duplicate_game_id"] += 1
                    continue

                reason = check_bot_record_reason(
                    record=record,
                    min_elo=min_elo,
                    max_elo=max_elo,
                    min_plies=min_plies,
                    allowed_time_controls=allowed_time_controls,
                    require_clock_data=require_clock_data,
                )

                if reason is not None:
                    reject_stats[reason] += 1
                    continue

                if max_saved_games_per_bot is not None and saved_by_bot[username] >= max_saved_games_per_bot:
                    reject_stats["per_bot_cap_reached"] += 1
                    continue

                seen_game_ids.add(game_id)

                record["source_class"] = "bot"
                record["source_username"] = username

                out.write(json.dumps(record, ensure_ascii=False) + "\n")
                games_saved += 1

                time_control = record.get("time_control") or "UNKNOWN"
                opening = record.get("opening") or "UNKNOWN"

                saved_time_controls[time_control] += 1
                saved_openings[opening] += 1
                saved_by_bot[username] += 1

                if games_saved % 25 == 0:
                    print(f"saved total: {games_saved}")

                if max_games_total is not None and games_saved >= max_games_total:
                    summary = build_summary(
                        games_seen=games_seen,
                        games_saved=games_saved,
                        reject_stats=reject_stats,
                        saved_time_controls=saved_time_controls,
                        saved_openings=saved_openings,
                        saved_by_bot=saved_by_bot,
                        seen_events=seen_events,
                    )
                    save_summary(summary_path, summary)

                    print("\nReached max_games_total limit.")
                    print(f"games_seen={games_seen}, games_saved={games_saved}")
                    print("reject stats:", dict(reject_stats))
                    return

            time.sleep(request_pause_sec)

    summary = build_summary(
        games_seen=games_seen,
        games_saved=games_saved,
        reject_stats=reject_stats,
        saved_time_controls=saved_time_controls,
        saved_openings=saved_openings,
        saved_by_bot=saved_by_bot,
        seen_events=seen_events,
    )
    save_summary(summary_path, summary)

    print("\nBot dataset build finished.")
    print(f"games_seen={games_seen}, games_saved={games_saved}")
    print("reject stats:", dict(reject_stats))


if __name__ == "__main__":
    build_bot_raw_dataset(
        usernames_file="bot_usernames.txt",
        output_path="data/raw/bot_games_raw_rapid_1400_2800.jsonl",
        summary_path="data/raw/bot_games_raw_rapid_1400_2800_summary.json",
        min_elo=1400,
        max_elo=2800,
        min_plies=20,
        allowed_time_controls={"600+0", "600+5"},
        require_clock_data=True,
        max_games_total=350,
        max_games_per_user=200,
        max_saved_games_per_bot=60,
        request_pause_sec=0.5,
        print_first_game_preview=False,
    )