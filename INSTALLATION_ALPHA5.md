# ChessPush 0.2.0-alpha.5 — установленное обновление

18 октября 2026 года. Установлен релиз текущего checkout с продуктовым интерфейсом: Главная, Моя игра, Репертуар, Библиотека и Настройки.

- NSIS /S /UPDATE, exit code 0; прежний каталог установки и identifier io.github.rainfault.chesspush.v2.
- Desktop, Start Menu и существующий Taskbar shortcut направлены на C:\Users\<user>\AppData\Local\ChessPush\chesspush.exe. Arguments пусты; working directory и icon указывают на установленную программу; AppUserModelID сохранён.
- ProductVersion: 0.2.0-alpha.5. SHA256 установленного exe: BF43217A261BD8EDFFF5BCF856C0E772AAECE0E9EF3C81DCC700ABF64FEA60C0. Совпадение с релизом проверено с учётом служебного NSIS-маркера.
- Перед первым запуском все исходные строки всех таблиц совпали с backup; schema 7, integrity ok, foreign_key_check 0.
- После запуска все 401 исходные партии, PGN/payload, завершённый анализ, оценки и Account ID/T0 сохранены. Учётные данные Windows существуют под прежним target; токен не считывался и не экспортировался средствами обновления.
- Штатная синхронизация догрузила 14 завершённых партий: теперь 15 личных партий, 415 партий всего; граф автоматически проиндексировал 15. Операционные статусы синхронизации/очереди обновились; это не потеря исходных строк.
- Запуск выполнен через настоящий Desktop shortcut. Accessibility установленного WebView подтвердил новое меню, личные партии и подключённый <local-account> с актуальными рейтингами. Приложение оставлено открытым.
- Дополнительный GUI-проход после установки ограничен ошибками helper: FrameArrived timed out / window capture timed out; переход по элементу не выполнялся после сбоя. Это ограничение инструмента проверки, не подтверждённая ошибка ChessPush. Полные GUI-сценарии frontend этой же сборки выполнены на предыдущем этапе.

Резервная копия: C:\Users\<user>\AppData\Local\ChessPush Backups\20261018-015020-alpha5. Содержит SQLite backup, Roaming/Local WebView state, прежний exe/uninstaller и три ярлыка. Пароли и токены из Credential Manager в неё не экспортировались.

Протоколы: .qa/update-alpha5/install-result.json, data-verification-before.json, data-verification-after.json, credential-and-shortcuts.json. Исходники шахматных расчётов и схема не менялись; обновлены только номера релиза в манифестах и README. Build, 33 frontend-теста и format:check — PASS.
