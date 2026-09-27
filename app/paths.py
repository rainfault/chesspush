from __future__ import annotations

from pathlib import Path
from urllib.parse import unquote, urlparse


def normalize_path(value: str | None) -> str:
    if not value:
        return ""
    value = str(value).strip()
    if value.startswith("file://"):
        parsed = urlparse(value)
        path = unquote(parsed.path)
        # Windows file URLs may look like /C:/path/file.pgn
        if len(path) >= 3 and path[0] == "/" and path[2] == ":":
            path = path[1:]
        return path
    return value


def ensure_folder(path: str | Path) -> Path:
    folder = Path(path)
    folder.mkdir(parents=True, exist_ok=True)
    return folder
