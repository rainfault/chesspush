import { useState } from "react";
import type {
  OpeningQuality,
  OpeningTemporal,
  TablebaseObservation,
} from "./analyticsTypes";
import { Cases, type OpenPosition } from "./AnalyticsViews";
import { number, timestamp, metricStatus } from "./format";

type Selector = { kind: string; key: string };

export function OpeningQualityDetails({
  quality,
  selectorBase,
  onOpen,
}: {
  quality: OpeningQuality;
  selectorBase: Selector;
  onOpen: OpenPosition;
}) {
  const [limit, setLimit] = useState(30);
  const qualityKey = /::(?:previous_)?quality$/.test(selectorBase.key)
    ? selectorBase.key
    : `${selectorBase.key}::quality`;
  const selector = { kind: selectorBase.kind, key: qualityKey };
  return (
    <details>
      <summary>Качество и часы · {quality.decisions} решений</summary>
      <div className="stats-table-wrap">
        <table aria-label="Качество дебютных решений">
          <thead>
            <tr>
              <th>Показатель</th>
              <th>Значение</th>
              <th>Известно / решений</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>Lq, среднее / медиана / P90, п.п.</td>
              <td>
                {number(quality.mean_loss, 2)} /{" "}
                {number(quality.median_loss, 2)} / {number(quality.p90_loss, 2)}
              </td>
              <td>
                {quality.wdl_sample} / {quality.decisions}
              </td>
            </tr>
            <tr>
              <td>ACPL, сантипешки</td>
              <td>{number(quality.acpl, 2)}</td>
              <td>
                {quality.cp_sample} / {quality.decisions}
              </td>
            </tr>
            <tr>
              <td>Accuracy, гармоническое, %</td>
              <td>{number(quality.accuracy, 2)}</td>
              <td>
                {quality.accuracy_sample} / {quality.decisions}
              </td>
            </tr>
            <tr>
              <td>Lq ≥10 п.п., случаев / на 100 решений</td>
              <td>
                {quality.significant} / {number(quality.significant_per_100, 2)}
              </td>
              <td>
                {quality.wdl_sample} / {quality.decisions}
              </td>
            </tr>
            <tr>
              <td>Известный расход, сумма, с</td>
              <td>{number(quality.total_spent, 2)}</td>
              <td>
                {quality.spent_sample} / {quality.decisions}
              </td>
            </tr>
            <tr>
              <td>Расход на решение, медиана, с</td>
              <td>{number(quality.median_spent, 2)}</td>
              <td>
                {quality.spent_sample} / {quality.decisions}
              </td>
            </tr>
            <tr>
              <td>Известный расход на партию, среднее, с</td>
              <td>{number(quality.mean_spent_per_game, 2)}</td>
              <td>{quality.spent_game_sample} партий</td>
            </tr>
            <tr>
              <td>Покрытие часов, %</td>
              <td>{number(quality.clock_coverage)}</td>
              <td>
                {quality.spent_sample} / {quality.decisions}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <Cases
        items={quality.cases}
        selector={selector}
        onOpen={onOpen}
        label="Решения ветви"
      />
      <details>
        <summary>Часы по партиям ({quality.time_games.length})</summary>
        <div className="stats-table-wrap">
          <table aria-label="Дебютный расход по партиям">
            <thead>
              <tr>
                <th>Партия</th>
                <th>Известная сумма, с</th>
                <th>Известно / решений</th>
                <th>Покрытие, %</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {quality.time_games.slice(0, limit).map((g) => (
                <tr key={g.game_id}>
                  <td>{g.game_id.replace("lichess:", "")}</td>
                  <td>{number(g.known_seconds, 2)}</td>
                  <td>
                    {g.known_decisions} / {g.decisions}
                  </td>
                  <td>{number(g.coverage)}</td>
                  <td>
                    <Cases
                      items={g.cases}
                      selector={{
                        kind: selector.kind,
                        key: `${qualityKey}_time:${g.game_id}`,
                      }}
                      onOpen={onOpen}
                      label="Решения партии"
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {quality.time_games.length > limit && (
            <button onClick={() => setLimit(limit + 100)}>
              Ещё 100 партий
            </button>
          )}
        </div>
      </details>
    </details>
  );
}

export function OpeningTemporalDetails({
  temporal,
  uci,
  onOpen,
}: {
  temporal: OpeningTemporal;
  uci: string;
  onOpen: OpenPosition;
}) {
  return (
    <details>
      <summary>Периоды и пути</summary>
      <div className="stats-table-wrap">
        <table aria-label="Изменение сыгранных продолжений">
          <thead>
            <tr>
              <th>Показатель</th>
              <th>Текущий период</th>
              <th>Предыдущий период</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <td>Посещения хода / всех ходов узла</td>
              <td>
                {temporal.current_visits} / {temporal.current_total}
              </td>
              <td>
                {temporal.previous_visits} / {temporal.previous_total}
              </td>
            </tr>
            <tr>
              <td>Доля выбора, %</td>
              <td>{number(temporal.current_share)}</td>
              <td>{number(temporal.previous_share)}</td>
            </tr>
            <tr>
              <td>Изменение доли, п.п.</td>
              <td>{number(temporal.share_delta)}</td>
              <td></td>
            </tr>
            <tr>
              <td>Новые пути, N / % датированных</td>
              <td>
                {temporal.new_paths} / {number(temporal.new_path_percent)}
              </td>
              <td></td>
            </tr>
            <tr>
              <td>Повторные пути, N / % датированных</td>
              <td>
                {temporal.repeated_paths} /{" "}
                {number(temporal.repeated_path_percent)}
              </td>
              <td></td>
            </tr>
            <tr>
              <td>Пути без достоверной даты, N</td>
              <td>{temporal.unknown_paths}</td>
              <td></td>
            </tr>
            <tr>
              <td>Первое сохранённое появление</td>
              <td>{timestamp(temporal.first_seen_ms)}</td>
              <td>
                {temporal.first_seen && (
                  <button
                    onClick={() =>
                      onOpen(
                        temporal.first_seen!.game_id,
                        temporal.first_seen!.ply,
                        temporal.first_seen!.run_id ?? undefined,
                      )
                    }
                  >
                    Открыть
                  </button>
                )}
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <Cases
        items={temporal.previous_cases}
        selector={{ kind: "edge", key: `${uci}::previous` }}
        onOpen={onOpen}
        label="Посещения в предыдущем периоде"
      />
      {temporal.comparison.length > 0 && (
        <div className="stats-table-wrap">
          <table aria-label="Сравнение качества продолжения">
            <thead>
              <tr>
                <th>Показатель</th>
                <th>Δ</th>
                <th>Единицы</th>
                <th>Текущий / предыдущий N</th>
                <th>Состояние</th>
              </tr>
            </thead>
            <tbody>
              {temporal.comparison.map((m) => (
                <tr key={m.id}>
                  <td>{m.name}</td>
                  <td>{number(m.value, 2)}</td>
                  <td>{m.unit}</td>
                  <td>
                    {m.sample} / {m.eligible}
                  </td>
                  <td>{metricStatus(m.status)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </details>
  );
}

export function NodeTablebase({
  observation,
}: {
  observation: TablebaseObservation | null | undefined;
}) {
  if (!observation) return <span>—</span>;
  return (
    <details>
      <summary>
        Syzygy · WDL {number(observation.wdl, 0)} / DTZ{" "}
        {number(observation.dtz, 0)}
      </summary>
      <div className="stats-table-wrap">
        <table aria-label="Табличные данные позиции">
          <tbody>
            <tr>
              <th>WDL, POV белых</th>
              <td>{number(observation.wdl, 0)}</td>
            </tr>
            <tr>
              <th>DTZ, POV белых</th>
              <td>{number(observation.dtz, 0)}</td>
            </tr>
            <tr>
              <th>Правило 50 ходов</th>
              <td>{observation.rule50 ? "Учтено" : "Не учтено"}</td>
            </tr>
            <tr>
              <th>Точность DTZ</th>
              <td>
                {observation.dtz == null
                  ? "—"
                  : observation.rounded
                    ? "Округлён"
                    : "Без округления"}
              </td>
            </tr>
            <tr>
              <th>Состояние</th>
              <td>{metricStatus(observation.status)}</td>
            </tr>
            <tr>
              <th>Набор таблиц</th>
              <td>{observation.manifest_id || "—"}</td>
            </tr>
            <tr>
              <th>Контекст правил</th>
              <td>{observation.context_hash || "—"}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </details>
  );
}
