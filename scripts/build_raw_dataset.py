from __future__ import annotations

import codecs
import json
import os
import re
from collections import Counter
from pathlib import Path
from typing import Optional

from zstandard import ZstdDecompressor


HEADER_RE = re.compile(r'^\[(\w+)\s+"(.*)"\]$', re.MULTILINE)
RESULT_TOKENS = {"1-0", "0-1", "1/2-1/2", "*"}
CLK_RE = re.compile(r"\[%clk\s+([^\]]+)\]")
EVAL_RE = re.compile(r"\[%eval\s+([^\]]+)\]")


def parse_chunk(data: bytes, tail: str, decoder) -> tuple[list[str], str]:
    """
    Декодирует очередной бинарный чанк и разбивает его на PGN-партии.

    Возвращает:
    - список полных партий
    - хвост последней неполной партии, который нужно склеить со следующим чанком
    """
    text = decoder.decode(data)
    parts = text.split("\n\n[")

    pgns: list[str] = []
    last_part = ""

    total_parts = len(parts)

    for index, game in enumerate(parts):
        if index == 0:
            game = tail + game

        if not game.startswith("["):
            game = "[" + game

        if index == total_parts - 1:
            last_part = game
            break

        pgns.append(game)

    return pgns, last_part


def extract_headers(pgn: str) -> dict[str, str]:
    headers: dict[str, str] = {}

    header_part, _, _ = pgn.partition("\n\n")
    for match in HEADER_RE.finditer(header_part):
        key, value = match.groups()
        headers[key] = value

    return headers


def extract_moves_text(pgn: str) -> str:
    _, sep, moves = pgn.partition("\n\n")
    return moves.strip() if sep else ""


def safe_int(value: Optional[str]) -> Optional[int]:
    if value is None or value == "":
        return None

    try:
        return int(value)
    except ValueError:
        return None


def estimate_ply_count(moves_text: str) -> int:
    """
    Приблизительно считает число полуходов в партии.
    Для этапа 1 этого достаточно.
    """
    if not moves_text:
        return 0

    text = re.sub(r"\{[^}]*\}", " ", moves_text)
    text = re.sub(r";[^\n]*", " ", text)
    text = re.sub(r"\([^)]*\)", " ", text)

    tokens = text.split()
    ply_count = 0

    for token in tokens:
        if token in RESULT_TOKENS:
            continue

        if re.match(r"^\d+\.+$", token):
            continue
        if re.match(r"^\d+\.(\.\.)?$", token):
            continue

        ply_count += 1

    return ply_count


def count_clock_tags(moves_text: str) -> int:
    return len(CLK_RE.findall(moves_text))


def count_eval_tags(moves_text: str) -> int:
    return len(EVAL_RE.findall(moves_text))


def build_record(pgn: str) -> dict:
    headers = extract_headers(pgn)
    moves_text = extract_moves_text(pgn)
    ply_count = estimate_ply_count(moves_text)

    site = headers.get("Site", "")
    game_id = site.rstrip("/").split("/")[-1] if site else None

    clock_tag_count = count_clock_tags(moves_text)
    eval_tag_count = count_eval_tags(moves_text)

    return {
        "game_id": game_id,
        "event": headers.get("Event"),
        "site": site,
        "date": headers.get("Date"),
        "utc_date": headers.get("UTCDate"),
        "utc_time": headers.get("UTCTime"),
        "white": headers.get("White"),
        "black": headers.get("Black"),
        "white_elo": safe_int(headers.get("WhiteElo")),
        "black_elo": safe_int(headers.get("BlackElo")),
        "white_rating_diff": headers.get("WhiteRatingDiff"),
        "black_rating_diff": headers.get("BlackRatingDiff"),
        "result": headers.get("Result"),
        "eco": headers.get("ECO"),
        "opening": headers.get("Opening"),
        "time_control": headers.get("TimeControl"),
        "termination": headers.get("Termination"),
        "variant": headers.get("Variant"),
        "ply_count": ply_count,
        "clock_tag_count": clock_tag_count,
        "eval_tag_count": eval_tag_count,
        "has_clock_data": clock_tag_count > 0,
        "has_eval_data": eval_tag_count > 0,
        "moves_text": moves_text,
        "pgn": pgn.strip(),
    }


def check_record_reason(
    record: dict,
    min_elo: int,
    max_elo: int,
    min_plies: int,
    required_event: str,
    allowed_time_controls: Optional[set[str]] = None,
    require_clock_data: bool = False,
) -> Optional[str]:
    event = (record.get("event") or "").strip()
    variant = (record.get("variant") or "Standard").strip()
    white_elo = record.get("white_elo")
    black_elo = record.get("black_elo")
    time_control = (record.get("time_control") or "").strip()
    termination = (record.get("termination") or "").strip()
    ply_count = record.get("ply_count") or 0
    has_clock_data = bool(record.get("has_clock_data"))

    if variant != "Standard":
        return "non_standard_variant"

    if event != required_event:
        return "wrong_event"

    if white_elo is None or black_elo is None:
        return "missing_elo"

    if not (min_elo <= white_elo <= max_elo):
        return "white_elo_out_of_range"

    if not (min_elo <= black_elo <= max_elo):
        return "black_elo_out_of_range"

    if not time_control:
        return "missing_time_control"

    if allowed_time_controls is not None and time_control not in allowed_time_controls:
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
) -> dict:
    return {
        "games_seen": games_seen,
        "games_saved": games_saved,
        "reject_stats": dict(reject_stats),
        "saved_time_controls": dict(saved_time_controls),
        "top_saved_openings": dict(saved_openings.most_common(20)),
    }


