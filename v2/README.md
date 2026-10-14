# ChessPush v2 · 0.2.0-alpha.1

Рабочая миграция ZST/PGN-парсера и доски на Tauri 2 / React / Rust.
Qt Quick/Python в корне репозитория остаётся эталоном `v0.1.0-alpha.1`.

## Подготовка и запуск

Windows 10/11, Node.js 24+, Rust stable MSVC (проверено на 1.98.1),
Visual Studio Build Tools с Desktop development with C++ и Windows SDK,
WebView2 Runtime. [Официальные prerequisites Tauri](https://v2.tauri.app/start/prerequisites/).
После установки Rust откройте новый терминал, чтобы обновился PATH.

Из каталога `v2`:

```powershell
npm ci
npm run tauri dev
```

Только frontend: `npm run dev`. В браузере доступна доска; файловые операции
работают в Tauri. В production HTTP-сервера нет: Vite нужен только при разработке.

```powershell
npm run tauri build
```

Результат: `src-tauri/target/release/chesspush.exe` и установщик в
`src-tauri/target/release/bundle/nsis/`. Старый ярлык Qt не заменяется.
Для быстрой локальной сборки: `npm run tauri build -- --debug --no-bundle`.

## Парсер

Откройте «Базы», выберите `.pgn.zst`, `.zst` или `.pgn`, задайте фильтры и
нажмите «Найти партии». Дебют можно выбрать поиском или разыграть на доске.
Последовательность ходов до текущей позиции становится префиксом поиска;
ECO применяется, если префикса нет. Произвольный FEN доступен для доски,
но не подменяет поиск последовательности ходов поиском позиции.

«Дополнительно»: разные рейтинги сторон, минимальная длина, число пропускаемых
партий, ECO, игрок для определения стороны, Bullet и просрочка времени.
«Остановить» сохраняет законченную часть выборки. Счётчики абсолютные;
полоса неопределённая, без вымышленного процента распаковки.

«Файлы»: история источников и результатов. Нажатие на имя показывает файл
в Проводнике, кнопка справа выбирает его источником. «Импорт PGN» сохраняет
партии без фильтрации. Копия импорта нормализуется в PGN; исходник не изменяется.

## Данные

Windows: `%APPDATA%/io.github.rainfault.chesspush.v2/chesspush.sqlite3`.
Рядом `exports/` с выборками и импортами. Используется стандартный Tauri
`app_data_dir`; пути на других ОС отличаются. Таблицы: `games`, `files`,
`file_games`, `settings`. Версия схемы — `PRAGMA user_version`, SQL в `migrations/`.
Сырые огромные корпуса остаются внешними файлами.

Старая SQLite-база и `data/settings.json` не читаются и не изменяются автоматически.
Существующие экспорты можно импортировать через «Файлы → Импорт PGN».

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

Быстрые тесты Rust без GUI-зависимостей:
`cargo test --manifest-path src-tauri/Cargo.toml --no-default-features`.
Эталонные JSON/PGN/ZST уже сохранены в `tests/fixtures/`, Python для этих тестов
не нужен. Переснять эталон из Qt-версии: `../.venv/Scripts/python tests/capture_reference.py`.

При работающем `npm run dev`: `node tests/ui-smoke.mjs` (использует установленный Edge).
Для native smoke соберите debug-приложение, задайте `CHESSPUSH_TEST_DATA` в отдельный
каталог и `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223`
только процессу тестового приложения. Затем задайте `CHESSPUSH_CDP=http://127.0.0.1:9223`
и выполните `node tests/native-smoke.mjs`, `node tests/ui-smoke.mjs`.
`native-smoke.mjs` требует предварительно созданный `.qa/benchmark.pgn.zst`.
Не включайте отладочный порт для обычной работы. Release игнорирует `CHESSPUSH_TEST_DATA`.

Benchmark: `../.venv/Scripts/python tests/benchmark.py`, затем
`cargo run --manifest-path src-tauri/Cargo.toml --release --no-default-features --example scan -- .qa/benchmark.pgn.zst`.

## Каталоги

- `src/chess/` — состояние доски и React-адаптер Chessground.
- `src/Databases.tsx`, `src/api.ts` — workflow и типы Tauri boundary.
- `src/theme.css`, `src/App.tsx` — две темы и общая навигация.
- `src-tauri/src/` — parser, filters, PGN, application, storage, desktop commands.
- `src-tauri/tests/`, `tests/` — parity, regression и UI-проверки.

Границы модулей: [ARCHITECTURE.md](ARCHITECTURE.md).
Совместимость и отличия: [MIGRATION.md](MIGRATION.md).
Зависимости и лицензии: [THIRD_PARTY.md](THIRD_PARTY.md).
