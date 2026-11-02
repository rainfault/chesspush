# Changelog

## Unreleased

## 1.0.0 — 2026-11-02

Первый публичный релиз: базовая версия для личной шахматной подготовки.

- Синхронизация завершённых личных партий Lichess, дебютная статистика и граф позиций.
- Разбор партий в общем PGN-редакторе: главы, варианты, комментарии, NAG, стрелки, круги, теги и анализ выбранной позиции Stockfish.
- Библиотека знаний с импортом/экспортом PGN и подключением знаний к репертуару без копирования редактируемого дерева.
- Повторение репертуара по FSRS-6, короткие учебные сессии и независимый полный прогон.
- Резервное копирование профиля и проверяемое восстановление со страховочным снимком.
- Сохранены identifier приложения, каталог профиля и схема SQLite 11.
- Зафиксированы ограничения подготовки всех PGN-ветвей, отображения пакетного анализа, области завершения тренировки, черновиков, границы T0 и переносимости внешних источников/доступа Lichess: [примечания к релизу](RELEASE_NOTES.md).

## 0.2.0-alpha.11 — 2026-11-02

- Add personal game review with a paginated, filtered game list, game tags, review state and the shared Study editor.
- Add cancellable Stockfish position analysis using the canonical move history and explicit provenance for saved knowledge.
- Connect study documents to repertoire and repetition through the same editable document and stable chapter/node identities.
- Add complete profile snapshots, validated restore previews, automatic safety backups and recovery during a coordinated restart.
- Preserve played games, existing knowledge, repertoire schedules and settings through SQLite migration 11.

## 0.2.0-alpha.10 — 2026-11-02

- Fit both opening-statistics tables within the window using compact rows and independent pagination sized to the available height.
- Add opening search, clearer result and repertoire indicators, and always-visible totals while retaining existing sorting, board previews and graph navigation.
- Preserve statistics calculations, IPC contracts, shared domain and database schema.

## 0.2.0-alpha.9 — 2026-11-02

- Add the Knowledge PGN library with automatic position/theory classification, search, type/tag filters, file import/export and guarded deletion.
- Extend the shared editor with a local Lichess Study layout, chapter ordering, editable PGN headers and persistent custom-tag suggestions.
- Integrate indexed knowledge positions and moves into the redesigned graph without changing factual game statistics; support direct navigation and creation from a graph position.
- Add transactional migration 10, retaining the shared KnowledgeDocument/VariationTree as the sole authoritative PGN model.
- Bundle pinned official Noto Sans and Roboto fonts with their Apache-2.0 license.

## 0.2.0-alpha.8 — 2026-11-02

- Replace fixed SM-2 scheduling with pinned FSRS 6.6.2, preserving existing due dates and lazily migrating memory state.
- Add short review-first sessions, limited new material, guided introductions and delayed independent recall after errors.
- Redesign repertoire training around the board, compact queue counters and faster transitions; keep Full Drill independent of SRS.
- Add SQLite migration 9 with a consistent pre-upgrade backup; retain repertoire identities, review history and resumable sessions.

- Replace the application and browser icons with a detailed transparent silver pawn; include Windows icon sizes and refresh desktop shortcut icons.
- Fix training database locks during background writes by reserving write transactions before reading session state; keep the training overview read-only.
- Preserve named chapter selection from multi-chapter PGNs in the combined viewer and both training modes, while retaining the global merged repertoire and existing review history.

- Added repertoire training with SM-2 spaced repetition and independent Full Drill coverage.
- Reuse the combined PGN VariationTree: transpositions share a position task with all prepared alternatives; opponent moves and short context replay run on Chessground.
- Persist review history, due dates and resumable sessions; reconcile PGN changes without losing unchanged decisions. Errors and hints require guided board correction.
- Added transactional SQLite migration 8 with a pre-upgrade backup and repertoire preservation check.

## 0.2.0-alpha.3 — 2026-10-14

- Restored Home clock, Lichess Rapid/Blitz ratings and offline authentication button.
- Added browser OAuth PKCE and manual token connection in compact settings.
- Tokens use Windows Credential Manager; old SQLite copies are migrated and removed.
- Sanitized published Git history and added a privacy check before commits.
- Added account/network/OAuth and Home interaction regression checks.

## 0.2.0-alpha.2 — 2026-10-14

- Tauri 2 / React / TypeScript / Rust is now the main project at repository root.
- Retired the active Qt/Python implementation; the original alpha remains in Git history.
- Added a backed-up, idempotent transfer of legacy games, file history, settings and saved positions to SQLite.
- Desktop shortcut targets the native ChessPush executable.
- Parity fixtures and native checks run without a Qt/Python installation.
- Corrected the visibility of PGN/FEN editor validation errors.

## 0.1.0-alpha.1 — 2026-09-28

Зафиксированная десктопная альфа перед переходом на новый стек.

- Qt Quick / Python: главная страница, ZST-парсер, компактные настройки.
- Рейтинги Lichess Rapid и Blitz, offline-режим, шкалы достижения 2000.
- Светлая и тёмная темы с общими дизайн-токенами.
- Официальный Chessground 10.4.0: легальные ходы, drag-and-drop,
  стрелки/кружки, навигация, превращение и повышенное разрешение фигур.
- Снято ограничение WebEngine в 60 Гц; на тестовом экране получено около
  143–144 представлений кадра/с. Добавлена предварительная загрузка доски.
- Потоковый парсинг ZST/PGN, фильтры дебюта и рейтинга, экспорт выборок.
- Постоянное хранение партий и файлов, управляемые копии импортированных PGN.
- Оконный запуск и скрипт создания ярлыка Windows.

Проверки: 84 unit-теста, Qt/WebEngine smoke-тест и замеры отрисовки.

Это исходный код альфа-версии, а не установщик. Проверенная среда — Windows,
Python 3.14, PySide6 6.11.1. Старые модули Statistics и Position Lab сохранены
в исходниках, но исключены из активного интерфейса. Новый стек ещё не выбран.

Личные настройки, токен, партии пользователя, локальные БД и результаты
запусков не входят в репозиторий. Лицензии сторонних компонентов и ассетов
сохранены рядом с ними.
