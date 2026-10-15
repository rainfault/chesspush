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
| url | Lichess game URLs and OAuth URLs | MIT / Apache-2.0 |
| reqwest / native-tls | Lichess HTTPS via Windows SChannel | MIT / Apache-2.0 |
| base64 / getrandom / zeroize | PKCE, OS randomness and secret memory cleanup | MIT / Apache-2.0 |
| windows-sys | Windows credential store and system browser | MIT / Apache-2.0 |

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

The Home Rabbit and Fire icons use the unmodified Lichess icon font restored
from the previous application, pinned to [lila fa6614fc](https://github.com/lichess-org/lila/tree/fa6614fcd5bc8afdad24bf341dbc6686afa4208d/public/font).
The font, editable SFD source and glyph map are in `public/lichess/`.
Its authors and mixed OFL/MIT/CC BY 3.0/AGPLv3+ attribution are retained in
`public/lichess/licenses/LILA-COPYING.md`, alongside the AGPL license text.

Statistics legal replay uses [shakmaty 0.30.0](https://github.com/niklasf/shakmaty)
and [pgn-reader 0.29.1](https://github.com/niklasf/rust-pgn-reader), both
GPL-3.0-or-later; GPL text is retained in `licenses/GPL-3.0.txt`.

`src-tauri/src/statistics/chess.rs` adapts the board phase Divider from
[scalachess e3f7eb8](https://github.com/lichess-org/scalachess/blob/e3f7eb8f73a268f668d70e594e8f2426d8614881/core/src/main/scala/Divider.scala),
Copyright (c) 2012-2014 Thibault Duplessis, MIT. The complete notice is retained
in `licenses/SCALACHESS-MIT.txt`. The adaptation was made on 2026-10-15.

`src-tauri/src/statistics/metrics.rs` adapts Accuracy aggregation/alignment from
[Lichess lila ad760137](https://github.com/lichess-org/lila/blob/ad760137cf9941cadd93126433695d6933209b0a/modules/analyse/src/main/AccuracyPercent.scala)
(AGPL-3.0-or-later), with WinPercent and initial score from
[scalachess e3f7eb8](https://github.com/lichess-org/scalachess/blob/e3f7eb8f73a268f668d70e594e8f2426d8614881/core/src/main/scala/eval.scala).
The AGPL text is retained in `licenses/LILA-AGPL-3.0.txt`. The Rust adaptation,
including missing-data and analysis-profile handling, was made on 2026-10-15.
The metric method records these revisions; local Stockfish evidence does not
promise identical values to a different Lichess cloud analysis.
