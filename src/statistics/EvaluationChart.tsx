import type { TrajectoryPoint } from "./analyticsTypes";
import { number } from "./format";
export function EvaluationChart({
  points,
  ply,
  onSelect,
}: {
  points: TrajectoryPoint[];
  ply: number;
  onSelect: (ply: number) => void;
}) {
  if (!points.some((p) => p.q != null))
    return <div className="stats-chart-empty">Нет траектории оценки</div>;
  const max = Math.max(1, ...points.map((p) => p.ply)),
    x = (p: number) => 45 + (p / max) * 700,
    y = (q: number) => 170 - q * 150;
  let path = "",
    gap = true;
  for (const p of points) {
    if (p.q == null) {
      gap = true;
      continue;
    }
    path += (gap ? "M" : "L") + x(p.ply) + " " + y(p.q) + " ";
    gap = false;
  }
  return (
    <div className="stats-chart">
      <svg
        viewBox="0 0 780 205"
        role="img"
        aria-label="Ожидаемое очко модели по ходам"
      >
        {[0, 0.5, 1].map((q) => (
          <g key={q}>
            <line
              x1="45"
              x2="745"
              y1={y(q)}
              y2={y(q)}
              className="stats-chart-grid"
            />
            <text x="35" y={y(q) + 4} textAnchor="end">
              {q * 100}
            </text>
          </g>
        ))}
        <path d={path} stroke="var(--accent)" strokeWidth="2" fill="none" />
        {points
          .filter((p) => p.q != null)
          .map((p) => (
            <circle
              key={p.ply}
              cx={x(p.ply)}
              cy={y(p.q!)}
              r={p.ply === ply ? 6 : 3}
              fill={p.ply === ply ? "var(--success)" : "var(--accent)"}
              tabIndex={0}
              role="button"
              aria-label={"Ply " + p.ply + ": q " + number(p.q! * 100) + "%"}
              onClick={() => onSelect(p.ply)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onSelect(p.ply);
                }
              }}
            >
              <title>
                Ply {p.ply} · q {number(p.q! * 100)}%
              </title>
            </circle>
          ))}
        <text x="45" y="198">
          0 ply
        </text>
        <text x="745" y="198" textAnchor="end">
          {max} ply · q, %
        </text>
      </svg>
    </div>
  );
}
