# Board assets

Chessground **10.4.0**, official `@lichess-org/chessground` package,
Copyright Lichess contributors, GPL-3.0-or-later. The minified ES module is
vendored unmodified. See `vendor/CHESSGROUND-LICENSE` and its source URL
in `vendor/manifest.json`.

Brown board: lichess-org/lila, AGPL-3.0-or-later. Cburnett chess pieces:
Colin M. L. Burnett, GPL-2.0-or-later. Pinned source revision and every
asset URL/hash are in the manifest. Upstream attribution and asset-specific
licenses are preserved in `vendor/LILA-COPYING.md`.

Noto Sans: Noto project, SIL Open Font License 1.1; see `vendor/NOTO-LICENSE`.
The Lichess icon font (Rabbit U+E002 and Fire U+E02F) is bundled with its SFD
source and upstream glyph map. Its authors and mixed OFL/MIT/CC BY 3.0/AGPL
attribution are listed in `vendor/LILA-COPYING.md`, under `public/font/lichess`.
The small board adapter and coordinate styling are based on the upstream
Chessground/Lichess integration conventions. Changes: Qt WebChannel bridge,
Python legal moves, promotion selector, local URLs and keyboard navigation.

All resources load locally. To reproduce vendoring: `python tools/vendor_board.py`.
Keep these notices and applicable copyleft/source obligations when distributing
the application. No license for unrelated existing project code is changed here.