def save_summary(summary_path: Optional[str], summary: dict) -> None:
    if summary_path is None:
        return

    summary_file = Path(summary_path)
    summary_file.parent.mkdir(parents=True, exist_ok=True)

    with open(summary_file, "w", encoding="utf-8") as f:
        json.dump(summary, f, ensure_ascii=False, indent=2)


def decompress_database_to_jsonl(
    source_path: str,
    output_path: str,
    min_elo: int = 1800,
    max_elo: int = 2400,
    min_plies: int = 20,
    required_event: str = "Rated Rapid game",
    allowed_time_controls: Optional[set[str]] = None,
    require_clock_data: bool = True,
    max_games: Optional[int] = None,
    chunk_size: int = 1_638_400,
    print_first_accepted: int = 3,
    print_first_rejected: int = 3,
    summary_path: Optional[str] = None,
) -> None:
    source = Path(source_path)
    output = Path(output_path)
    output.parent.mkdir(parents=True, exist_ok=True)

    if not source.exists():
        raise FileNotFoundError(f"Source file not found: {source}")

    chunks_total = 0
    games_seen = 0
    games_saved = 0

    reject_stats: Counter[str] = Counter()
    saved_time_controls: Counter[str] = Counter()
    saved_openings: Counter[str] = Counter()

    accepted_preview_printed = 0
    rejected_preview_printed = 0

    with open(source, "rb") as fh:
        dctx = ZstdDecompressor(max_window_size=2**31)
        reader = dctx.stream_reader(fh, os.stat(source).st_size)
        decoder = codecs.getincrementaldecoder("utf-8")()

        with open(output, "w", encoding="utf-8") as out:
            tail = ""

            while True:
                chunk = reader.read(chunk_size)
                if not chunk:
                    break

                chunks_total += 1
                print(f"=== chunk {chunks_total} ===")

                games, tail = parse_chunk(chunk, tail, decoder)

                for pgn in games:
                    games_seen += 1
                    record = build_record(pgn)

                    reason = check_record_reason(
                        record=record,
                        min_elo=min_elo,
                        max_elo=max_elo,
                        min_plies=min_plies,
                        required_event=required_event,
                        allowed_time_controls=allowed_time_controls,
                        require_clock_data=require_clock_data,
                    )

                    if reason is None:
                        out.write(json.dumps(record, ensure_ascii=False) + "\n")
                        games_saved += 1

                        time_control = record.get("time_control") or "UNKNOWN"
                        opening = record.get("opening") or "UNKNOWN"
                        saved_time_controls[time_control] += 1
                        saved_openings[opening] += 1

                        if accepted_preview_printed < print_first_accepted:
                            print("ACCEPTED:")
                            print(json.dumps(record, ensure_ascii=False, indent=2)[:1500])
                            accepted_preview_printed += 1

                        if games_saved % 50 == 0:
                            print(f"saved: {games_saved}")

                        if max_games is not None and games_saved >= max_games:
                            summary = build_summary(
                                games_seen=games_seen,
                                games_saved=games_saved,
                                reject_stats=reject_stats,
                                saved_time_controls=saved_time_controls,
                                saved_openings=saved_openings,
                            )
                            save_summary(summary_path, summary)

                            print("\nReached max_games limit.")
                            print(f"games_seen={games_seen}, games_saved={games_saved}")
                            print("reject stats:", dict(reject_stats))
                            return
                    else:
                        reject_stats[reason] += 1

                        if rejected_preview_printed < print_first_rejected:
                            print(f"REJECTED: {reason}")
                            print(json.dumps(record, ensure_ascii=False, indent=2)[:1500])
                            rejected_preview_printed += 1

            final_text = decoder.decode(b"", final=True)
            if final_text:
                tail += final_text

            tail = tail.strip()
            if tail:
                games_seen += 1
                record = build_record(tail)

                reason = check_record_reason(
                    record=record,
                    min_elo=min_elo,
                    max_elo=max_elo,
                    min_plies=min_plies,
                    required_event=required_event,
                    allowed_time_controls=allowed_time_controls,
                    require_clock_data=require_clock_data,
                )

                if reason is None:
                    out.write(json.dumps(record, ensure_ascii=False) + "\n")
                    games_saved += 1
                    time_control = record.get("time_control") or "UNKNOWN"
                    opening = record.get("opening") or "UNKNOWN"
                    saved_time_controls[time_control] += 1
                    saved_openings[opening] += 1
                else:
                    reject_stats[reason] += 1

    summary = build_summary(
        games_seen=games_seen,
        games_saved=games_saved,
        reject_stats=reject_stats,
        saved_time_controls=saved_time_controls,
        saved_openings=saved_openings,
    )
    save_summary(summary_path, summary)

    print("\nDecompression finished.")
    print(f"games_seen={games_seen}, games_saved={games_saved}")
    print("reject stats:", dict(reject_stats))


if __name__ == "__main__":
    config_path = Path(__file__).resolve().parent.parent / "config.json"

    with open(config_path, "r", encoding="utf-8") as f:
        config = json.load(f)

    lichess_db_path = config.get("GAME_DATABASE_PATH")
    if not lichess_db_path:
        raise ValueError("GAME_DATABASE_PATH not found in config.json")

    decompress_database_to_jsonl(
        source_path=lichess_db_path,
        output_path="data/raw/games_raw_rapid_1800_2400.jsonl",
        min_elo=1800,
        max_elo=2400,
        min_plies=20,
        required_event="Rated Rapid game",
        allowed_time_controls={"600+0", "600+5"},
        require_clock_data=True,
        max_games=500,
        chunk_size=1_638_400,
        print_first_accepted=3,
        print_first_rejected=3,
        summary_path="data/raw/games_raw_rapid_1800_2400_summary.json",
    )