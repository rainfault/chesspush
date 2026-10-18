# ChessPush alpha.7 — финальные общие проверки

Snapshot: финальные production-файлы alpha.7, включая TEMP guard синхронизации по T0/generation и три дополнительных проверки конкурентного reset.

- Rust: **202 unit + 12 integration + 3 actual Stockfish = 217 PASS**.
- Frontend: **38 tests / 11 files PASS**.
- Cargo fmt и Prettier format:check PASS.
- Cargo Clippy all-targets с desktop и без default features, -D warnings: PASS.
- Privacy: 125 file versions PASS.

Сводка: [summary.json](summary.json). Полные логи находятся рядом. Сводка предыдущего snapshot до sync guard сохранена в [summary-before-sync-guard.json](summary-before-sync-guard.json).

Реальный Stockfish запускается из `.qa/stockfish/release/stockfish/stockfish-windows-x86-64-universal.exe` через обязательную переменную `CHESSPUSH_STOCKFISH`. В исходном процессе оркестрации переменная не была установлена; диагностический результат сохранён в [sync-guard-initial-attempt.json](sync-guard-initial-attempt.json) и `stockfish-real.sync-guard.initial.log`. Повторный запуск с явно заданным официальным движком прошёл все три теста. Скрипт будущих прогонов также исправлен.

Системные настройки, пользовательские данные и production-код проверками не изменялись. GUI-сценарии и сборка приложения выполняются родительским агентом отдельно.
