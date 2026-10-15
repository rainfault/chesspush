export function number(value: number | null | undefined, digits = 1) {
  return value == null || !Number.isFinite(value)
    ? "—"
    : new Intl.NumberFormat("ru-RU", { maximumFractionDigits: digits }).format(
        value,
      );
}
export function timestamp(value: number | null | undefined) {
  return value == null
    ? "—"
    : new Intl.DateTimeFormat("ru-RU", {
        day: "2-digit",
        month: "2-digit",
        year: "numeric",
        timeZone: "Europe/Moscow",
        hour: "2-digit",
        minute: "2-digit",
      }).format(value);
}
export function metricStatus(status: string) {
  return (
    (
      {
        BLOCKED: "нужен внешний источник",
        COMPLETE: "готово",
        PARTIAL: "частично готово",
        MISSING: "не реализовано",
        blocked_dependency: "нужны внешние данные",
        not_configured: "не настроено",
        missing_tables: "нет подходящих таблиц",
        missing_tables_or_rule_context:
          "нет таблиц или безопасного контекста правил",
        invalid_position: "некорректная позиция",
        castling_rights: "таблицы не поддерживают права рокировки",
        out_of_coverage: "вне покрытия таблиц",
        history_sensitive: "нужен учёт истории повторений",
        rounded_ambiguous: "исход неоднозначен из-за округления DTZ",
        missing_or_bound: "нет точечной оценки",
        bound: "оценка является границей",
        incomplete: "неполные данные",
        incomplete_analysis: "анализ не завершён",
        incomplete_or_bound: "неполная оценка или граница",
        conflicting_observations: "наблюдения противоречат друг другу",
        tablebase_only: "доступны только таблицы",
        no_analysis: "нет анализа",
        no_wdl: "нет WDL",
        no_boundary: "граница фазы не обнаружена",
        no_opening_sample: "нет дебютных решений",
        confirmed_first_loss: "первая потеря подтверждена",
        first_observed_loss_partial:
          "первая наблюдаемая потеря · анализ неполный",
        complete_no_loss: "существенных потерь не найдено",
        partial_no_loss: "потерь не найдено · анализ неполный",
        short_game: "недостаточно последующих решений",
        descriptive: "описательное сравнение",
        no_common_strata: "нет общих условий",
        mixed_rating_scales: "разные рейтинговые шкалы",
        unbounded_performance: "нет конечного значения перформанса",
        insufficient_quantile_sample: "недостаточно решений для квантилей",
        descriptive_episode: "описание эпизода",
        descriptive_first_observation: "первое наблюдение",
        descriptive_matched_windows: "сопоставимые окна решений",
        missing_profile: "нет совместимого анализа",
        missing_spent: "неизвестен расход часов",
        missing_remaining: "неизвестен остаток часов",
        missing_comparable_q: "нет сопоставимого ожидаемого очка",
        missing_same_root: "нет анализа из одного корня",
        incomplete_same_root: "анализ из одного корня не завершён",
        bound_or_incomplete_score: "граница или неполная оценка",
        missing_wdl_terminal_score: "нет WDL или подтверждённого исхода",
        invalid_token: "требуется повторное подключение",
        offline: "нет соединения",
        connected: "подключено",
        authenticating: "подключение",
        rate_limited: "пауза по ограничению Lichess",
        eligible: "включено в выборку",
        boundary_uncertain: "неопределённость границы T0",
        quarantined: "данные требуют проверки",
        terminal: "наблюдалось до завершения партии",
        exited: "порог эпизода утрачен",
        ready: "готово",
        idle: "проверено",
        syncing: "синхронизация",
        disconnected: "отключено",
        no_sample: "нет выборки",
        ambiguous_rating_baseline: "неоднозначный снимок T0",
        ambiguous_rating_time: "неоднозначное время рейтинга",
        no_independent_comparison: "периоды перекрываются",
        insufficient_interval_sample: "недостаточно партий для интервала",
        descriptive_degenerate_interval: "описательное сравнение",
        missing_common_strata: "нет общих условий",
        no_data: "нет данных",
        error: "ошибка",
        retryable_failed: "повтор",
        permanent_failed: "ошибка",
        missing_input: "нет данных",
        pending: "в очереди",
        unsupported: "не поддерживается",
        unstable: "нестабильная оценка",
        failed: "ошибка",
        available: "готово",
        complete: "готово",
        queued: "в очереди",
        running: "анализ",
        cancelled: "отменено",
        paused: "пауза",
        succeeded: "готово",
      } as Record<string, string>
    )[status] ?? status
  );
}

const labels: Record<string, string> = {
  own: "Собственные ходы",
  opponent: "Ответы соперника",
  inaccuracy: "Неточность",
  mistake: "Ошибка",
  blunder: "Зевок",
  significant: "Существенная потеря",
  opportunity_used: "Возможность использована",
  opportunity_missed: "Возможность упущена",
  advantage_entry: "Вход в устойчивое преимущество",
  defense_entry: "Вход в трудное состояние",
  advantage: "Преимущество",
  defense: "Защита",
  same_bishops: "Одноцветные слоны",
  opposite_bishops: "Разноцветные слоны",
  kingside: "Короткая рокировка",
  queenside: "Длинная рокировка",
  same: "Односторонние рокировки",
  opposite: "Разносторонние рокировки",
  wings: "Стороны рокировки",
  isolated: "Изолированные пешки",
  doubled: "Сдвоенные пешки",
  passed: "Геометрически проходные пешки",
  open: "Открытые файлы",
  half: "Полуоткрытые файлы",
  half_count: "Число полуоткрытых файлов",
  open_count: "Число открытых файлов",
  signature: "Точная пешечная форма",
  transition: "Пешечный переход",
  initial: "В исходной позиции",
  reached: "Достигнуто в партии",
  queen_capture: "Взятие ферзя",
  position: "Позиция",
  path: "Линия",
  white: "Белые",
  black: "Чёрные",
  rated: "Рейтинговые",
  casual: "Нерейтинговые",
  unknown: "Нет данных",
  lichess: "Lichess",
  standard: "Обычные шахматы",
  fromPosition: "Заданная позиция",
  ultraBullet: "Ультрапуля",
  bullet: "Пуля",
  blitz: "Блиц",
  rapid: "Рапид",
  classical: "Классика",
  correspondence: "По переписке",
  mate: "Мат",
  resign: "Сдача",
  stalemate: "Пат",
  timeout: "Время / отключение",
  outoftime: "Просрочка времени",
  draw: "Ничья",
  pawn: "Пешечные",
  rook: "Ладейные",
  queen: "Ферзевые",
  minor: "Лёгкие фигуры",
  mixed: "Смешанные",
  none: "Нет",
  opening: "Дебют",
  middlegame: "Миттельшпиль",
  endgame: "Эндшпиль",
  decision: "Решение",
};
export function groupLabel(value: string): string {
  if (value.includes(" · "))
    return value.split(" · ").map(groupLabel).join(" · ");
  const [kind, ...parts] = value.split(":");
  if (
    kind === "color" ||
    kind === "rated" ||
    kind === "termination" ||
    kind === "variant"
  )
    return groupLabel(parts.join(":"));
  if (kind === "rating_diff")
    return "Разница рейтингов: " + (labels[parts.join(":")] ?? parts.join(":"));
  if (kind === "control")
    return parts.join(":").split(" ").map(groupLabel).join(" · ");
  if (parts.length)
    return [kind, ...parts].map((p) => labels[p] ?? p).join(" · ");
  return labels[value] ?? value;
}
