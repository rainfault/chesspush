export function controlLabel(value: string): string {
  const match = /^(\d+(?:\.\d+)?)\+(\d+(?:\.\d+)?)$/.exec(value);
  if (!match) return value === "unknown" ? "Неизвестен" : value;
  const minutes = Number(match[1]) / 60;
  return `${new Intl.NumberFormat("ru-RU", { maximumFractionDigits: 2 }).format(minutes)}+${match[2]}`;
}
