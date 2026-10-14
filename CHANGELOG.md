# Changelog

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
