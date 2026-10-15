# Statistics — итог реализации

15 октября 2026. Задание 05c52a81 завершено в доступном объёме. Матрица всех 86 показателей: [STATISTICS_IMPLEMENTATION_COVERAGE.md](STATISTICS_IMPLEMENTATION_COVERAGE.md). Измерения и ограничения: [STATISTICS_PERFORMANCE.md](STATISTICS_PERFORMANCE.md).

## Покрытие

- COMPLETE (76): G01–G16; O01–O13; Q01–Q11; E01–E04 и E06–E11; T01–T10; S01–S04 и S07–S14; P01–P04.
- PARTIAL (2): E05 — подтверждённые потери матового пути с отдельной перепроверкой готовы, точный TB-фрагмент требует реальных таблиц; S05 — движковый диапазон сохранения ничейного состояния готов, TB-фрагмент требует таблиц.
- BLOCKED (8): O14/O15 — отсутствует согласованный версионированный PGN-репертуар; S06 — отсутствует реальный набор локальных Syzygy и успешный safe-context probe; P05–P09 — категория C исключена, исследовательские модели/валидация не созданы.
- MISSING: 0. Отсутствие часов/партии/пригодной оценки означает недоступное значение, не ноль. Exact TB-аналитика не считается проверенной по synthetic observations или tbhits.

## Что работает

Lichess polling во время приложения и при старте, immutable T0, атомарные начальные снимки доступных rating pools, overlapping creation ranges, durable pending IDs/ranges, 429/backoff, idempotent IDs, quarantine history и контрольная сверка. Сырой PGN/JSON и версии сохранены в SQLite v5; первичные данные не удаляются при пересчёте. Нет обязательного исторического импорта или OS service. Неатомарная pre-T0 граница и неограниченные задержки публикации не дают гарантии абсолютной полноты; lastMoveAt не выдаётся за endedAt.

Legal replay использует полную историю и отдельные position/line/history keys. Дебюты: именованные пути pinned catalogue, варианты/позиции/транспозиции, собственные и чужие ходы, visit/unique-game denominators, качество и часы, три границы выхода, первые потери, response windows и периоды. Q/E/S используют конкретный engine profile, same-root подтверждения, явный POV, rule-aware контексты и отдельные missing/bound/unstable states. Геометрические признаки не называются причинами ошибок. Прогресс сравнивает одинаковые условия; CI ресэмплируют целые партии.

Stockfish UCI: один supervised процесс, quick/standard/deep и custom limits, pause/cancel/retry/checkpoint/recovery, кэш по истории/профилю. Локальный optional Syzygy adapter реализован; большие файлы не загружались. Анализ допускается только после подтверждённого завершения партии.

GUI: Обзор (результаты/рейтинг/динамика/сессии/серии), Дебюты (доска/дерево/иерархия/качество/периоды), Качество игры (метрики/события/покрытие), Время (часы/условия/эпизоды/окна), Реализация и эндшпиль (преимущество/защита/тип/материал/пешечная геометрия), Партии (история/очередь/карантин/viewer). Фильтры и навигация сохраняются в mounted Statistics. Viewer открывает точный run/ply/FEN; явный устаревший run не подменяется другим. Обнаруженная гонка немедленного открытия после выбора профиля исправлена и перепроверена.

В Качестве игры доступна гистограмма Q02 с абсолютным n и переходами к партиям; неполный эпизод показывает последнюю пригодную позицию. Кнопки страниц находятся внутри раскрываемых таблиц и зависят от фактически отображаемых строк.

Полный domain report используется для расчётов. Projection ограничивает только IPC evidence, сохраняет полные агрегаты/counts/game_ids; примеры доступны страницами до 100. Fingerprint исходных games/runs/filter исключает смешение разных snapshots. Отдельные events/time/endings и session-gap subsets имеют собственные знаменатели. Каждые 10 с обновляется статус очереди; полная статистика обновляется по событиям, фильтрам и минутному fallback, без непрерывного дорогого пересчёта.

## Итоговые проверки — PASS

