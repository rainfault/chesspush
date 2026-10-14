# Миграция альфы

## Фактическая исходная версия

Эталон — чистая рабочая копия `main`, commit
`74e01035baa8a02df11ceecd9d9b9253504b8165`, tag `v0.1.0-alpha.1`.
История до него: `d24b483`, `791919e`. Незакоммиченных исправлений перед переносом не было.
Несмотря на упоминание C++ в задании, реализация — **PySide6/Qt Quick + Python**.
ZST читает Python `zstandard`, правила доски выполняет `python-chess`.
Нет C++-парсера или скрытого sidecar, который следовало бы переносить.

| Эталон | Новая реализация |
|---|---|
| `zst_reader.py`, потоковый `pgn_parser.py` | `parser.rs`, `pgn.rs` |
| `game_filters.py`, `model_game_search.py` | `filters.rs`, `parser::scan` |
| `extraction_worker.py`, `ParserController` | `application.rs`, Tauri worker + events |
| `file_library.py`, canonical games | отдельная SQLite v2, games/files/file_games |
| `board_session.py` / python-chess | TypeScript `BoardSession` / chessops |
| `board.js`, QWebChannel, WebEngine | React `ChessBoard` + официальный Chessground |
| QML shell / Theme | React shell / CSS tokens |

Qt-исходники, их 84 теста, локальные данные и ярлык оставлены без изменений.
Новый рабочий фундамент находится только в `v2/`; Qt используется как reference.
Публикация коммита/ветки на GitHub не выполняется автоматически этим переносом.

## Сохранённое поведение

- Потоковое чтение PGN/ZST, cheap tag prefilter до movetext.
- Rating ranges обеих сторон, minimum both, rating difference, ECO prefix/range/list,
  opening substring, SAN prefix с очисткой комментариев/NAG/вариантов, control, длина,
  Bullet, timeout, результаты относительно стороны/игрока.
- Event имеет приоритет над TimeControl. Fallback использует начальные секунды,
  как эталон, без новой формулы учёта increment.
- `start_index=N` пропускает N сырых записей. Scan limit ограничивает просмотр после
  пропуска; UI limit ограничивает найденные партии. Счётчик абсолютный и 64-битный.
- Сторона задаёт перспективу результата. Без выбранной/определённой стороны фильтр
  результата не отбрасывает игру. Неизвестный Result сохраняет поведение эталона.
- Дебют доски — префикс ходов, а не FEN/transposition matcher. При нём ECO отключён.
- Отмена сохраняет полные найденные записи. Ошибка PGN-экспорта не отменяет SQLite-save.
- Нет фиктивного процента чтения сжатого файла; есть progress events и счётчики.
- Chessground **10.4.0**: SVG cburnett, brown colors, 200ms animation, click/drag,
  legal destinations, оба способа рокировки, en passant, четыре превращения,
  arrows/circles/clear, FEN/PGN mainline, flip, keyboard и cursor navigation.
- Сохранены исправления stale revision, сохранения аннотаций по позиции,
  лишней передачи FEN после хода и повторной загрузки доски при навигации.

## Зафиксированные результаты

`tests/capture_reference.py` исполняет именно Python-эталон и создаёт синтетические
fixtures. `reference.json` содержит **22 сценария** с абсолютными индексами, metadata,
tags и raw PGN, а не ожидаемые результаты, сгенерированные Rust-кодом.
`python_reference_parity` сравнивает каждый сценарий на PGN и ZST: **44 сравнения совпали**.
Проверяются defaults/all, ratings, controls, стороны/username, outcomes, ECO,
opening, annotated prefix, min moves, skipping, match/scan limits, EOF и cancellation.
Отдельной статистики в активном эталонном workflow нет — сравниваются scanned/matched.

Rust regression tests также проверяют CRLF, truncated/UTF-8 errors, плохой ECO range,
SQLite migrations/save, export failure и flush частичной выборки при отмене.
Frontend unit tests: legal/illegal/stale moves, branching, castling, en passant,
promotion, notes, atomic imports. UI smoke в Edge и native WebView2: реальные click,
drag, arrows, circles, clear, navigation, promotion, opening picker, light/dark и resize.
Native smoke проверяет commands/events, точную выборку из 7 партий, отмену на большем
дампе, отзывчивость доски, ошибку повреждённого архива, историю и тему после reload.
При проверке packaged WebView2 исправлена CSP: локальный каталог дебютов доступен
через `connect-src 'self'`. Native smoke повторён успешно после исправления.

На текущем 144 Hz дисплее WebView2: 144.00 requestAnimationFrame/с, медиана 6.9 мс,
p95 7.0 мс, DPR 1.25 (240 кадров, `tests/frame-check.mjs`). Это cadence кадрового
цикла, не измерение scanout и не гарантия отсутствия отдельных пропусков при нагрузке.

Итоговый прогон: 9 Rust tests (включая 44 parity comparisons), 5 frontend tests,
84 теста сохранённой Qt/Python-версии. TypeScript, Prettier, cargo fmt/check/clippy
пройдены. Собраны debug executable и release executable + NSIS installer.

Синтетический benchmark, 120 000 коротких игр / 90 000 совпадений, один локальный запуск:
Python 4.59 с (~26k игр/с), Rust release 0.95 с (~126k игр/с). Измерен scan/filter,
без SQLite/export. Это не оценка полного production-дампа или универсальное ускорение.

## Намеренные отличия / исправления

| Случай | Зафиксированный старый результат | v2 |
|---|---|---|
| ZST с CRLF | 12 игр склеиваются в 1, metadata последней | 12 отдельных игр; CRLF нормализуется |
| Обрезанный ZST | 0 игр, без ошибки на fixture | structured `decode` error; полные ранее сохранённые записи остаются |
| Неверный UTF-8 в ZST | UnicodeDecodeError | structured `utf8` error |
| ECO `B12-` | IndexError | безопасный no-match |
| Очень длинная/повреждённая запись | неограниченный tail | explicit error выше 16 MiB |
| Lichess id в storage | fingerprint понижает регистр | case-sensitive identity, чтобы не объединять разные игры |

Экспорт использует уникальное имя с timestamp, формат содержимого сохранён (LF).
В import сохраняются распознанные записи, а не побайтовая копия исходника; исходный
файл остаётся в истории. SQLite содержит только импорт/выборки, не весь сырой корпус.
Старая БД не мигрируется автоматически: её игры можно перенести существующим PGN-export/import.
Дедупликация не-Lichess игр по PGN может оставить две записи при разных комментариях.

Отброшены только Qt-specific детали: QML controls, QThread/QObject signals,
JSON/WebChannel bridge, WebEngine preload-overlay и DPR supersampling workaround.
Нет принудительного отключения vsync или обещания 144 FPS на любом оборудовании.
Иконки/PNG/шрифты из Lila в v2 не копируются; cburnett SVG поставляется официальным
npm-пакетом Chessground, как разрешённая зависимость, с его license notice.

## Следующая проверка

Проверить одну репрезентативную выборку на настоящем большом Lichess ZST:
время, память, отмена, содержимое PGN и удобство фильтров. До этой проверки
не добавлять третий крупный модуль.
