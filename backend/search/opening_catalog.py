from __future__ import annotations

from collections import Counter, defaultdict
import csv
from dataclasses import asdict, dataclass
from functools import lru_cache
from pathlib import Path
import re
from typing import Iterable

from backend.parsers.auto_loader import iter_games_from_path


OPENING_DATA_DIR = Path(__file__).resolve().parents[1] / "openings"


@dataclass(frozen=True)
class OpeningEntry:
    eco: str
    name: str
    pgn: str

    @property
    def family(self) -> str:
        return self.name.split(":", 1)[0].strip()

    @property
    def key(self) -> str:
        return slug(self.family)

    def to_choice(self) -> dict:
        return {
            "key": f"{self.eco}:{self.name}:{self.pgn}",
            "label": f"{self.eco} · {self.name}",
            "eco": self.eco,
            "opening_contains": self.family,
            "move_prefix": self.pgn,
            "name": self.name,
            "pgn": self.pgn,
            "source": "eco_table",
            "games": 0,
        }


@dataclass(frozen=True)
class OpeningChoice:
    key: str
    label: str
    eco: str = ""
    opening_contains: str = ""
    move_prefix: str = ""
    source: str = "database"
    games: int = 0

    def to_dict(self) -> dict:
        return asdict(self)


def slug(value: str) -> str:
    value = re.sub(r"[^a-z0-9]+", "_", value.lower())
    return value.strip("_") or "opening"


def normalize_query(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", " ", value.lower()).strip()


def normalize_line(value: str) -> str:
    value = re.sub(r"\{[^}]*\}", " ", value)
    value = re.sub(r"\([^)]*\)", " ", value)
    value = re.sub(r"\s+", " ", value)
    return value.strip().lower()


class OpeningBook:
    def __init__(self, entries: Iterable[OpeningEntry]):
        self.entries = sorted(entries, key=lambda item: (item.family, item.eco, item.name, item.pgn))
        self.by_family: dict[str, list[OpeningEntry]] = defaultdict(list)
        for entry in self.entries:
            self.by_family[entry.key].append(entry)

        self.families = []
        for key, items in self.by_family.items():
            eco_codes = sorted({item.eco for item in items})
            family = items[0].family
            self.families.append(
                {
                    "key": key,
                    "label": family,
                    "family": family,
                    "eco": eco_range_label(eco_codes),
                    "line_count": len(items),
                    "detail": f"{eco_range_label(eco_codes)} · {len(items)} lines",
                }
            )
        self.families.sort(key=lambda item: item["label"].lower())

    def search_families(self, query: str = "", limit: int = 80) -> list[dict]:
        query = normalize_query(query)
        if not query:
            return self.families[:limit]

        terms = query.split()
        scored = []
        for family in self.families:
            haystack = normalize_query(" ".join([family["family"], family["eco"], family["detail"]]))
            if all(term in haystack for term in terms):
                score = 0
                if haystack.startswith(query):
                    score += 100
                score += max(0, 40 - haystack.find(terms[0]))
                score += min(30, int(family["line_count"]))
                scored.append((score, family))

        scored.sort(key=lambda item: (-item[0], item[1]["label"].lower()))
        return [dict(family) for _, family in scored[:limit]]

    def variants_for_family(self, family_key: str, eco: str = "", query: str = "", limit: int = 200) -> list[dict]:
        entries = self.by_family.get(family_key, [])
        eco = eco.strip().upper()
        query = normalize_query(query)
        variants = []
        for entry in entries:
            if eco and entry.eco != eco:
                continue
            if query:
                haystack = normalize_query(f"{entry.eco} {entry.name} {entry.pgn}")
                if not all(term in haystack for term in query.split()):
                    continue
            variants.append(entry.to_choice())
        return variants[:limit]

    def ecos_for_family(self, family_key: str) -> list[dict]:
        counts = Counter(entry.eco for entry in self.by_family.get(family_key, []))
        rows = [{"label": "All ECO", "eco": "", "count": sum(counts.values())}]
        rows.extend(
            {"label": f"{eco} ({count})", "eco": eco, "count": count}
            for eco, count in sorted(counts.items())
        )
        return rows

    def match_line(self, pgn_prefix: str, limit: int = 40) -> list[dict]:
        prefix = normalize_line(pgn_prefix)
        if not prefix:
            return []
        exact = []
        continuations = []
        parents = []
        for entry in self.entries:
            line = normalize_line(entry.pgn)
            if line == prefix:
                exact.append(entry)
            elif line.startswith(prefix + " "):
                continuations.append(entry)
            elif prefix.startswith(line + " "):
                parents.append(entry)

        ranked = exact + sorted(continuations, key=lambda item: (len(item.pgn), item.name)) + sorted(
            parents,
            key=lambda item: (-len(item.pgn), item.name),
        )
        return [entry.to_choice() for entry in ranked[:limit]]

    def default_choice(self) -> dict:
        return {
            "key": "any",
            "label": "Any opening",
            "eco": "",
            "opening_contains": "",
            "move_prefix": "",
            "name": "",
            "pgn": "",
            "source": "empty",
            "games": 0,
        }


def eco_range_label(eco_codes: list[str]) -> str:
    if not eco_codes:
        return ""
    if len(eco_codes) == 1:
        return eco_codes[0]
    return f"{eco_codes[0]}-{eco_codes[-1]}"


def read_opening_entries(data_dir: Path = OPENING_DATA_DIR) -> list[OpeningEntry]:
    entries: list[OpeningEntry] = []
    for path in sorted(data_dir.glob("*.tsv")):
        with path.open("r", encoding="utf-8", newline="") as fh:
            for row in csv.DictReader(fh, delimiter="\t"):
                eco = (row.get("eco") or "").strip().upper()
                name = (row.get("name") or "").strip()
                pgn = (row.get("pgn") or "").strip()
                if eco and name and pgn:
                    entries.append(OpeningEntry(eco=eco, name=name, pgn=pgn))
    return entries


@lru_cache(maxsize=1)
def opening_book() -> OpeningBook:
    return OpeningBook(read_opening_entries())


def builtin_opening_choices() -> list[dict]:
    book = opening_book()
    return [book.default_choice()] + [
        {
            "key": family["key"],
            "label": f"{family['family']} · {family['detail']}",
            "eco": family["eco"],
            "opening_contains": family["family"],
            "move_prefix": "",
            "name": family["family"],
            "pgn": "",
            "source": "eco_table",
            "games": 0,
        }
        for family in book.search_families("", limit=200)
    ]


def scan_database_openings(
    path: str | Path,
    *,
    max_games: int = 5000,
    limit: int = 40,
    exclude_bullet: bool = True,
) -> list[dict]:
    counter: Counter[tuple[str, str]] = Counter()
    scanned = 0
    for game in iter_games_from_path(path, max_games=max_games):
        scanned += 1
        if exclude_bullet and game.perf_type in {"bullet", "ultraBullet"}:
            continue
        eco = (game.eco or "").strip().upper()
        opening = (game.opening or "").strip()
        if not eco and not opening:
            continue
        counter[(eco, opening)] += 1

    choices: list[OpeningChoice] = []
    for index, ((eco, opening), games) in enumerate(counter.most_common(limit), start=1):
        title = " · ".join(part for part in (eco, opening) if part)
        family = opening.split(":", 1)[0].strip()
        choices.append(
            OpeningChoice(
                key=f"db_{index}",
                label=f"{title} ({games})",
                eco=eco,
                opening_contains=family or opening,
                source=f"database:{scanned}",
                games=games,
            )
        )
    return [choice.to_dict() for choice in choices]
