import { controlLabel } from "../control";
import type { Filter, Report, StatisticsState } from "./api";
type Props = {
  filter: Filter;
  state: StatisticsState | null;
  report: Report | null;
  graph: boolean;
  onChange: (filter: Partial<Filter>) => void;
  onReset: () => void;
  availableControls?: string[];
};
function date(ms?: number | null) {
  return ms == null
    ? ""
    : new Date(ms + 3 * 3600000).toISOString().slice(0, 10);
}
function dateMs(value: string, end = false) {
  return value
    ? Date.parse(value + "T00:00:00+03:00") + (end ? 86400000 : 0)
    : null;
}
export function PracticeFilters({
  filter,
  state,
  report,
  graph,
  onChange,
  onReset,
  availableControls = [],
}: Props) {
  const controls = [
    ...new Set([
      filter.control ?? "",
      ...availableControls,
      ...(report?.games.map((game) => game.control) ?? []),
    ]),
  ].filter(Boolean);
  return (
    <div className="stats-filters practice-filters">
      {graph ? (
        <div className="practice-side" role="group" aria-label="Моя сторона">
          {[
            ["white", "Я белыми"],
            ["black", "Я чёрными"],
          ].map(([value, label]) => (
            <button
              key={value}
              className={(filter.color ?? "white") === value ? "selected" : ""}
              aria-pressed={(filter.color ?? "white") === value}
              onClick={() => onChange({ color: value })}
            >
              {label}
            </button>
          ))}
        </div>
      ) : (
        <label>
          Сторона
          <select
            aria-label="Цвет статистики"
            value={filter.color ?? ""}
            onChange={(e) => onChange({ color: e.target.value || null })}
          >
            <option value="">Обе</option>
            <option value="white">Белыми</option>
            <option value="black">Чёрными</option>
          </select>
        </label>
      )}
      <label>
        Период
        <select
          aria-label="Период практики"
          value={filter.since_ms || filter.until_ms ? "custom" : "all"}
          onChange={(e) =>
            onChange(
              e.target.value === "all"
                ? { since_ms: null, until_ms: null }
                : { since_ms: Date.now() - 30 * 86400000, until_ms: null },
            )
          }
        >
          <option value="all">Вся моя практика</option>
          <option value="custom">Выбранный период</option>
        </select>
      </label>
      <label>
        Контроль
        <select
          aria-label="Контроль времени статистики"
          value={filter.control ?? ""}
          onChange={(e) => onChange({ control: e.target.value || null })}
        >
          <option value="">Все</option>
          {controls.map((control) => (
            <option key={control} value={control}>
              {controlLabel(control)}
            </option>
          ))}
        </select>
      </label>
      <details className="stats-extra">
        <summary>Фильтры</summary>
        <div className="stats-extra-fields">
          {!!state?.accounts.length && (
            <label>
              Аккаунт
              <select
                aria-label="Аккаунт статистики"
                value={
                  filter.account_id ??
                  state.active_account_id ??
                  state.accounts[0]?.id ??
                  ""
                }
                onChange={(e) => onChange({ account_id: e.target.value })}
              >
                {state.accounts.map((account) => (
                  <option key={account.id} value={account.id}>
                    {account.username}
                  </option>
                ))}
              </select>
            </label>
          )}
          <label>
            С
            <input
              aria-label="Начало периода"
              type="date"
              value={date(filter.since_ms)}
              onChange={(e) => onChange({ since_ms: dateMs(e.target.value) })}
            />
          </label>
          <label>
            По
            <input
              aria-label="Конец периода"
              type="date"
              value={date(
                filter.until_ms == null ? null : filter.until_ms - 86400000,
              )}
              onChange={(e) =>
                onChange({ until_ms: dateMs(e.target.value, true) })
              }
            />
          </label>
          <label>
            Категория
            <select
              aria-label="Категория рейтинга"
              value={filter.speed ?? ""}
              onChange={(e) => onChange({ speed: e.target.value || null })}
            >
              <option value="">Все</option>
              {[
                "ultraBullet",
                "bullet",
                "blitz",
                "rapid",
                "classical",
                "correspondence",
              ].map((value) => (
                <option key={value}>{value}</option>
              ))}
            </select>
          </label>
          {!graph && (
            <label>
              Фаза
              <select
                aria-label="Фаза игры"
                value={filter.phase ?? ""}
                onChange={(e) => onChange({ phase: e.target.value || null })}
              >
                <option value="">Все</option>
                <option value="opening">Дебют</option>
                <option value="middlegame">Миттельшпиль</option>
                <option value="endgame">Эндшпиль</option>
              </select>
            </label>
          )}
          <label>
            Дебют
            <input
              aria-label="Фильтр дебюта"
              value={filter.opening ?? ""}
              onChange={(e) => onChange({ opening: e.target.value || null })}
            />
          </label>
          <label>
            Рейтинг от
            <input
              type="number"
              aria-label="Рейтинг соперника от"
              value={filter.opponent_rating_min ?? ""}
              onChange={(e) =>
                onChange({
                  opponent_rating_min: e.target.value
                    ? Number(e.target.value)
                    : null,
                })
              }
            />
          </label>
          <label>
            Рейтинг до
            <input
              type="number"
              aria-label="Рейтинг соперника до"
              value={filter.opponent_rating_max ?? ""}
              onChange={(e) =>
                onChange({
                  opponent_rating_max: e.target.value
                    ? Number(e.target.value)
                    : null,
                })
              }
            />
          </label>
          <label>
            Партии
            <select
              aria-label="Тип партий"
              value={filter.rated == null ? "all" : String(filter.rated)}
              onChange={(e) =>
                onChange({
                  rated:
                    e.target.value === "all" ? null : e.target.value === "true",
                })
              }
            >
              <option value="all">Все</option>
              <option value="true">Рейтинговые</option>
              <option value="false">Нерейтинговые</option>
            </select>
          </label>
          <label>
            Источник
            <select
              aria-label="Источник партий"
              value={filter.source ?? ""}
              onChange={(e) => onChange({ source: e.target.value || null })}
            >
              <option value="">Все</option>
              <option value="lichess">Lichess</option>
            </select>
          </label>
          <label>
            Вариант
            <select
              aria-label="Вариант шахмат"
              value={filter.variant ?? ""}
              onChange={(e) => onChange({ variant: e.target.value || null })}
            >
              <option value="standard">Обычные шахматы</option>
              <option value="">Все</option>
            </select>
          </label>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={filter.include_uncertain ?? false}
              onChange={(e) =>
                onChange({ include_uncertain: e.target.checked })
              }
            />
            Пограничные партии начала наблюдения
          </label>
          <button onClick={onReset}>Сбросить фильтры</button>
        </div>
      </details>
    </div>
  );
}