- cargo test: 147 unit + 3 legacy-import + 9 parity = 159; 0 failed, 3 real-SF opt-in отдельно.
- Все 3 настоящих Stockfish 19 opt-in tests отдельно PASS: handshake/searchmoves, full-game persist/metrics, отдельная mate recheck без смешения профилей.
- cargo clippy --all-targets -- -D warnings; cargo fmt --check; git diff --check.
- npm check/build/format:check; Vitest 3 файла / 11 tests; прежний account UI PASS.
- Desktop release cargo check --release --lib PASS; production GUI не запускался на личном профиле.
- Native parser/ZST/SQLite/account/Home/board regression PASS: реальные click/drag/arrows/circles/promotion/resize/themes.
- Statistics native: полный исправленный сценарий с новой SQLite / четырьмя запусками настоящего приложения PASS: empty/basic/filter/transposition/viewer/themes/quarantine/pause; interrupt/resume/completion; paging/stale snapshot/новые таблицы; persistence/cache/second profile и exact old run/FEN. Финальные артефакты .qa/statistics-native-aaVmpE. Ранее обнаруженная реальная profile race исправлена; полная перепроверка больше не опирается на продолжение частичного запуска.
- Новый свежий API flow на окончательной сборке: connect → штатное auto discovery → SQLite/results/openings → real Stockfish → critical FEN → closed-app catchup → interrupted/recovery → disconnect keeps history, все 3 фазы PASS. Каталог .qa/statistics-flow-6CPWAY.
- Volume native: 1001 синтетическая законная партия + persisted clones одного настоящего SF run, 17 017 own decisions, 6 разделов, фильтры, две темы, paused worker, heartbeat PASS. Каталог .qa/statistics-performance-b3eef979/volume-run-mO2dsZ.
- Поиск TODO/FIXME/todo!/unimplemented!/заглушек в Statistics: совпадений нет. Единственный silent UI catch относится к подписке на events; минутный fallback остаётся. Ошибки загрузки/engine/quarantine/clipboard отображаются или сохраняются в их состоянии.

Скриншоты/semantic tree проверены непосредственно в реальном Tauri/WebView2 через CDP. Computer Use runtime в этой среде ранее не смог запуститься из-за Windows sandbox helper; успешная компиляция не заменяла проверку окна. Обычный shell sandbox helper также требовал approved narrow escalated execution; один read-only auto-review истёк по timeout, однократный повтор прошёл. Незавершённых approval-зависимостей нет. Личные БД/credentials не использовались и не изменялись.

## Производительность и готовность

Release: 1000 партий — полный расчёт 1,66 с, дерево 6,36 мс, фильтр 0,77 с; JSON 118,9→15,7 MB (−86,8%). Rust benchmark peak ~534 MiB. Настоящий standard Stockfish 19, 33 ply, 1 thread/Hash128MiB — 157,08 с; исходный анализ сохранён и переиспользован при последующих нагрузочных повторах.

Native debug на 1001 партии: report IPC 19.20 с, фильтр IPC 12.56 с, GUI фильтры 15.71–20.92 с. Это ограничение debug и полного пересчёта, не скрытая фоновая ошибка. Разделы 109–271 мс; heartbeat p99 17,9 мс/max873,1 мс; host RSS после теста 106.6 MiB, JS heap ~51.1 MiB (разные области памяти). Полный release GUI не измерялся, объёмы >1001 не проверены. Подробнее и raw paths — в performance документе.

Statistics готова к повседневному поддерживаемому контуру Lichess → локальная история → анализ → статистика → партия, с указанными ограничениями объёма/скорости. Рекомендуется release для накопленных историй. Нужны подключение собственного Lichess и путь к локальному Stockfish; optional Syzygy требует предоставленных файлов. Stockfish в .qa — только QA, не поставка пользователю. Реальный private production account и успешный реальный Syzygy probe в этой сессии не проверены. Chess.com, репертуар/Knowledge/SRS и категория C не имитируются и не реализуются этим заданием. Данные и identities позволяют продолжить интеграции без вымышленных будущих сервисов.
