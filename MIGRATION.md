# Миграция на Tauri

## Основная версия

Начиная с `0.2.0-alpha.2` Tauri 2 / React / TypeScript / Rust размещены в корне
репозитория и являются единственной рабочей реализацией. Qt/Python, QML, WebChannel,
прежние workers и их dependencies исключены из текущего checkout.

Исторический эталон сохранён под tag `v0.1.0-alpha.1`, commit
`74e01035baa8a02df11ceecd9d9b9253504b8165`. Полный checkpoint перед переносом в корень —
commit `e46ae15`. Прежняя фактическая реализация была **PySide6/Qt Quick + Python**,
а не C++. Рабочие локальные `data/`, `outputs/` и Tauri application data сохраняются
отдельно от исходников и не публикуются в Git.

| Исторический эталон | Текущая реализация |
|---|---|
| `zst_reader.py`, `pgn_parser.py` | `src-tauri/src/parser.rs`, `pgn.rs` |
| `game_filters.py`, `model_game_search.py` | `filters.rs`, `parser::scan` |
| `extraction_worker.py`, ParserController | `application.rs`, Tauri worker/events |
| `file_library.py`, canonical games | SQLite games/files/file_games |
| `board_session.py`, python-chess | TypeScript BoardSession / chessops |
| board.js, QWebChannel, WebEngine | React ChessBoard / официальный Chessground |
| QML shell / Theme | React shell / CSS tokens |

Нет постоянного Python/C++ sidecar, HTTP backend или одновременного развития двух UI.

## Сохранённое поведение

- Потоковое PGN/ZST чтение с проверкой tags до movetext.
- Rating ranges сторон, minimum both, rating difference, ECO prefix/range/list,
  opening substring, SAN prefix с очисткой comments/NAG/вариантов, control, длина,
  Bullet, timeout и результат относительно стороны/игрока.
- Event имеет приоритет над TimeControl; fallback сохраняет прежнюю формулу секунд
  без изменения semantics increment.
- `start_index=N` пропускает N сырых записей. Scan limit ограничивает просмотр после
  пропуска; UI limit ограничивает найденные игры. Счётчик абсолютный, 64-битный.
- Сторона — перспектива результата. Без определённой стороны outcome-фильтр не
  отбрасывает игру. Неизвестный Result сохраняет эталонное поведение.
- Дебют доски — prefix ходов; ECO при нём отключён. FEN/transposition matcher отсутствует.
- Отмена сохраняет полные найденные записи. Ошибка PGN export не отменяет SQLite save.
- Progress events показывают counts, без фиктивного процента сжатого файла.
- Chessground 10.4.0: cburnett SVG, brown colors, 200ms animation, drag/click,
  legal destinations, обе формы castling, en passant, четыре promotion choices,
  arrows/circles/clear, FEN/PGN mainline, flip и cursor/keyboard navigation.
- Сохранены stale revision protection, annotations по позиции, отсутствие лишней
  передачи FEN после хода и повторной загрузки доски при навигации.

## Behavioral parity

`tests/fixtures/reference.json` содержит **22 сценария**, снятые исходным Python parser,
с абсолютными индексами, metadata, tags и raw PGN. Каждый сценарий проверяется на
PGN и ZST: **44 сравнения совпали при исходной миграции**. Снимок остаётся неизменным;
ожидаемые результаты не генерируются переписанным Rust parser.

Проверяются defaults/all, ratings, controls, стороны/username, outcomes, ECO, opening,
annotated prefix, min moves, skipping, match/scan limits, EOF и cancellation.
Regression tests покрывают CRLF, truncated/UTF-8 errors, malformed ECO, SQLite,
export failure и flush частичной выборки при отмене.

Скрипт съёма исторического эталона доступен в checkpoint
`e46ae15:v2/tests/capture_reference.py`. Он исключён из активных tests: текущие
проверки не зависят от Qt/Python/backend. Synthetic performance input генерирует
`tests/benchmark.mjs` стандартными Node.js streams и `node:zlib`.

Frontend tests покрывают legal/illegal/stale moves, branching, castling, en passant,
promotion, annotations и atomic imports. UI smoke в Edge/WebView2 проверяет реальные
click/drag, arrows/circles/clear, navigation, promotion, openings, themes и resize.
Native smoke дополнительно проверяет commands/events, точные 7 PGN matches,
progress/cancellation, отзывчивость доски, corrupt-input error, историю и theme reload.
Проверяется видимость ошибки неверного FEN поверх его dialog.

