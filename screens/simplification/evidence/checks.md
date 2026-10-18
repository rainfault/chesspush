# Final checks: ChessPush alpha.6 UI simplification

- Rust: 184 unit + 12 integration + 3 real Stockfish checks = 199 passed.
- Frontend: 37 tests, 11 files passed.
- Statistics GUI: 8 checks across light/dark, 4 actual native-backed screenshots.
- Formatting (Rust and frontend), privacy (125 versions), Clippy all-targets with and without desktop passed.
- Data profile: isolated 4-game fixture, 3 white / 1 black; own-game records and repertoire documents were not changed by the Statistics test. Graph UI state changed through ordinary navigation.
- No full statistics_report IPC was requested by the Statistics GUI scenarios.

Machine-readable details: [checks-summary.json](checks-summary.json). GUI: [results.json](../openings/results.json). Initial npm.ps1 child-shell launcher failure and successful npm.cmd retry are recorded separately in launcher-attempts.json. No execution policy or user data was changed.
