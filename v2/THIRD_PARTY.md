# Dependencies and attribution

Versions are pinned in `package-lock.json` and `src-tauri/Cargo.lock`.

| Runtime dependency | Purpose | Upstream license |
|---|---|---|
| React / React DOM 19.3.0 | UI | MIT |
| Tauri 2.12 / API / dialog plugin | desktop IPC, window, native file dialog | MIT / Apache-2.0 |
| [@lichess-org/chessground 10.4.0](https://github.com/lichess-org/chessground) | board interaction/rendering | GPL-3.0-or-later |
| [chessops 0.15.1](https://github.com/niklasf/chessops) | chess legality, FEN, SAN | GPL-3.0-or-later |
| zstd | streaming decompression | MIT (zstd native BSD/GPL dual license) |
| rusqlite / bundled SQLite | local persistence | MIT / SQLite public domain |
| serde / serde_json | typed IPC and metadata | MIT / Apache-2.0 |
| regex | reference-compatible move tokenization | MIT / Apache-2.0 |
| sha2 | stable PGN identity | MIT / Apache-2.0 |
| url | direct Lichess game URL detection | MIT / Apache-2.0 |

Chessground's npm CSS includes the cburnett SVG pieces (Colin M. L. Burnett).
They are used through the explicitly requested official dependency, not downloaded
from Lila. Its GPL notice and source are available in the installed package/upstream.
The existing reference project preserves the additional cburnett GPL-2.0-or-later
attribution in `../ui/web/THIRD_PARTY.md` and `../ui/web/vendor/GPL-2.0.txt`.
No Lila application source, brown PNG, Lichess icon font or website font was copied
into v2. The board checker pattern and app styles are authored CSS.

`public/openings.json` is a conversion of the **existing** 3,641 TSV rows in
`../backend/openings/`; upstream [lichess-org/chess-openings](https://github.com/lichess-org/chess-openings)
uses [CC0](https://github.com/lichess-org/chess-openings/blob/master/COPYING.txt).
The ChessPush application icon is reused from the existing application.

Build/test-only tools: TypeScript, Vite, React Vite plugin, Tauri CLI, Vitest,
Prettier, Playwright and Rust tempfile. No UI kit or frontend state framework.
Keep upstream notices and provide corresponding source under the applicable
copyleft terms when distributing a binary containing Chessground/chessops.
This notice does not relicense unrelated legacy project code.
