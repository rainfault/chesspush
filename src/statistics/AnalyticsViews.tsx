import {
  OpeningQualityDetails,
  OpeningTemporalDetails,
  NodeTablebase,
} from "./OpeningDetails";
import { useEvidence, MoreEvidence, useEvidenceCounts } from "./Evidence";
import { useState } from "react";
import type { Metric, Report, Filter, EvidenceSelector } from "./api";
import type { EngineDetails, OpeningNodeAnalysis } from "./analyticsTypes";
import { number, timestamp, groupLabel, metricStatus } from "./format";
export type OpenPosition = (id: string, ply: number, runId?: string) => void;
type Ref = {
  game_id: string;
  ply: number;
  run_id?: string | null;
  fen?: string;
};
export function Cases({
  items,
  onOpen,
  before = false,
  label = "Случаи",
  selector,
  mapPage,
}: {
  items: unknown[];
  onOpen: OpenPosition;
  before?: boolean;
  label?: string;
  selector?: EvidenceSelector;
  mapPage?: (items: unknown[]) => Ref[];
}) {
  const evidence = useEvidence<unknown>(items, selector);
  const rows = mapPage ? mapPage(evidence.items) : (evidence.items as Ref[]);
  return (
    <details className="stats-cases">
      <summary>
        {label} ({evidence.total})
      </summary>
      <div className="stats-table-wrap">
        <table aria-label={label}>
          <thead>
            <tr>
              <th>Партия</th>
              <th>Ply</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r, i) => (
              <tr key={r.game_id + ":" + r.ply + ":" + i}>
                <td>{r.game_id.replace("lichess:", "")}</td>
                <td>{r.ply}</td>
                <td>
                  <button
                    onClick={() =>
                      onOpen(
                        r.game_id,
                        Math.max(0, r.ply - (before ? 1 : 0)),
                        r.run_id ?? undefined,
                      )
                    }
                  >
                    Открыть
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <MoreEvidence value={evidence} />
      </div>
    </details>
  );
}
function Missing({ count }: { count: number }) {
  return count ? null : (
    <div className="stats-chart-empty">Нет пригодных случаев</div>
  );
}
export function NodeEvaluations({
  node,
  onOpen,
}: {
  node: OpeningNodeAnalysis;
  onOpen: OpenPosition;
}) {
  return (
    <details>
      <summary>
        Оценки позиции · {node.usable_contexts}/{node.unique_contexts}{" "}
        контекстов
      </summary>
      <div className="stats-table-wrap">
        <table aria-label="Контексты позиции">
          <thead>
            <tr>
              <th>q, доля</th>
              <th>CP / мат за белых</th>
              <th>W / D / L за белых, ‰</th>
              <th>POV для q</th>
              <th>Состояние</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {node.contexts.map((c) => (
              <tr key={c.context_key}>
                <td>{number(c.q, 3)}</td>
                <td>
                  {c.evaluation?.mate != null
                    ? "#" + c.evaluation.mate
                    : number(c.evaluation?.cp, 0)}
                </td>
                <td>{c.evaluation?.wdl?.join(" / ") ?? "—"}</td>
                <td>{groupLabel(c.perspective)}</td>
                <td>{metricStatus(c.status)}</td>
                <td>
                  <Cases
                    selector={{ kind: "node", key: c.context_key }}
                    items={c.cases}
                    onOpen={onOpen}
                  />
                  {c.tablebase && <NodeTablebase observation={c.tablebase} />}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </details>
  );
}
export function OpeningViews({
  report,
  onOpen,
  onGames,
  onFilter,
  renderMetrics,
}: {
  report: Report;
  onOpen: OpenPosition;
  onGames: (ids: string[]) => void;
  onFilter: (f: Partial<Filter>) => void;
  renderMetrics: (m: Metric[], name: string) => React.ReactNode;
}) {
  const [limit, setLimit] = useState(80),
    [onlyRepeated, setOnlyRepeated] = useState(false);
  const firstLosses = useEvidence(report.opening_analysis.first_losses, {
    kind: "first_losses",
    key: "all",
  });
  const a = report.opening_analysis,
    hasComparison =
      report.filter.comparison_since_ms != null &&
      report.filter.comparison_until_ms != null;
  return (
    <>
      <details open>
        <summary>Дебюты, варианты и подварианты ({a.hierarchy.length})</summary>
        <div className="stats-table-wrap">
          <table aria-label="Иерархия дебютов">
            <thead>
              <tr>
                <th>Ветвь</th>
                <th>Цвет</th>
                <th>N / доля, %</th>
                <th>W / D / L</th>
                <th>Очки, %</th>
                <th>ΔR, медиана · N</th>
                <th>Loss, п.п. · n</th>
                <th>ACPL</th>
                <th>Accuracy</th>
                <th>≥10 п.п. /100</th>
                <th>Spent, с</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {a.hierarchy.slice(0, limit).map((n) => (
                <tr key={n.id}>
                  <td style={{ paddingLeft: 12 + (n.path.length - 1) * 18 }}>
                    <button
                      className="stats-link"
                      onClick={() =>
                        onFilter({
                          opening_label_id: n.id,
                          opening: null,
                          line: null,
                          position_key: null,
                          color: n.color,
                        })
                      }
                    >
                      {n.label}
                    </button>
                  </td>
                  <td>{groupLabel(n.color)}</td>
                  <td>
                    {n.result.games} / {number(n.frequency)}
                  </td>
                  <td>
                    {n.result.wins}/{n.result.draws}/{n.result.losses}
                  </td>
                  <td>{number(n.result.score)}</td>
                  <td>
                    {number(n.rating_difference_median, 0)} ·{" "}
                    {n.rating_difference_sample}
                  </td>
                  <td>
                    {number(n.quality.mean_loss, 2)} · {n.quality.wdl_sample}/
                    {n.quality.decisions}
                  </td>
                  <td>{number(n.quality.acpl)}</td>
                  <td>{number(n.quality.accuracy)}</td>
                  <td>{number(n.quality.significant_per_100)}</td>
                  <td>
                    {number(n.quality.median_spent)} · {n.quality.spent_sample}
                  </td>
                  <td>
                    <button onClick={() => onGames(n.result.game_ids)}>
                      Партии
                    </button>
                    <Cases
                      selector={{ kind: "hierarchy", key: n.id }}
                      items={n.cases}
                      onOpen={onOpen}
                    />
                    <OpeningQualityDetails
                      quality={n.quality}
                      selectorBase={{ kind: "hierarchy", key: n.id }}
                      onOpen={onOpen}
                    />
                    {hasComparison && (
                      <details>
                        <summary>Периоды</summary>
                        {renderMetrics(n.comparison, "Периоды ветви")}
                        <OpeningQualityDetails
                          quality={n.previous_quality}
                          selectorBase={{
                            kind: "hierarchy",
                            key: n.id + "::previous_quality",
                          }}
                          onOpen={onOpen}
                        />
                      </details>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {a.hierarchy.length > limit && (
            <button onClick={() => setLimit(limit + 100)}>
              Ещё 100 ветвей
            </button>
          )}
        </div>
      </details>
      <details>
        <summary>Выход из дебюта</summary>
        {a.boundaries.map((b) => (
          <BoundaryCases key={b.kind} b={b} onOpen={onOpen} />
        ))}
      </details>
      <details>
        <summary>
          Первая существенная потеря · N={firstLosses.total} партий
        </summary>
        <div className="stats-table-wrap">
          <table aria-label="Первая потеря в дебюте">
            <thead>
              <tr>
                <th>Партия</th>
                <th>Ход</th>
                <th>Loss, п.п.</th>
                <th>Анализ, n</th>
                <th>Состояние</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {firstLosses.items.map((r) => (
                <tr key={r.game_id}>
                  <td>{r.game_id}</td>
                  <td>{r.case?.san ?? "—"}</td>
                  <td>{number(r.loss_q, 2)}</td>
                  <td>
                    {r.analysed}/{r.decisions}
                  </td>
                  <td>{metricStatus(r.status)}</td>
                  <td>
                    <button
                      disabled={!r.case}
                      onClick={() =>
                        r.case &&
                        onOpen(
                          r.game_id,
                          Math.max(0, r.case.ply - 1),
                          r.case.run_id ?? undefined,
                        )
                      }
                    >
                      Позиция до решения
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <MoreEvidence value={firstLosses} />
      </details>
      <details>
        <summary>Ответы соперника: следующие 3 решения</summary>
        <label className="checkbox">
          <input
            type="checkbox"
            checked={onlyRepeated}
            onChange={(e) => setOnlyRepeated(e.target.checked)}
          />
          Повторявшиеся ответы
        </label>
        <div className="stats-table-wrap">
          <table aria-label="Трудные ответы соперника">
            <thead>
              <tr>
                <th>Ответ</th>
                <th>Посещения / игры</th>
                <th>Доля, %</th>
                <th>Полные / исключённые окна</th>
                <th>Loss, п.п.</th>
                <th>≥10 п.п. /100</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {a.opponent_responses
                .filter((r) => !onlyRepeated || r.games > 1)
                .slice(0, limit)
                .map((r) => (
                  <tr key={r.position_key + r.played_uci}>
                    <td>{r.san}</td>
                    <td>
                      {r.visits}/{r.games}
                    </td>
                    <td>{number(r.frequency)}</td>
                    <td>
                      {r.eligible_windows}/{r.excluded_windows}
                    </td>
                    <td>{number(r.mean_loss, 2)}</td>
                    <td>{number(r.significant_per_100)}</td>
                    <td>
                      <Cases
                        selector={{
                          kind: "response",
                          key: r.position_key + "|" + r.played_uci,
                        }}
                        mapPage={(rows) =>
                          (rows as typeof r.cases).map((c) => c.response)
                        }
                        items={r.cases}
                        onOpen={onOpen}
                        label="Ответы"
                      />
                      <Cases
                        selector={{
                          kind: "response",
                          key: r.position_key + "|" + r.played_uci,
                        }}
                        mapPage={(rows) =>
                          (rows as typeof r.cases).flatMap((c) => c.decisions)
                        }
                        items={r.cases}
                        before
                        onOpen={onOpen}
                        label="Окна решений"
                      />
                    </td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
        {a.opponent_responses.filter((r) => !onlyRepeated || r.games > 1)
          .length > limit && (
          <button onClick={() => setLimit(limit + 100)}>Ещё 100 ответов</button>
        )}
      </details>
      {hasComparison && (
        <details open>
          <summary>Изменение дебютных показателей</summary>
          {renderMetrics(a.comparison, "Динамика дебютов")}
          <div className="stats-table-wrap">
            <table aria-label="Динамика продолжений">
              <thead>
                <tr>
                  <th>Ход</th>
                  <th>Доля A, %</th>
                  <th>Доля B, %</th>
                  <th>Δ, п.п.</th>
                  <th>Новые / повторные пути</th>
                  <th>Впервые в истории</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {report.opening_tree.edges.map((e) => (
                  <tr key={e.uci}>
                    <td>{e.san}</td>
                    <td>{number(e.temporal.current_share)}</td>
                    <td>{number(e.temporal.previous_share)}</td>
                    <td>{number(e.temporal.share_delta)}</td>
                    <td>
                      {e.temporal.new_paths}/{e.temporal.repeated_paths}
                    </td>
                    <td>{timestamp(e.temporal.first_seen_ms)}</td>
                    <td>
                      <Cases
                        selector={{ kind: "edge", key: e.uci }}
                        items={e.cases}
                        onOpen={onOpen}
                      />
                      <OpeningTemporalDetails
                        temporal={e.temporal}
                        uci={e.uci}
                        onOpen={onOpen}
                      />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </details>
      )}
      <details>
        <summary>Репертуар</summary>
        <table aria-label="Зависимости репертуара">
          <tbody>
            {a.blockers.map((b) => (
              <tr key={b.id}>
                <td>{b.id}</td>
                <td>{metricStatus(b.status)}</td>
                <td>{b.dependency}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </>
  );
}
export function EngineEvidence({
  analysis,
  prefixes,
  onOpen,
}: {
  analysis: EngineDetails;
  prefixes: string[];
  onOpen: OpenPosition;
}) {
  const [metric, setMetric] = useState("");
  const counts = useEvidenceCounts();
  const ids = Object.keys(counts)
    .filter((k) => k.startsWith("engine:") && k !== "engine:all")
    .map((k) => k.slice(7))
    .filter((id) => prefixes.some((p) => id.startsWith(p)));
  const selected = metric || ids[0] || "all";
  const evidence = useEvidence(
    analysis.cases.filter((c) => c.metric_id === selected),
    { kind: "engine", key: selected },
  );
  const filtered = evidence.items;
  return (
    <details>
      <summary>Измерения и позиции ({evidence.total})</summary>
      <select
        aria-label="Показатель списка позиций"
        value={selected}
        onChange={(e) => {
          setMetric(e.target.value);
        }}
      >
        {ids.map((id) => (
          <option key={id}>{id}</option>
        ))}
      </select>
      <div className="stats-table-wrap">
        <table aria-label="Измерения решений">
          <thead>
            <tr>
              <th>ID</th>
              <th>Партия / ply</th>
              <th>Событие</th>
              <th>Значение</th>
              <th>Фаза</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((c, i) => (
              <tr key={c.game_id + c.metric_id + c.ply + i}>
                <td>{c.metric_id}</td>
                <td>
                  {c.game_id.replace("lichess:", "")} / {c.ply}
                </td>
                <td>{c.label}</td>
                <td>
                  {number(c.value, 2)} {c.unit}
                </td>
                <td>{groupLabel(c.phase)}</td>
                <td>
                  <button
                    onClick={() => onOpen(c.game_id, c.position_ply, c.run_id)}
                  >
                    Позиция
                  </button>
                  <details>
                    <summary>Данные</summary>
                    <pre className="stats-evidence-json">{c.details}</pre>
                  </details>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <MoreEvidence value={evidence} />
      </div>
      <Missing count={evidence.total} />
    </details>
  );
}
export function ExtendedViews({
  report,
  section,
  onOpen,
  onGames,
}: {
  report: Report;
  section: "overview" | "quality" | "time" | "endings";
  onOpen: OpenPosition;
  onGames: (ids: string[]) => void;
}) {
  const [gap, setGap] = useState(30),
    [limit, setLimit] = useState(100),
    [context, setContext] = useState("");
  const x = report.extended;
  const sessions = useEvidence(
      x.sessions.filter((s) => s.gap_minutes === gap),
      { kind: "sessions", key: String(gap) },
    ),
    streaks = useEvidence(x.streaks, { kind: "streaks", key: "all" }),
    episodes = useEvidence(report.analysis.episodes, {
      kind: "episodes",
      key: "all",
    });
  const progressRows = x.progress.filter((r) =>
    section === "time"
      ? r.metric_id.startsWith("T")
      : section === "endings"
        ? r.metric_id.startsWith("P03")
        : true,
  );
  const visibleRows = Math.max(
    progressRows.length,
    section === "time"
      ? x.time_contexts.filter((r) => !context || r.metric_id === context)
          .length
      : section === "endings"
        ? x.structures.length
        : 0,
  );
  return (
    <>
      {section === "overview" && (
        <>
          <details>
            <summary>Игровые сессии</summary>
            <label>
              Разрыв, мин
              <select
                aria-label="Разрыв между сессиями"
                value={gap}
                onChange={(e) => setGap(Number(e.target.value))}
              >
                {[15, 30, 60].map((n) => (
                  <option key={n}>{n}</option>
                ))}
              </select>
            </label>
            <div className="stats-table-wrap">
              <table aria-label="Игровые сессии">
                <thead>
                  <tr>
                    <th>Начало</th>
                    <th>N</th>
                    <th>W/D/L</th>
                    <th>Очки, %</th>
                    <th>Достоверный интервал, с</th>
                    <th>Между началами, с</th>
                    <th>Порядок</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {sessions.items
                    .filter((s) => s.gap_minutes === gap)
                    .map((s) => (
                      <tr key={s.id}>
                        <td>{timestamp(s.first_start_ms)}</td>
                        <td>{s.games}</td>
                        <td>
                          {s.wins}/{s.draws}/{s.losses}
                        </td>
                        <td>{number(s.score)}</td>
                        <td>{number(s.span_seconds)}</td>
                        <td>{number(s.observed_start_span_seconds)}</td>
                        <td>
                          {s.order_ambiguous
                            ? "Неоднозначен"
                            : s.overlapping
                              ? "Пересечение"
                              : "Наблюдаемый"}
                        </td>
                        <td>
                          <button onClick={() => onGames(s.game_ids)}>
                            Партии
                          </button>
                        </td>
                      </tr>
                    ))}
                </tbody>
              </table>
            </div>
            <MoreEvidence value={sessions} />
          </details>
          <details>
            <summary>Серии результатов ({streaks.total})</summary>
            <div className="stats-table-wrap">
              <table aria-label="Серии результатов">
                <thead>
                  <tr>
                    <th>Исход</th>
                    <th>Длина</th>
                    <th>Период</th>
                    <th>Условия</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {streaks.items.map((s) => (
                    <tr key={s.id}>
                      <td>{s.kind === "win" ? "Победы" : "Поражения"}</td>
                      <td>{s.length}</td>
                      <td>
                        {timestamp(s.first_ms)} — {timestamp(s.last_ms)}
                      </td>
                      <td>{groupLabel(s.stratum)}</td>
                      <td>
                        <button onClick={() => onGames(s.game_ids)}>
                          Партии
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <MoreEvidence value={streaks} />
          </details>
        </>
      )}
      {section === "time" && (
        <details open>
          <summary>Временные условия и эпизоды</summary>
          <select
            aria-label="Временной показатель"
            value={context}
            onChange={(e) => {
              setContext(e.target.value);
              setLimit(100);
            }}
          >
            <option value="">Все</option>
            {Array.from(new Set(x.time_contexts.map((r) => r.metric_id))).map(
              (id) => (
                <option key={id}>{id}</option>
              ),
            )}
          </select>
          <div className="stats-table-wrap">
            <table aria-label="Временные условия">
              <thead>
                <tr>
                  <th>Условие</th>
                  <th>Страта</th>
                  <th>Значение</th>
                  <th>N / доступно</th>
                  <th>Интервал</th>
                  <th>Loss, п.п.</th>
                  <th>Потери ≥10 п.п.</th>
                  <th>Состояние</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {x.time_contexts
                  .filter((r) => !context || r.metric_id === context)
                  .slice(0, limit)
                  .map((r) => (
                    <tr key={r.id}>
                      <td>{r.label}</td>
                      <td>{groupLabel(r.stratum)}</td>
                      <td>
                        {number(r.value, 2)} {r.unit}
                      </td>
                      <td>
                        {r.sample}/{r.eligible}
                      </td>
                      <td
                        title={
                          r.status === "descriptive_iqr"
                            ? "P25…P75"
                            : "95% bootstrap целых партий"
                        }
                      >
                        {r.low == null
                          ? "—"
                          : number(r.low, 2) + "…" + number(r.high, 2)}
                        {r.status === "descriptive_iqr" ? " · IQR" : ""}
                      </td>
                      <td>{number(r.mean_loss, 2)}</td>
                      <td>{r.significant}</td>
                      <td>{metricStatus(r.status)}</td>
                      <td>
                        <Cases
                          selector={{ kind: "context", key: r.id }}
                          items={r.evidence}
                          onOpen={onOpen}
                        />
                      </td>
                    </tr>
                  ))}
              </tbody>
            </table>
          </div>
        </details>
      )}
      {section === "endings" && (
        <>
          <details>
            <summary>Эпизоды преимущества и защиты ({episodes.total})</summary>
            <div className="stats-table-wrap">
              <table aria-label="Эпизоды преимущества и защиты">
                <thead>
                  <tr>
                    <th>Тип</th>
                    <th>Партия</th>
                    <th>Вход / подтверждение</th>
                    <th>Выход / завершение</th>
                    <th>Последняя позиция, ply</th>
                    <th>Своих решений</th>
                    <th>Σ loss, п.п.</th>
                    <th>Состояние</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {episodes.items.map((r, i) => (
                    <tr key={r.game_id + r.start_ply + r.kind + i}>
                      <td>
                        {r.kind === "advantage" ? "Преимущество" : "Защита"}
                      </td>
                      <td>{r.game_id}</td>
                      <td>
                        {r.start_ply}/{r.confirmation_ply}
                      </td>
                      <td>
                        {r.exit_ply ?? "—"}/{r.terminal_ply ?? "—"}
                      </td>
                      <td>
                        <button
                          className="stats-link"
                          onClick={() => onOpen(r.game_id, r.end_ply, r.run_id)}
                        >
                          {r.end_ply}
                        </button>
                      </td>
                      <td>{r.own_decisions}</td>
                      <td>{number(r.total_loss, 2)}</td>
                      <td>{metricStatus(r.status)}</td>
                      <td>
                        <button
                          onClick={() =>
                            onOpen(r.game_id, r.start_ply, r.run_id)
                          }
                        >
                          Вход
                        </button>
                        <button
                          disabled={r.first_loss_ply == null}
                          onClick={() =>
                            onOpen(
                              r.game_id,
                              Math.max(0, (r.first_loss_ply ?? 1) - 1),
                              r.run_id,
                            )
                          }
                        >
                          Первая потеря
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <MoreEvidence value={episodes} />
          </details>
          <details>
            <summary>
              Материал, пешечные структуры и рокировки ({x.structures.length})
            </summary>
            <div className="stats-table-wrap">
              <table aria-label="Позиционные признаки">
                <thead>
                  <tr>
                    <th>ID</th>
                    <th>Признак</th>
                    <th>Партии</th>
                    <th>Посещения</th>
                    <th>Знаменатель</th>
                    <th>Доля, %</th>
                    <th></th>
                  </tr>
                </thead>
                <tbody>
                  {x.structures.slice(0, limit).map((r) => (
                    <tr key={r.id}>
                      <td>{r.metric_id}</td>
                      <td>{r.label}</td>
                      <td>{r.games}</td>
                      <td>{r.visits}</td>
                      <td>{r.decisions}</td>
                      <td>{number(r.share)}</td>
                      <td>
                        <Cases
                          selector={{ kind: "structure", key: r.id }}
                          items={r.evidence}
                          onOpen={onOpen}
                        />
                        <button onClick={() => onGames(r.game_ids)}>
                          Партии
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </details>
          <div className="stats-status">
            Syzygy: {metricStatus(report.analysis.tablebase_status)}
          </div>
        </>
      )}
      {x.progress.length > 0 && (
        <details>
          <summary>Сопоставимые позиции и периоды</summary>
          <div className="stats-table-wrap">
            <table aria-label="Прогресс в сопоставимых условиях">
              <thead>
                <tr>
                  <th>ID</th>
                  <th>Условие</th>
                  <th>A</th>
                  <th>B</th>
                  <th>Δ A−B</th>
                  <th>N A / B</th>
                  <th>95% интервал</th>
                  <th>Состояние</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {progressRows.slice(0, limit).map((r) => (
                  <tr key={r.id}>
                    <td>{r.metric_id}</td>
                    <td>{r.label}</td>
                    <td>
                      {number(r.current, 2)} {r.unit}
                    </td>
                    <td>{number(r.previous, 2)}</td>
                    <td>{number(r.delta, 2)}</td>
                    <td>
                      {r.current_sample}/{r.previous_sample}
                    </td>
                    <td>
                      {r.low == null
                        ? "—"
                        : number(r.low, 2) + "…" + number(r.high, 2)}
                    </td>
                    <td>{metricStatus(r.status)}</td>
                    <td>
                      <Cases
                        selector={{ kind: "progress", key: r.id }}
                        items={r.evidence}
                        onOpen={onOpen}
                      />
                      <button onClick={() => onGames(r.game_ids)}>
                        Партии
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </details>
      )}
      {section === "quality" && (
        <details>
          <summary>Покрытие и исключения</summary>
          <table aria-label="Покрытие анализа">
            <thead>
              <tr>
                <th>Причина</th>
                <th>Партии</th>
                <th>Решения</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {x.coverage.map((r, i) => (
                <tr key={r.reason + i}>
                  <td>{metricStatus(r.reason)}</td>
                  <td>{r.games}</td>
                  <td>{r.decisions}</td>
                  <td>
                    <button
                      disabled={!r.game_ids.length}
                      onClick={() => onGames(r.game_ids)}
                    >
                      Партии
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </details>
      )}
      {visibleRows > limit && (
        <button
          className="stats-load-more"
          onClick={() => setLimit(limit + 100)}
        >
          Показать ещё 100 строк
        </button>
      )}
    </>
  );
}

function BoundaryCases({
  b,
  onOpen,
}: {
  b: Report["opening_analysis"]["boundaries"][number];
  onOpen: OpenPosition;
}) {
  const evidence = useEvidence(b.cases, { kind: "boundary", key: b.kind });
  return (
    <details>
      <summary>
        {{
          phase: "Граница фаз",
          last_named: "Последняя именованная позиция",
          fixed_20: "Срез 20 ply",
        }[b.kind] ?? b.kind}{" "}
        · {b.sample}/{b.eligible} · q {number(b.q_mean, 3)}
      </summary>
      <div className="stats-table-wrap">
        <table aria-label={"Выход: " + b.kind}>
          <thead>
            <tr>
              <th>Партия</th>
              <th>Ply</th>
              <th>CP / мат</th>
              <th>q, доля</th>
              <th>POV</th>
              <th>Состояние</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {evidence.items.map((c, i) => (
              <tr key={c.game_id + i}>
                <td>{c.game_id}</td>
                <td>{c.ply ?? "—"}</td>
                <td>{c.mate != null ? "#" + c.mate : number(c.cp, 0)}</td>
                <td>{number(c.q, 3)}</td>
                <td>{groupLabel(c.pov)}</td>
                <td>{metricStatus(c.status)}</td>
                <td>
                  <button
                    disabled={!c.case}
                    onClick={() =>
                      c.case &&
                      onOpen(c.game_id, c.case.ply, c.case.run_id ?? undefined)
                    }
                  >
                    Позиция
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <MoreEvidence value={evidence} />
    </details>
  );
}
