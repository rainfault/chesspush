"""Run with the reference project's .venv Python; never reads personal data."""
from pathlib import Path
import json
import sys
import zstandard

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from backend.filters.game_filters import GameFilter
from backend.search.model_game_search import iter_matching_games_from_path
from backend.parsers.zst_reader import iter_zst_pgn_texts
from backend.parsers.pgn_parser import parse_pgn_game

DEST = Path(__file__).parent / "fixtures"
DEST.mkdir(exist_ok=True)
base = dict(Event="Rated Rapid game", Site="https://lichess.org/AbCd0001", UTCDate="2026.09.28", White="Alice", Black="Bob", Result="1-0", WhiteElo="2400", BlackElo="2350", ECO="B12", Opening="Caro-Kann Defense", TimeControl="600+5", Termination="Normal")
variants = [
    {}, {"Result": "0-1", "ECO": "C20", "Opening": "King's Pawn"},
    {"Event": "Rated Blitz game", "Result": "1/2-1/2", "TimeControl": "180+2"},
    {"Event": "Rated Bullet game", "TimeControl": "60+0"},
    {"Termination": "Time forfeit"}, {"WhiteElo": "1800", "BlackElo": "1850"},
    {"WhiteElo": "?", "Site": "Chess.com", "Opening": "Славянская защита"},
    {"Event": "Friendly", "TimeControl": "180+20", "Result": "*"},
    {"Event": "Friendly", "TimeControl": "0+10", "ECO": "D10", "Result": "0-1"},
    {"Event": "UltraBullet", "TimeControl": "15+0"},
    {"Site": "https://lichess.org/study/ABCDEFGH", "LichessURL": "https://lichess.org/ZyXw9876/black"},
    {"Event": "Friendly", "TimeControl": "-", "White": "", "Black": "", "Result": "*"},
]
texts = []
for i, changes in enumerate(variants):
    tags = base | {"Site": f"https://lichess.org/AbCd{i:04}"} | changes
    moves = "1. e4 {note} c6 2. d4 d5 3. Nc3 dxe4 " + tags["Result"]
    if i == 1: moves = "1.e4 e5 2.Nf3 $1 Nc6 (2... d6) 3.Bb5!? a6 0-1"
    if i == 8: moves = " ".join(f"{n}. Nf3 Nf6" for n in range(1, 27)) + " 0-1"
    if i == 11: moves = "1. NOT_A_MOVE ??? *"
    texts.append("\n".join(f'[{k} "{v}"]' for k, v in tags.items()) + "\n\n" + moves)
raw = "\n\n".join(texts) + "\n\n"
(DEST / "corpus.pgn").write_bytes(raw.encode())
compress = zstandard.ZstdCompressor().compress
(DEST / "corpus.pgn.zst").write_bytes(compress(raw.encode()))
(DEST / "crlf.pgn.zst").write_bytes(compress(raw.replace("\n", "\r\n").encode()))
(DEST / "truncated.pgn.zst").write_bytes(compress(raw.encode())[:-8])
(DEST / "invalid-utf8.pgn.zst").write_bytes(compress(b'[Event "Bad"]\n\n1. e4 \xff *'))
scenarios = [
    ("defaults", {}), ("all", {"exclude_bullet": False, "include_timeout": True}),
    ("rapid_rating", {"perf_type": "rapid", "min_white_elo": 2300, "min_black_elo": 2300}),
    ("white_wins", {"color": "white", "result_mode": "selected_wins"}),
    ("black_losses", {"color": "black", "result_mode": "selected_losses"}),
    ("wins_draws", {"color": "white", "result_mode": "wins_draws"}),
    ("unknown_side", {"result_mode": "selected_wins", "username": "Nobody"}),
    ("username", {"result_mode": "selected_losses", "username": "ALICE"}),
    ("no_quick_losses", {"color": "white", "result_mode": "no_quick_losses"}),
    ("eco_range", {"eco": "B10-B19; D"}), ("opening", {"opening_contains": "caro-kann"}),
    ("prefix", {"move_prefix": "1. e4 c6 2. d4"}),
    ("annotated_prefix", {"move_prefix": "1.e4 e5 2.Nf3 Nc6 3.Bb5+"}),
    ("min_moves", {"min_moves": 25}),
    ("rating_difference", {"min_both_elo": 1800, "max_rating_diff": 20}),
    ("bullet", {"perf_type": "bullet", "exclude_bullet": False}),
    ("time_fallback", {"perf_type": "classical"}),
    ("skip", {}, 4, 3), ("match_limit", {}, 0, None, 2),
    ("cancel", {}, 0, None, 100, 3),
    ("past_eof", {}, 100), ("zero_scan", {}, 0, 0),
]
cases = []
for case in scenarios:
    name, overrides, *opts = case
    start, maximum, limit, cancel_at = (opts + [0, None, 100, None][len(opts):])
    state = {"scanned": 0, "stop": False}
    def progress(n):
        state["scanned"] = n
        if cancel_at and n >= cancel_at: state["stop"] = True
    found = []
    for index, game in iter_matching_games_from_path(DEST / "corpus.pgn.zst", GameFilter(**overrides), start_index=start, max_games=maximum, progress_step=1, progress_callback=progress, should_stop=lambda: state["stop"]):
        state["scanned"] = index
        meta = game.to_dict()
        for key in ("storage_id", "source", "source_kind"): meta.pop(key)
        found.append({"index": index, "metadata": meta, "tags": game.tags, "raw_pgn": game.raw_pgn})
        if len(found) >= limit or state["stop"]: break
    cases.append(dict(name=name, filters=overrides, start_index=start, scan_limit=maximum, limit=limit, cancel_at=cancel_at, scanned=state["scanned"], matches=found))
edge = {}
for file in ("crlf", "truncated", "invalid-utf8"):
    try:
        records = list(iter_zst_pgn_texts(DEST / f"{file}.pgn.zst", chunk_size=17))
        edge[file] = {"records": len(records), "ids": [parse_pgn_game(x).game_id for x in records]}
    except Exception as exc:
        edge[file] = {"error": type(exc).__name__}
assert len(list(iter_zst_pgn_texts(DEST / "corpus.pgn.zst", chunk_size=17))) == len(texts)
(DEST / "reference.json").write_text(json.dumps({"reference_commit": "74e01035baa8a02df11ceecd9d9b9253504b8165", "cases": cases, "legacy_edges": edge}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print(f"Captured {len(cases)} scenarios; edge behavior: {edge}")
