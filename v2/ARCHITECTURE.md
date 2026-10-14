# Архитектура v2

```text
React: App / Databases / ChessBoard
  ├─ BoardSession → chessops (легальные ходы, FEN, SAN)
  │                  ↓
  │              Chessground (DOM, input, animation, drawing)
  └─ invoke / listen
       ↓
     desktop.rs (Tauri commands, events, один worker)
       ↓
     application.rs (выборка / импорт / экспорт / сохранение)
       ├─ parser.rs → поток PGN или ZST → pgn.rs / filters.rs
       └─ storage.rs → SQLite + SQL migrations
```

## Данные и парсер

`domain.rs`: общие `Game`, `Record`, `Filters`, `SourceKind`, `AppError`.
Нет отдельных классов партии для своих игр, Lichess и мастеров.
Партия имеет стабильный SQLite id; происхождение хранится отдельно от шахматных
метаданных. Будущие связи с узлами и знаниями можно добавить отдельными таблицами;
сами Knowledge/Node/Pattern модели сейчас не создаются.

`PgnStream` держит буфер распаковки, строку и одну партию. Лимит записи — 16 MiB;
превышение даёт явную ошибку, а не неограниченный рост памяти. ZST decoder разрешает
окно до 2 GiB для крупных Lichess-архивов; это отдельный от буфера PGN расход памяти.
Метаданные проверяются до разбора movetext. Префикс токенизируется один раз на запуск.
Валидация легальности каждого хода корпуса не добавлена: эталон её не выполняет.

`application::run` пишет PGN последовательно и коммитит SQLite пачками по 250 партий
или 2 MiB PGN. Не собирает всю выборку в `Vec`. Только выбранные/импортированные партии
попадают в application DB. Корпус не копируется и не индексируется в неё целиком.

SQLite: WAL, foreign keys, busy timeout, атомарная migration 001. Соединение принадлежит
конкретной операции; глобального mutex вокруг БД нет. Lichess id сохраняет регистр.
Для остальных партий ключ дедупликации — SHA-256 нормализованного сырого PGN.

## Tauri boundary

- `start_parse(request, id)` резервирует единственную задачу и запускает `spawn_blocking`.
- `parser-progress`: `{ id, progress: { scanned, matched, cancelled } }`, примерно до 10/с.
- `parser-finished`: `{ id, result: { Ok: ParseResult } | { Err: AppError } }`.
- `cancel_parse(id)` устанавливает atomic flag конкретной задачи.
- `library_files`, `remember_file`, `reveal_file`, `get_theme`, `set_theme` — короткие операции.

`AppError` содержит `code` и `message`; ошибка чтения также приходит с итоговыми счётчиками
и путём частичного результата. Отмена проверяется между строками и партиями.
При закрытии окна во время операции сначала запрашивается отмена и сбрасываются буферы.
Единственный `Mutex<Option<ActiveJob>>` защищает запуск/отмену; цикл парсера его не держит.

Frontend подписывается на события до разрешения запуска, отбрасывает чужие job id,
не управляет streaming loop. Tauri dialog нужен только для выбора файла. Произвольного
shell command, HTTP backend, Python/C++ sidecar нет. Explorer получает путь только по id
записи библиотеки, не произвольную командную строку из UI.

## Доска

`BoardSession` владеет легальной позицией, короткой главной линией, курсором, ориентацией
и аннотациями по ply. Загрузка PGN/FEN атомарна. Ход после возврата обрезает продолжение.
Ревизия защищает от устаревшего выбора превращения. Полного дерева анализа нет.

`ChessBoard` создаёт официальный Chessground 10.4.0 один раз, обновляет configuration,
использует native drag/click/draw и ResizeObserver. Перетаскивание не проходит через React
на каждом кадре. Уже нарисованная позиция не присылается обратно как новый FEN:
это сохраняет исправление перезапуска анимации из Qt-версии.
Страница остаётся mounted при навигации; нет загрузки отдельного WebEngine и QWebChannel.
При hidden-состоянии браузер вправе приостанавливать кадры; ResizeObserver обновляет доску
при возвращении. Частота кадров управляется WebView2/дисплеем, не таймером React.

Темы, размеры, spacing и цвета — CSS tokens. Используются стандартные brown/cburnett
стили официального пакета; цветная сетка доски задана собственным CSS gradient.
Код Lila, brown.png, icon font и Qt supersampling в v2 не переносились.

## Отложено

Home — часы; остальные разделы кроме Databases — пустые страницы с заголовком.
Settings содержит тему. Lichess API/токен, engine, statistics v2, Knowledge, SRS,
индекс корпусов, Pattern Matcher, автоматическая миграция старой БД и сложный PGN tree
не входят в этот этап. Нет Redux, generic repositories/traits или системы плагинов.