Историческая проверка до переноса в корень: 9 Rust tests, 5 frontend tests,
84 Qt/Python tests, TypeScript, formatter, cargo fmt/check/clippy, debug/release
executables и NSIS installer. Эти результаты относятся к этапу переноса двух вертикалей;
Qt tests теперь находятся только в Git history.

На 144 Hz дисплее WebView2 ранее измерено 144.00 requestAnimationFrame/с,
median 6.9 ms, p95 7.0 ms, DPR 1.25 (`tests/frame-check.mjs`, 240 кадров).
Это cadence цикла кадров, не измерение scanout или гарантия при любой нагрузке.

Исторический synthetic scan/filter benchmark: 120 000 игр / 90 000 matches,
Python 4.59 s (~26k игр/s), Rust release 0.95 s (~126k игр/s). SQLite/export не
измерялись; результат не является оценкой полного production dump.

## Намеренные исправления

| Случай | Исторический результат | Текущая реализация |
|---|---|---|
| ZST CRLF | 12 игр склеивались в 1 | 12 отдельных игр |
| Truncated ZST fixture | 0 игр без ошибки | structured decode error |
| Invalid ZST UTF-8 | UnicodeDecodeError | structured utf8 error |
| ECO `B12-` | IndexError | безопасный no-match |
| Повреждённая длинная запись | неограниченный tail | explicit error выше 16 MiB |
| Lichess storage id | fingerprint понижал регистр | case-sensitive identity |

Export имеет уникальное timestamp-имя и сохраняет LF PGN content.
Import нормализует распознанные записи; оригинал остаётся в истории.
Non-Lichess PGN с разными комментариями может дать две записи дедупликации.

## Пользовательские данные и запуск

Tauri identifier `io.github.rainfault.chesspush.v2` намеренно сохранён: root promotion
не переключает каталог `%APPDATA%` и не создаёт пустую библиотеку вместо существующей.

Одноразовый Rust example `migrate_legacy` переносит прежнюю SQLite-библиотеку в Tauri
DB. Он получает source SQLite, settings JSON, target SQLite и backup directory.
Перенос читает исходники и сохраняет их на месте; перед изменениями целевой базы
создаётся резервная копия. Игры и история файлов переходят в основную библиотеку;
legacy-only данные сохраняются без добавления нового раздела UI. Ник хранится локально;
при первом запуске старый токен переносится в защищённое хранилище Windows и удаляется
из текущей SQLite, включая архив настроек. Резервные копии остаются только локально.
Команда и пути описаны в [README.md](README.md).

Ярлык ChessPush теперь указывает на native Tauri executable; Python interpreter
при запуске не используется. Прежние Qt-specific workaround (WebChannel batching,
WebEngine preload, DPR supersampling) заменены прямой web-интеграцией.

## Следующий шаг

Проверить одну репрезентативную выборку настоящего большого Lichess ZST:
время, память, отмену и PGN результат. До этого не добавлять третий крупный модуль.

Проверки основной версии из корня: 12 Rust-тестов (включая 44 parity-сравнения),
5 frontend-тестов, TypeScript, Prettier, cargo fmt и Clippy прошли.
`npm run test:native` собрал и проверил собственный WebView2-процесс с отдельной
SQLite: ходы, drag, аннотации, PGN/FEN, обе темы, выборку, прогресс, отмену, ошибки
и сохранение истории. Qt/Python-среда из рабочего каталога удалена.

## Возврат Home и подключения Lichess

Home перенесён из прежней Qt-версии: часы, официальные Rabbit/Fire, два рейтинга,
шкала последних 100 пунктов до 2000, offline без устаревших рейтингов. Токен
принимается через настройки; браузерный вход OAuth PKCE добавлен впервые.
Текущий аккаунт определяется по токену, а не по независимо введённому нику.
Сетевые ошибки сохраняют доступ; disconnect удаляет только локальную учётную запись.

Опубликованная история веток и alpha-тегов очищена от прежних локальных выгрузок,
настроек, персональных идентификаторов в примерах и личной Git-метадаты. Поэтому
хэши исторических коммитов изменились. Лицензии и авторство сторонних ассетов сохранены.
Реальные пользовательские данные и отчёты проверки в репозиторий не включаются.

Проверки возврата Home/Lichess: 24 Rust-теста (включая frozen parser parity),
11 frontend/privacy-тестов, TypeScript, Prettier, cargo fmt и Clippy прошли.
Нативный WebView2 с отдельной БД проверил доску, парсер, токен-валидацию и settings.
OAuth-обмен и отмена проверены локальными HTTP-стендами; реальный Lichess принял
параметры public-client/loopback/S256. Согласие на доступ остаётся действием пользователя.
