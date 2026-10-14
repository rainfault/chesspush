# Архитектура ChessPush

```text
React: App / Home / Settings / Databases / ChessBoard
  ├─ BoardSession → chessops → Chessground
  └─ invoke / listen
       ↓
     desktop.rs: Tauri commands, events, один worker
       ↓
     application.rs: выборка / импорт / экспорт / сохранение
       ├─ parser.rs → PGN/ZST stream → pgn.rs / filters.rs
       └─ storage.rs → SQLite + SQL migrations
```

## Парсер и данные

`domain.rs` содержит общие `Game`, `Record`, `Filters`, `SourceKind`, `AppError`.
Партия имеет стабильный SQLite id, происхождение — metadata. Отдельные модели партии
по источнику и будущие Knowledge/Pattern-схемы сейчас не создаются.

`PgnStream` хранит буфер распаковки, строку и одну партию. Запись ограничена 16 MiB;
ZST decoder допускает окно до 2 GiB для Lichess-архивов. Метаданные фильтруются до
разбора movetext; префикс токенизируется один раз. Легальность всех ходов корпуса
не проверяется: исходный parser этого не делал.

`application::run` пишет PGN последовательно, SQLite коммитит пачками по 250 партий
или 2 MiB PGN. Выборка целиком в памяти не собирается. В основной DB хранятся только
выбранные и импортированные игры, история файлов и локальные настройки; сырой корпус остаётся внешним.

SQLite использует WAL, foreign keys, busy timeout и `PRAGMA user_version` migrations.
Соединение принадлежит операции; глобального mutex БД нет. Lichess id сохраняет регистр.
Для прочих игр identity — SHA-256 нормализованного сырого PGN.

Application-data identifier `io.github.rainfault.chesspush.v2` сохранён при переносе
проекта в корень. Одноразовый Rust example `migrate_legacy` переносит старую библиотеку
в существующую Tauri DB с резервной копией. Старые данные сохраняются отдельно от кода.
Схема v2 добавляет legacy_imports и legacy_rows: одноразовый marker и lossless архив
прежних записей с новыми game_id. Архив сохраняет заметки/позиции до реализации
соответствующего UI. Этот инструмент не зависит от Qt или Python.

## Tauri boundary

- `start_parse(request, id)` резервирует одну задачу и запускает `spawn_blocking`.
- `parser-progress`: `{ id, progress: { scanned, matched, cancelled } }`, примерно до 10/с.
- `parser-finished`: `{ id, result: { Ok: ParseResult } | { Err: AppError } }`.
- `cancel_parse(id)` устанавливает atomic flag конкретной задачи.
- `library_files`, `remember_file`, `reveal_file`, `get_theme`, `set_theme` — короткие операции.

`AppError` содержит code/message. Ошибка чтения сопровождается счётчиками и частичным
результатом. Отмена проверяется между строками и партиями; закрытие окна сначала
запрашивает отмену и flush. Единственный `Mutex<Option<ActiveJob>>` защищает запуск
и отмену; цикл парсера его не удерживает.

Frontend подписывается до запуска, отбрасывает события чужих job id. Tauri dialog
выбирает файл; Explorer получает путь из библиотеки по id. Произвольных shell commands,
HTTP backend и sidecar нет.

## Доска и UI

TypeScript `BoardSession` владеет легальной позицией, главной линией, курсором,
ориентацией и аннотациями по ply. PGN/FEN загружаются атомарно; ход после возврата
обрезает продолжение. Ревизия защищает от устаревшего превращения.

React `ChessBoard` создаёт официальный Chessground 10.4.0 один раз, обновляет config
и использует native drag/click/draw. Drag не проходит через React на каждом кадре.
FEN уже нарисованного хода не присылается повторно, чтобы не перезапускать анимацию.
ResizeObserver обновляет размеры. Страница остаётся mounted при навигации;
WebView2/дисплей определяют частоту кадров.

Две темы, spacing, размеры и цвета находятся в CSS tokens. Используются стандартные
brown/cburnett стили официального npm-пакета; сетка доски задана CSS gradient.

## Границы текущего этапа

Home показывает часы и текущие Rapid/Blitz; Databases содержит парсер и доску.
Settings: проверенный ник, токен, вход через браузер, тема и отключение аккаунта.
Остальные разделы — пустые страницы с заголовком. Engine, Knowledge, SRS, индексы
корпусов, Pattern Matcher и полное PGN tree отложены.
Redux, generic repositories/traits и framework поверх React отсутствуют.

Прежний Qt/Python-код находится только в Git history (`v0.1.0-alpha.1`).
Сборка, тесты и запуск текущего проекта используют Node.js/Rust/Tauri.

## Аккаунт Lichess

`lichess.rs` владеет сетевой логикой, состоянием подключения и OAuth PKCE.
`credentials.rs` хранит токен через Windows Credential Manager; в SQLite остаётся ник.
При старте прежний plaintext-токен переносится и удаляется из активной БД и архива
настроек. Существующие локальные резервные копии не изменяются.

Commands: `account_status`, `connect_lichess_token`, `start_lichess_auth`,
`cancel_lichess_auth`, `disconnect_lichess`. Event: `lichess-account-updated`.
DTO содержит status, username, rapid, blitz, message, hasToken; токен не возвращается.
Длительные операции выполняются через spawn_blocking, отдельно от парсера.
Сетевые запросы сериализованы, refresh ограничен 15 секундами, UI обновляется раз
в минуту; HTTP429 учитывает Retry-After с минимумом 60 секунд.

OAuth использует системный браузер, случайные state/verifier, S256, public client
без client_secret и дополнительных scopes. Callback — временный loopback listener
на случайном 127.0.0.1-порту (180 секунд), проверяющий state и путь. Отмена и смена
поколения не позволяют старому ответу переподключить аккаунт.
Ошибки не включают URL callback, заголовки, token или содержимое ответов API.
При offline рейтинги скрыты, сохранённый доступ остаётся. На других ОС адаптер
защищённого хранилища пока не реализован; plaintext fallback отсутствует.
