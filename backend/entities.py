from __future__ import annotations

from dataclasses import dataclass, field
import re
from typing import Any


@dataclass
class GameRecord:
    storage_id: int = 0
    game_id: str = ""
    event: str = ""
    site: str = ""
    date: str = ""
    white: str = ""
    black: str = ""
    result: str = "*"
    white_elo: int = 0
    black_elo: int = 0
    eco: str = ""
    opening: str = ""
    time_control: str = ""
    termination: str = ""
    moves: str = ""
    raw_pgn: str = ""
    source: str = ""
    source_kind: str = ""
    tags: dict[str, str] = field(default_factory=dict)

    @property
    def ply_count(self) -> int:
        if not self.moves:
            return 0
        text = re.sub(r"\{[^}]*\}", " ", self.moves)
        text = re.sub(r"\([^)]*\)", " ", text)
        text = re.sub(r";[^\n]*", " ", text)
        text = re.sub(r"\$\d+", " ", text)
        text = re.sub(r"\b\d+\.(?:\.\.)?", " ", text)
        tokens = [t for t in text.replace("\n", " ").split(" ") if t]
        count = 0
        for token in tokens:
            token = token.strip().rstrip("!?+#")
            if not token or token in {"1-0", "0-1", "1/2-1/2", "*"}:
                continue
            count += 1
        return count

    @property
    def move_count(self) -> int:
        return self.ply_count // 2

    @property
    def perf_type(self) -> str:
        return detect_perf_type(self.event, self.time_control)

    def score_for_username(self, username: str) -> float | None:
        if not username:
            return None
        name = username.lower()
        if self.white.lower() == name:
            return result_score(self.result, "white")
        if self.black.lower() == name:
            return result_score(self.result, "black")
        return None

    def color_for_username(self, username: str) -> str:
        name = username.lower()
        if self.white.lower() == name:
            return "white"
        if self.black.lower() == name:
            return "black"
        return "unknown"

    def to_dict(self) -> dict[str, Any]:
        return {
            "storage_id": self.storage_id,
            "game_id": self.game_id,
            "event": self.event,
            "site": self.site,
            "date": self.date,
            "white": self.white,
            "black": self.black,
            "result": self.result,
            "white_elo": self.white_elo,
            "black_elo": self.black_elo,
            "eco": self.eco,
            "opening": self.opening,
            "time_control": self.time_control,
            "perf_type": self.perf_type,
            "termination": self.termination,
            "moves": self.moves,
            "move_count": self.move_count,
            "source": self.source,
            "source_kind": self.source_kind,
        }


def result_score(result: str, color: str) -> float:
    if result == "1/2-1/2":
        return 0.5
    if result == "1-0":
        return 1.0 if color == "white" else 0.0
    if result == "0-1":
        return 1.0 if color == "black" else 0.0
    return 0.0


def detect_perf_type(event: str, time_control: str) -> str:
    event = event.lower()
    if "ultrabullet" in event:
        return "ultraBullet"
    if "bullet" in event:
        return "bullet"
    if "blitz" in event:
        return "blitz"
    if "rapid" in event:
        return "rapid"
    if "classical" in event:
        return "classical"
    seconds = 0
    inc = 0
    if "+" in time_control:
        try:
            a, b = time_control.split("+", 1)
            seconds = int(a)
            inc = int(b)
        except ValueError:
            pass
    if seconds and seconds < 180:
        return "bullet"
    if seconds and seconds < 480:
        return "blitz"
    if seconds and seconds < 1500:
        return "rapid"
    if seconds or inc:
        return "classical"
    return "unknown"
