# Changelog

## Unreleased

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
