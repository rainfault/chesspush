from __future__ import annotations

from contextlib import closing
from datetime import datetime, timezone
import hashlib
import json
import re
import sqlite3
import urllib.parse
from pathlib import Path
from typing import Iterable

from backend.entities import GameRecord
from backend.parsers.pgn_parser import parse_tags


class SQLiteStorage:
    def __init__(self, path: str | Path):
        self.path = Path(path)

    def connect(self) -> sqlite3.Connection:
        conn = sqlite3.connect(self.path)
        conn.row_factory = sqlite3.Row
        conn.create_function("merge_json_objects", 2, self._merge_json_objects)
        conn.create_function("source_kind_priority", 1, self._source_kind_priority)
        conn.execute("PRAGMA foreign_keys = ON")
        conn.execute("PRAGMA busy_timeout = 5000")
        return conn

    def initialize(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with closing(self.connect()) as conn, conn:
            conn.executescript(
                """
                CREATE TABLE IF NOT EXISTS games (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    game_id TEXT,
                    event TEXT,
                    site TEXT,
                    date TEXT,
                    white TEXT,
                    black TEXT,
                    result TEXT,
                    white_elo INTEGER,
                    black_elo INTEGER,
                    eco TEXT,
                    opening TEXT,
                    time_control TEXT,
                    perf_type TEXT,
                    termination TEXT,
                    moves TEXT,
                    raw_pgn TEXT,
                    source TEXT,
                    user_tag TEXT DEFAULT '',
                    note TEXT DEFAULT '',
                    UNIQUE(game_id, site)
                );

                CREATE TABLE IF NOT EXISTS training_cards (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    title TEXT NOT NULL,
                    fen TEXT DEFAULT '',
                    theme TEXT DEFAULT '',
                    problem TEXT DEFAULT '',
                    plan TEXT DEFAULT '',
                    typical_break TEXT DEFAULT '',
                    avoid TEXT DEFAULT '',
                    model_games TEXT DEFAULT '',
                    status TEXT DEFAULT 'new',
                    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                    updated_at TEXT DEFAULT CURRENT_TIMESTAMP
                );

                CREATE TABLE IF NOT EXISTS position_lab_games (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    fingerprint TEXT NOT NULL UNIQUE,
                    game_id TEXT DEFAULT '',
                    event TEXT DEFAULT '',
                    site TEXT DEFAULT '',
                    date TEXT DEFAULT '',
                    white TEXT DEFAULT '',
                    black TEXT DEFAULT '',
                    result TEXT DEFAULT '*',
                    white_elo INTEGER DEFAULT 0,
                    black_elo INTEGER DEFAULT 0,
                    eco TEXT DEFAULT '',
                    opening TEXT DEFAULT '',
                    time_control TEXT DEFAULT '',
                    perf_type TEXT DEFAULT '',
                    termination TEXT DEFAULT '',
                    moves TEXT DEFAULT '',
                    raw_pgn TEXT DEFAULT '',
                    source TEXT DEFAULT '',
                    source_kind TEXT DEFAULT 'external_pgn',
                    tags_json TEXT DEFAULT '{}',
                    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                    updated_at TEXT DEFAULT CURRENT_TIMESTAMP
                );

                CREATE INDEX IF NOT EXISTS idx_position_lab_games_date
                    ON position_lab_games(date DESC, id DESC);
                CREATE INDEX IF NOT EXISTS idx_position_lab_games_eco
                    ON position_lab_games(eco);
                CREATE INDEX IF NOT EXISTS idx_position_lab_games_source
                    ON position_lab_games(source_kind);

                CREATE TABLE IF NOT EXISTS saved_positions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    game_ref INTEGER,
                    fen TEXT NOT NULL,
                    ply INTEGER NOT NULL DEFAULT 0,
                    move_label TEXT DEFAULT '',
                    side_to_move TEXT DEFAULT 'white',
                    white TEXT DEFAULT '',
                    black TEXT DEFAULT '',
                    game_date TEXT DEFAULT '',
                    eco TEXT DEFAULT '',
                    opening TEXT DEFAULT '',
                    color TEXT DEFAULT 'unknown',
                    source TEXT DEFAULT '',
                    source_kind TEXT DEFAULT '',
                    tags_json TEXT DEFAULT '[]',
                    comment TEXT DEFAULT '',
                    arrows_json TEXT DEFAULT '[]',
                    highlights_json TEXT DEFAULT '[]',
                    last_rating TEXT DEFAULT '',
                    review_count INTEGER DEFAULT 0,
                    last_reviewed_at TEXT DEFAULT '',
                    created_at TEXT DEFAULT CURRENT_TIMESTAMP,
                    updated_at TEXT DEFAULT CURRENT_TIMESTAMP,
                    FOREIGN KEY(game_ref) REFERENCES position_lab_games(id) ON DELETE SET NULL
                );

                CREATE INDEX IF NOT EXISTS idx_saved_positions_eco
                    ON saved_positions(eco);
                CREATE INDEX IF NOT EXISTS idx_saved_positions_source
                    ON saved_positions(source_kind);
                CREATE INDEX IF NOT EXISTS idx_saved_positions_date
                    ON saved_positions(game_date DESC, id DESC);

                CREATE TABLE IF NOT EXISTS position_reviews (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    position_id INTEGER NOT NULL,
                    rating TEXT NOT NULL,
                    reviewed_at TEXT DEFAULT CURRENT_TIMESTAMP,
                    FOREIGN KEY(position_id) REFERENCES saved_positions(id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS storage_meta (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL DEFAULT ''
                );
                """
            )
        self._migrate_legacy_games()

    def upsert_games(self, games: Iterable[GameRecord]) -> int:
        """Compatibility entry point; new game storage lives in Position Lab."""
        return self.upsert_lab_games(games, source_kind="legacy")

    def upsert_lab_games(
        self,
        games: Iterable[GameRecord],
        *,
        source_kind: str,
    ) -> int:
        game_list = list(games)
        if not game_list:
            return 0

        normalized_kind = (source_kind or "external_pgn").strip() or "external_pgn"
        rows = []
        fingerprints = []
        for game in game_list:
            fingerprint = self.game_fingerprint(game)
            fingerprints.append(fingerprint)
            rows.append(
                (
                    fingerprint,
                    game.game_id,
                    game.event,
                    game.site,
                    game.date,
                    game.white,
                    game.black,
                    game.result,
                    game.white_elo,
                    game.black_elo,
                    game.eco,
                    game.opening,
                    game.time_control,
                    game.perf_type,
                    game.termination,
                    game.moves,
                    game.raw_pgn,
                    game.source,
                    normalized_kind,
                    json.dumps(game.tags or {}, ensure_ascii=False, sort_keys=True),
                )
            )

        with closing(self.connect()) as conn, conn:
            before = int(
                conn.execute("SELECT COUNT(*) AS c FROM position_lab_games").fetchone()["c"]
            )
            conn.executemany(
                """
                INSERT INTO position_lab_games (
                    fingerprint, game_id, event, site, date, white, black, result,
                    white_elo, black_elo, eco, opening, time_control, perf_type,
                    termination, moves, raw_pgn, source, source_kind, tags_json
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(fingerprint) DO UPDATE SET
                    game_id = CASE WHEN trim(excluded.game_id) NOT IN ('', '?', '-')
                        THEN excluded.game_id ELSE position_lab_games.game_id END,
                    event = CASE WHEN trim(excluded.event) NOT IN ('', '?', '-')
                        THEN excluded.event ELSE position_lab_games.event END,
                    site = CASE WHEN trim(excluded.site) NOT IN ('', '?', '-')
                        THEN excluded.site ELSE position_lab_games.site END,
                    date = CASE WHEN trim(excluded.date) NOT IN ('', '?', '-')
                        THEN excluded.date ELSE position_lab_games.date END,
                    white = CASE WHEN trim(excluded.white) NOT IN ('', '?', '-')
                        THEN excluded.white ELSE position_lab_games.white END,
                    black = CASE WHEN trim(excluded.black) NOT IN ('', '?', '-')
                        THEN excluded.black ELSE position_lab_games.black END,
                    result = CASE WHEN excluded.result IN ('1-0', '0-1', '1/2-1/2')
                        THEN excluded.result ELSE position_lab_games.result END,
                    white_elo = CASE WHEN excluded.white_elo > 0
                        THEN excluded.white_elo ELSE position_lab_games.white_elo END,
                    black_elo = CASE WHEN excluded.black_elo > 0
                        THEN excluded.black_elo ELSE position_lab_games.black_elo END,
                    eco = CASE WHEN trim(excluded.eco) NOT IN ('', '?', '-')
                        THEN excluded.eco ELSE position_lab_games.eco END,
                    opening = CASE WHEN trim(excluded.opening) NOT IN ('', '?', '-')
                        THEN excluded.opening ELSE position_lab_games.opening END,
                    time_control = CASE WHEN trim(excluded.time_control) NOT IN ('', '?', '-')
                        THEN excluded.time_control ELSE position_lab_games.time_control END,
                    perf_type = CASE WHEN trim(excluded.perf_type) NOT IN ('', '?', '-')
                        THEN excluded.perf_type ELSE position_lab_games.perf_type END,
                    termination = CASE WHEN trim(excluded.termination) NOT IN ('', '?', '-')
                        THEN excluded.termination ELSE position_lab_games.termination END,
                    moves = CASE
                        WHEN length(excluded.moves) >= length(position_lab_games.moves)
                        THEN excluded.moves ELSE position_lab_games.moves END,
                    raw_pgn = CASE
                        WHEN length(excluded.raw_pgn) >= length(position_lab_games.raw_pgn)
                            AND length(excluded.moves) >= length(position_lab_games.moves)
                        THEN excluded.raw_pgn ELSE position_lab_games.raw_pgn END,
                    source = CASE
                        WHEN trim(excluded.source) = '' THEN position_lab_games.source
                        WHEN source_kind_priority(excluded.source_kind)
                            >= source_kind_priority(position_lab_games.source_kind)
                        THEN excluded.source ELSE position_lab_games.source END,
                    source_kind = CASE
                        WHEN source_kind_priority(excluded.source_kind)
                            >= source_kind_priority(position_lab_games.source_kind)
                        THEN excluded.source_kind ELSE position_lab_games.source_kind END,
                    tags_json = merge_json_objects(
                        position_lab_games.tags_json, excluded.tags_json
                    ),
                    updated_at = CURRENT_TIMESTAMP
                """,
                rows,
            )
            after = int(
                conn.execute("SELECT COUNT(*) AS c FROM position_lab_games").fetchone()["c"]
            )
            id_rows = []
            for offset in range(0, len(fingerprints), 500):
                batch = fingerprints[offset:offset + 500]
                placeholders = ",".join("?" for _ in batch)
                id_rows.extend(
                    conn.execute(
                        f"SELECT id, fingerprint, source_kind FROM position_lab_games WHERE fingerprint IN ({placeholders})",
                        batch,
                    ).fetchall()
                )

        stored = {
            str(row["fingerprint"]): (int(row["id"]), str(row["source_kind"]))
            for row in id_rows
        }
        for game, fingerprint in zip(game_list, fingerprints):
            game.storage_id = stored.get(fingerprint, (0, normalized_kind))[0]
            # Keep the provenance of the active session on the in-memory object.
            # The canonical database row may prefer the user's Lichess copy, but
            # a model game currently opened from ZST must still be labelled ZST.
            game.source_kind = normalized_kind
        return after - before

    def list_games(self, limit: int | None = 500) -> list[GameRecord]:
        return self.list_lab_games(limit=limit)

    def list_lab_games(
        self,
        *,
        limit: int | None = 500,
        source_kinds: Iterable[str] | None = None,
        eco: str = "",
        opening_contains: str = "",
        username: str = "",
        color: str = "any",
    ) -> list[GameRecord]:
        clauses = []
        params: list[object] = []
        kinds = [str(value).strip() for value in (source_kinds or []) if str(value).strip()]
        if kinds:
            clauses.append(f"source_kind IN ({','.join('?' for _ in kinds)})")
            params.extend(kinds)
        if eco.strip():
            clauses.append("UPPER(eco) = ?")
            params.append(eco.strip().upper())
        if opening_contains.strip():
            clauses.append("LOWER(opening) LIKE ?")
            params.append(f"%{opening_contains.strip().lower()}%")

        sql = "SELECT * FROM position_lab_games"
        if clauses:
            sql += " WHERE " + " AND ".join(clauses)
        sql += " ORDER BY date DESC, id DESC"
        if limit is not None and limit > 0:
            sql += " LIMIT ?"
            params.append(int(limit))

        with closing(self.connect()) as conn, conn:
            rows = conn.execute(sql, params).fetchall()
        games = [self._row_to_game(row) for row in rows]

        normalized_username = username.strip().casefold()
        normalized_color = color.strip().lower()
        if normalized_username and normalized_color in {"white", "black"}:
            games = [
                game
                for game in games
                if (
                    game.white.strip().casefold() == normalized_username
                    if normalized_color == "white"
                    else game.black.strip().casefold() == normalized_username
                )
            ]
        return games

    def get_lab_game(self, game_id: int) -> GameRecord | None:
        with closing(self.connect()) as conn, conn:
            row = conn.execute(
                "SELECT * FROM position_lab_games WHERE id = ?",
                (int(game_id),),
            ).fetchone()
        return self._row_to_game(row) if row is not None else None

    def count_games(self) -> int:
        return self.count_lab_games()

    def count_lab_games(self, source_kinds: Iterable[str] | None = None) -> int:
        kinds = [str(value).strip() for value in (source_kinds or []) if str(value).strip()]
        with closing(self.connect()) as conn, conn:
            if kinds:
                row = conn.execute(
                    f"SELECT COUNT(*) AS c FROM position_lab_games WHERE source_kind IN ({','.join('?' for _ in kinds)})",
                    kinds,
                ).fetchone()
            else:
                row = conn.execute("SELECT COUNT(*) AS c FROM position_lab_games").fetchone()
        return int(row["c"])

    def clear_games(self) -> int:
        with closing(self.connect()) as conn, conn:
            before = int(
                conn.execute("SELECT COUNT(*) AS c FROM position_lab_games").fetchone()["c"]
            )
            conn.execute("DELETE FROM position_lab_games")
            return before

    def _row_to_game(self, r: sqlite3.Row) -> GameRecord:
        keys = set(r.keys())

        def value(name: str, default=""):
            return r[name] if name in keys and r[name] is not None else default

        tags = {}
        try:
            tags = json.loads(value("tags_json", "{}"))
        except (TypeError, ValueError, json.JSONDecodeError):
            tags = {}
        raw_pgn = str(value("raw_pgn"))
        if not tags and raw_pgn:
            tags = parse_tags(raw_pgn)
        return GameRecord(
            storage_id=int(value("id", 0) or 0),
            game_id=str(value("game_id")),
            event=str(value("event")),
            site=str(value("site")),
            date=str(value("date")),
            white=str(value("white")),
            black=str(value("black")),
            result=str(value("result", "*")),
            white_elo=int(value("white_elo", 0) or 0),
            black_elo=int(value("black_elo", 0) or 0),
            eco=str(value("eco")),
            opening=str(value("opening")),
            time_control=str(value("time_control")),
            termination=str(value("termination")),
            moves=str(value("moves")),
            raw_pgn=raw_pgn,
            source=str(value("source")),
            source_kind=str(value("source_kind")),
            tags=tags if isinstance(tags, dict) else {},
        )

    @staticmethod
    def game_fingerprint(game: GameRecord) -> str:
        stable_id = (game.game_id or "").strip()
        site = (game.site or "").strip()
        parsed_site = urllib.parse.urlparse(site)
        hostname = (parsed_site.hostname or "").casefold()
        lichess_host = hostname == "lichess.org" or hostname.endswith(".lichess.org")
        direct_lichess_id = ""
        for candidate_url in (site, str(game.tags.get("LichessURL", ""))):
            parsed_candidate = urllib.parse.urlparse(candidate_url.strip())
            candidate_host = (parsed_candidate.hostname or "").casefold()
            candidate_parts = [
                part for part in parsed_candidate.path.split("/") if part
            ]
            if (
                (candidate_host == "lichess.org" or candidate_host.endswith(".lichess.org"))
                and candidate_parts
                and candidate_parts[0].casefold() == stable_id.casefold()
                and re.fullmatch(r"[A-Za-z0-9]{8,12}", stable_id)
            ):
                direct_lichess_id = stable_id
                break
        inferred_venue_id = bool(site) and stable_id.casefold() == site.casefold()
        if direct_lichess_id:
            identity = f"lichess|{stable_id.casefold()}"
        elif (
            stable_id
            and stable_id not in {"?", "-"}
            and not inferred_venue_id
            and not lichess_host
        ):
            if site and site not in {"?", "-"}:
                identity = f"id|{stable_id.casefold()}|{site.casefold()}"
            else:
                identity = f"id|{stable_id.casefold()}"
        else:
            payload = {
                "site": site.casefold(),
                "event": game.event.strip().casefold(),
                "round": str(game.tags.get("Round", "")).strip(),
                "date": game.date.strip(),
                "utc_date": str(game.tags.get("UTCDate", "")).strip(),
                "utc_time": str(game.tags.get("UTCTime", "")).strip(),
                "white": game.white.strip().casefold(),
                "black": game.black.strip().casefold(),
                "result": game.result.strip(),
                "variant": str(game.tags.get("Variant", "")).strip().casefold(),
                "initial_fen": str(game.tags.get("FEN", "")).strip(),
                "moves": " ".join(game.moves.split()),
            }
            identity = "pgn|" + json.dumps(payload, ensure_ascii=False, sort_keys=True)
        return hashlib.sha256(identity.encode("utf-8")).hexdigest()

    def _migrate_legacy_games(self) -> None:
        with closing(self.connect()) as conn, conn:
            if conn.execute(
                "SELECT 1 FROM storage_meta WHERE key = 'legacy_games_migrated_v1'"
            ).fetchone():
                return
            rows = conn.execute("SELECT * FROM games ORDER BY id").fetchall()
        if rows:
            self.upsert_lab_games(
                [self._row_to_game(row) for row in rows],
                source_kind="legacy",
            )
        with closing(self.connect()) as conn, conn:
            conn.execute(
                "INSERT OR REPLACE INTO storage_meta(key, value) VALUES (?, ?)",
                ("legacy_games_migrated_v1", datetime.now(timezone.utc).isoformat()),
            )

    def add_saved_position(
        self,
        *,
        game_ref: int | None,
        fen: str,
        ply: int,
        move_label: str,
        side_to_move: str,
        white: str,
        black: str,
        game_date: str,
        eco: str,
        opening: str,
        color: str,
        source: str,
        source_kind: str,
        tags: Iterable[str],
        comment: str,
        arrows: list[dict],
        highlights: list[dict],
    ) -> int:
        with closing(self.connect()) as conn, conn:
            cursor = conn.execute(
                """
                INSERT INTO saved_positions (
                    game_ref, fen, ply, move_label, side_to_move,
                    white, black, game_date, eco, opening, color,
                    source, source_kind, tags_json, comment,
                    arrows_json, highlights_json
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    int(game_ref) if game_ref else None,
                    fen.strip(),
                    max(0, int(ply)),
                    move_label.strip(),
                    side_to_move if side_to_move in {"white", "black"} else "white",
                    white.strip(),
                    black.strip(),
                    game_date.strip(),
                    eco.strip().upper(),
                    opening.strip(),
                    color if color in {"white", "black", "unknown"} else "unknown",
                    source.strip(),
                    source_kind.strip(),
                    self._json_tags(tags),
                    comment.strip(),
                    json.dumps(arrows, ensure_ascii=False),
                    json.dumps(highlights, ensure_ascii=False),
                ),
            )
            return int(cursor.lastrowid)

    def update_saved_position(
        self,
        position_id: int,
        *,
        tags: Iterable[str],
        comment: str,
        arrows: list[dict],
        highlights: list[dict],
    ) -> bool:
        with closing(self.connect()) as conn, conn:
            cursor = conn.execute(
                """
                UPDATE saved_positions
                SET tags_json = ?, comment = ?, arrows_json = ?, highlights_json = ?,
                    updated_at = CURRENT_TIMESTAMP
                WHERE id = ?
                """,
                (
                    self._json_tags(tags),
                    comment.strip(),
                    json.dumps(arrows, ensure_ascii=False),
                    json.dumps(highlights, ensure_ascii=False),
                    int(position_id),
                ),
            )
            return cursor.rowcount > 0

    def delete_saved_position(self, position_id: int) -> bool:
        with closing(self.connect()) as conn, conn:
            conn.execute(
                "DELETE FROM position_reviews WHERE position_id = ?",
                (int(position_id),),
            )
            cursor = conn.execute(
                "DELETE FROM saved_positions WHERE id = ?",
                (int(position_id),),
            )
            return cursor.rowcount > 0

    def get_saved_position(self, position_id: int) -> dict | None:
        with closing(self.connect()) as conn, conn:
            row = conn.execute(
                "SELECT * FROM saved_positions WHERE id = ?",
                (int(position_id),),
            ).fetchone()
        return self._saved_position_row(row) if row is not None else None

    def list_saved_positions(
        self,
        *,
        search: str = "",
        tags: Iterable[str] | None = None,
        eco: str = "",
        opening: str = "",
        color: str = "",
        source: str = "",
        date_from: str = "",
        date_to: str = "",
    ) -> list[dict]:
        with closing(self.connect()) as conn, conn:
            rows = conn.execute(
                "SELECT * FROM saved_positions ORDER BY updated_at DESC, id DESC"
            ).fetchall()
        positions = [self._saved_position_row(row) for row in rows]

        needle = search.strip().casefold()
        required_tags = {tag.casefold() for tag in self._normalize_tags(tags or [])}
        normalized_eco = eco.strip().upper()
        normalized_opening = opening.strip().casefold()
        normalized_color = color.strip().lower()
        normalized_source = source.strip().casefold()
        normalized_from = self._normalized_date(date_from)
        normalized_to = self._normalized_date(date_to)

        filtered = []
        for position in positions:
            position_tags = {str(tag).casefold() for tag in position["tags"]}
            haystack = " ".join(
                str(position.get(field, ""))
                for field in ("white", "black", "eco", "opening", "comment", "source")
            ).casefold()
            game_date = self._normalized_date(str(position.get("game_date", "")))
            if needle and needle not in haystack and not any(needle in tag for tag in position_tags):
                continue
            if required_tags and not required_tags.issubset(position_tags):
                continue
            if normalized_eco and str(position["eco"]).upper() != normalized_eco:
                continue
            if normalized_opening and normalized_opening not in str(position["opening"]).casefold():
                continue
            if normalized_color in {"white", "black", "unknown"} and position["color"] != normalized_color:
                continue
            if normalized_source and (
                normalized_source not in str(position["source_kind"]).casefold()
                and normalized_source not in str(position["source"]).casefold()
            ):
                continue
            if normalized_from and (not game_date or game_date < normalized_from):
                continue
            if normalized_to and (not game_date or game_date > normalized_to):
                continue
            filtered.append(position)
        return filtered

    def count_saved_positions(self) -> int:
        with closing(self.connect()) as conn, conn:
            row = conn.execute("SELECT COUNT(*) AS c FROM saved_positions").fetchone()
        return int(row["c"])

    def record_position_review(self, position_id: int, rating: str) -> int:
        normalized = rating.strip().lower()
        if normalized not in {"forgot", "partial", "remembered"}:
            raise ValueError("Unknown training rating")
        reviewed_at = datetime.now(timezone.utc).replace(microsecond=0).isoformat()
        with closing(self.connect()) as conn, conn:
            exists = conn.execute(
                "SELECT 1 FROM saved_positions WHERE id = ?",
                (int(position_id),),
            ).fetchone()
            if exists is None:
                raise ValueError("Saved position not found")
            cursor = conn.execute(
                "INSERT INTO position_reviews (position_id, rating, reviewed_at) VALUES (?, ?, ?)",
                (int(position_id), normalized, reviewed_at),
            )
            conn.execute(
                """
                UPDATE saved_positions
                SET last_rating = ?, review_count = review_count + 1,
                    last_reviewed_at = ?, updated_at = CURRENT_TIMESTAMP
                WHERE id = ?
                """,
                (normalized, reviewed_at, int(position_id)),
            )
            return int(cursor.lastrowid)

    def list_position_reviews(self, position_id: int) -> list[dict]:
        with closing(self.connect()) as conn, conn:
            rows = conn.execute(
                """
                SELECT id, position_id, rating, reviewed_at
                FROM position_reviews
                WHERE position_id = ?
                ORDER BY reviewed_at DESC, id DESC
                """,
                (int(position_id),),
            ).fetchall()
        return [dict(row) for row in rows]

    def _saved_position_row(self, row: sqlite3.Row) -> dict:
        data = dict(row)
        data["tags"] = self._load_json_list(data.pop("tags_json", "[]"))
        data["arrows"] = self._load_json_list(data.pop("arrows_json", "[]"))
        data["highlights"] = self._load_json_list(data.pop("highlights_json", "[]"))
        data["title"] = (
            f"{data.get('eco') or '???'} · {data.get('white') or '?'} — {data.get('black') or '?'}"
        )
        data["subtitle"] = (
            f"{data.get('move_label') or 'Начальная позиция'} · {data.get('game_date') or 'без даты'}"
        )
        data["tags_text"] = ", ".join(data["tags"])
        return data

    @staticmethod
    def _normalize_tags(tags: Iterable[str]) -> list[str]:
        seen = set()
        result = []
        for value in tags:
            tag = " ".join(str(value).strip().split())
            key = tag.casefold()
            if tag and key not in seen:
                seen.add(key)
                result.append(tag)
        return result

    def _json_tags(self, tags: Iterable[str]) -> str:
        return json.dumps(self._normalize_tags(tags), ensure_ascii=False)

    @staticmethod
    def _load_json_list(value: str) -> list:
        try:
            parsed = json.loads(value or "[]")
            return parsed if isinstance(parsed, list) else []
        except (TypeError, ValueError, json.JSONDecodeError):
            return []

    @staticmethod
    def _merge_json_objects(existing: str, incoming: str) -> str:
        def load(value: str) -> dict:
            try:
                parsed = json.loads(value or "{}")
                return parsed if isinstance(parsed, dict) else {}
            except (TypeError, ValueError, json.JSONDecodeError):
                return {}

        merged = load(existing)
        merged.update(
            {
                str(key): value
                for key, value in load(incoming).items()
                if str(value).strip() not in {"", "?", "-"}
            }
        )
        return json.dumps(merged, ensure_ascii=False, sort_keys=True)

    @staticmethod
    def _source_kind_priority(value: str) -> int:
        normalized = str(value or "").strip().lower()
        if normalized in {"lichess", "lichess_user", "statistics_pgn"}:
            return 3
        if normalized == "legacy":
            return 2
        return 1

    @staticmethod
    def _normalized_date(value: str) -> str:
        normalized = value.strip().replace(".", "-")
        return normalized if re.fullmatch(r"\d{4}-\d{2}-\d{2}", normalized) else ""

    def add_training_card(
        self,
        *,
        title: str,
        fen: str = "",
        theme: str = "",
        problem: str = "",
        plan: str = "",
        typical_break: str = "",
        avoid: str = "",
        model_games: str = "",
        status: str = "new",
    ) -> int:
        with closing(self.connect()) as conn, conn:
            cursor = conn.execute(
                """
                INSERT INTO training_cards (
                    title, fen, theme, problem, plan, typical_break, avoid, model_games, status
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (title, fen, theme, problem, plan, typical_break, avoid, model_games, status),
            )
            return int(cursor.lastrowid)

    def list_training_cards(self) -> list[dict]:
        with closing(self.connect()) as conn, conn:
            rows = conn.execute(
                "SELECT * FROM training_cards ORDER BY updated_at DESC, id DESC"
            ).fetchall()
        return [dict(row) for row in rows]

    def delete_training_card(self, card_id: int) -> None:
        with closing(self.connect()) as conn, conn:
            conn.execute("DELETE FROM training_cards WHERE id = ?", (card_id,))

    def storage_info(self) -> dict:
        return {
            "path": str(self.path),
            "games": self.count_games(),
            "positions": self.count_saved_positions(),
            "cards": len(self.list_training_cards()),
        }
