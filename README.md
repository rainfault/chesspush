# ChessPush · 0.2.0-alpha.2

Персональная шахматная утилита на **Tauri 2, React, TypeScript и Rust**.
Рабочие разделы — ZST/PGN-парсер и шахматная доска. Qt/Python исключён из текущей
версии; прежняя реализация сохранена в Git под tag `v0.1.0-alpha.1`.

## Подготовка и запуск

Windows 10/11, Node.js 24+, Rust stable MSVC, Visual Studio Build Tools с Desktop
development with C++ и Windows SDK, WebView2 Runtime.
[Официальные prerequisites Tauri](https://v2.tauri.app/start/prerequisites/).
Все команды выполняются из корня репозитория.

```powershell
npm ci
npm run tauri dev
```

`npm run dev` запускает frontend в браузере: доска доступна, файловые операции
требуют Tauri. Vite используется при разработке; production HTTP-сервера нет.

```powershell
npm run tauri build
```

Исполняемый файл: `src-tauri/target/release/chesspush.exe`.
Установщик: `src-tauri/target/release/bundle/nsis/`.
Быстрая локальная сборка: `npm run tauri build -- --debug --no-bundle`.
Ярлык рабочего стола обновляется скриптом `tools/create_shortcut.ps1`.

## Парсер

Откройте «Базы», выберите `.pgn.zst`, `.zst` или `.pgn`, задайте фильтры и нажмите
«Найти партии». Дебют выбирается поиском или ходами на доске. Ходы до текущей
позиции становятся префиксом поиска; ECO применяется, если префикса нет.
Произвольный FEN меняет доску, но не включает поиск позиции по корпусу.

«Дополнительно»: отдельные рейтинги сторон, минимальная длина, пропуск партий,
ECO, игрок для определения стороны, Bullet и просрочка времени.
«Остановить» сохраняет завершённую часть выборки. Счётчики абсолютные;
полоса прогресса неопределённая, без вымышленного процента распаковки.

В «Файлах» хранится история источников и результатов. Нажатие на имя показывает
файл в Проводнике; кнопка справа выбирает источник. «Импорт PGN» сохраняет игры
без фильтров. Импорт нормализуется в отдельный PGN; исходник не изменяется.

## Данные

Windows: `%APPDATA%/io.github.rainfault.chesspush.v2/chesspush.sqlite3`.
Identifier сохранён, поэтому уже созданная Tauri-библиотека продолжает использоваться.
Рядом находятся `exports/`. Используется Tauri `app_data_dir`; сырые огромные корпуса
остаются внешними файлами. Схема SQLite версионируется через `PRAGMA user_version`;
SQL migrations находятся в `src-tauri/migrations/`.

Старые локальные `data/` и `outputs/` сохраняются и не публикуются в Git.
Одноразовый перенос старой SQLite-библиотеки выполняется Rust example `migrate_legacy`.
Он читает прежнюю базу, сохраняет игры и историю в Tauri SQLite и создаёт резервную
копию целевой базы перед изменениями. Исходная база, экспорты и файлы настроек остаются
на месте. Ник и токен сохраняются только в локальной SQLite и резервной копии;
они не выводятся в лог и не публикуются в Git.

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --no-default-features --example migrate_legacy -- data/chess_training_tool.sqlite3 data/settings.json "$env:APPDATA/io.github.rainfault.chesspush.v2/chesspush.sqlite3" data/migration-backups
```

## Проверки

```powershell
npm run check
npm run format:check
npm test
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Быстрые Rust-тесты без GUI:
`cargo test --manifest-path src-tauri/Cargo.toml --no-default-features`.
Снимок исходного поведения сохранён в `tests/fixtures/`: 22 сценария на PGN и ZST.
Python и Qt для проверок и сборки текущего приложения не требуются.

При работающем `npm run dev`: `node tests/ui-smoke.mjs` (установленный Edge).
Нативная проверка: `npm run test:native`. Команда собирает debug-приложение, генерирует
тестовый ZST и запускает отдельный процесс с временной SQLite и WebView2-профилем
в `.qa/`. CDP port выбирается автоматически; рабочая библиотека не используется.
Результаты UI сохраняются в `.qa/`. Release игнорирует `CHESSPUSH_TEST_DATA`.

Синтетический performance input создаётся стандартным Node.js ZST stream:

```powershell
node tests/benchmark.mjs
cargo run --manifest-path src-tauri/Cargo.toml --release --no-default-features --example scan -- .qa/benchmark.pgn.zst
```

По умолчанию генерируются 120 000 игр, ожидаются 90 000 совпадений с фильтрами Rust
по умолчанию. `node tests/benchmark.mjs 1000` задаёт другое число повторов fixture.
Измеряется scan/filter; это не benchmark полного экспорта и SQLite-save.

## Каталоги

- `src/` — React, Tauri boundary и CSS tokens; `src/chess/` — BoardSession/Chessground.
- `src-tauri/src/` — parser, filters, PGN, application, storage, desktop commands.
- `src-tauri/migrations/` — SQLite schema; `src-tauri/examples/` — консольные инструменты.
- `tests/`, `src-tauri/tests/` — frozen parity, regression и UI-проверки.
- `public/` — локальный каталог дебютов и иконка; `licenses/` — сохранённые notices.

[Архитектура](ARCHITECTURE.md) · [Миграция](MIGRATION.md) · [Зависимости и лицензии](THIRD_PARTY.md).
