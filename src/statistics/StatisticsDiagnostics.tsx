import type { Report, StatisticsState } from "./api";
import { metricStatus, timestamp } from "./format";
export function StatisticsDiagnostics({
  state,
  report,
  onRetry,
  onOpen,
}: {
  state: StatisticsState | null;
  report: Report | null;
  onRetry?: (accountId: string, externalId: string) => void;
  onOpen?: (id: string) => void;
}) {
  return (
    <section className="practice-diagnostics">
      <h2>Диагностика данных</h2>
      <details open>
        <summary>Источник и наблюдение</summary>
        <dl>
          {state?.accounts.map((account) => (
            <div key={account.id}>
              <dt>{account.username}</dt>
              <dd>
                Наблюдение с {timestamp(account.t0_ms)} ·{" "}
                {account.enabled ? "подключён" : "отключён"}
              </dd>
            </div>
          ))}
        </dl>
        <p>
          Синхронизация:{" "}
          {state?.sync ? metricStatus(state.sync.status) : "нет данных"} ·
          последнее обновление {timestamp(state?.sync?.last_success_ms)}
        </p>
        <p>
          Ожидают сверки {state?.sync?.pending_games ?? "—"} · незакрытые
          диапазоны {state?.sync?.pending_ranges ?? "—"} · вне фильтра{" "}
          {report?.excluded_games ?? "—"}
        </p>
        {state?.sync?.boundary_uncertain && (
          <p>
            Граница начала наблюдения подтверждена не полностью. Правила
            включения партий сохранены.
          </p>
        )}
        {state?.sync?.error_message && (
          <pre>
            {state.sync.error_code}: {state.sync.error_message}
          </pre>
        )}
      </details>
      <details>
        <summary>Анализ и версии</summary>
        <p>
          {report?.analysed_decisions ?? 0}/{report?.total_decisions ?? 0}{" "}
          решений с анализом · ревизия {report?.evidence_revision ?? "—"}
        </p>
        {report?.profile_ids.map((id) => (
          <p key={id}>
            {report.profile_labels[id] ?? id}
            <br />
            <code>{id}</code>
          </p>
        ))}
        <table aria-label="Техническая очередь анализа">
          <thead>
            <tr>
              <th>Партия</th>
              <th>Профиль</th>
              <th>Состояние</th>
              <th>Попытки</th>
            </tr>
          </thead>
          <tbody>
            {state?.engine.jobs.map((job) => (
              <tr key={job.id}>
                <td>
                  <button
                    className="stats-link"
                    onClick={() => onOpen?.(job.game_id)}
                  >
                    {job.game_id}
                  </button>
                </td>
                <td>{job.profile_id}</td>
                <td>
                  {job.state} {job.last_error}
                </td>
                <td>{job.attempts}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </details>
      <details>
        <summary>
          Отклонённые загрузки ({state?.quarantine.length ?? 0})
        </summary>
        <table aria-label="Отклонённые загрузки">
          <thead>
            <tr>
              <th>Источник</th>
              <th>Получено</th>
              <th>Причина</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {state?.quarantine.map((item) => (
              <tr key={item.account_id + item.external_id}>
                <td>{item.external_id}</td>
                <td>{timestamp(item.received_at_ms)}</td>
                <td>
                  {item.reason}
                  <details>
                    <summary>Исходные данные</summary>
                    <pre>{item.raw_json}</pre>
                  </details>
                </td>
                <td>
                  {onRetry && (
                    <button
                      onClick={() => onRetry(item.account_id, item.external_id)}
                    >
                      Загрузить повторно
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </section>
  );
}
