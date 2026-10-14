# Dependencies and attribution

Versions are locked in `package-lock.json` and `src-tauri/Cargo.lock`.

| Runtime dependency | Purpose | Upstream license |
|---|---|---|
| React / React DOM 19.3.0 | UI | MIT |
| Tauri 2 / API / dialog plugin | desktop IPC, window, native file dialog | MIT / Apache-2.0 |
| [@lichess-org/chessground 10.4.0](https://github.com/lichess-org/chessground) | board input/rendering | GPL-3.0-or-later |
| [chessops 0.15.1](https://github.com/niklasf/chessops) | legality, FEN, SAN | GPL-3.0-or-later |
| zstd | streaming decompression | MIT; native zstd BSD/GPL dual license |
| rusqlite / bundled SQLite | local persistence | MIT / SQLite public domain |
| serde / serde_json | typed IPC and metadata | MIT / Apache-2.0 |
| regex | reference-compatible tokenization | MIT / Apache-2.0 |
| sha2 | stable PGN identity | MIT / Apache-2.0 |
| url | Lichess game URL detection | MIT / Apache-2.0 |

Chessground's official npm CSS includes the cburnett SVG pieces by
**Colin M. L. Burnett**, licensed GPL-2.0-or-later. Its additional license notice
is preserved in [licenses/GPL-2.0.txt](licenses/GPL-2.0.txt).
Chessground/chessops use GPL-3.0-or-later; the license text is preserved in
[licenses/GPL-3.0.txt](licenses/GPL-3.0.txt). Corresponding source and upstream
notices are available from the linked repositories and installed packages.
The board checker pattern and application styles are authored CSS.

`public/openings.json` preserves 3,641 existing entries originally converted
from the Qt-era TSV catalogue at tag `v0.1.0-alpha.1`. Its upstream is
[lichess-org/chess-openings](https://github.com/lichess-org/chess-openings),
licensed [CC0](https://github.com/lichess-org/chess-openings/blob/master/COPYING.txt).
The JSON is self-contained; the old backend directory is not required.
The ChessPush icon is retained from the previous application.

Build/test tools: TypeScript, Vite, React Vite plugin, Tauri CLI, Vitest,
Prettier, Playwright and Rust tempfile. The synthetic benchmark generator
uses built-in Node.js streams/zlib; it needs no Python dependency.

Keep upstream notices and provide corresponding source under the applicable
copyleft terms when distributing a binary containing Chessground/chessops.
This attribution does not relicense unrelated historical project code.
