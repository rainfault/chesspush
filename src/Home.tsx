import { useEffect, useState, type CSSProperties } from "react";
import type { AccountStatus } from "./lichess";
import "./home.css";

/** Only the last 100 points to 2000 were visible in the original Home. */
export function ratingProgress(rating: number | null): number | null {
  if (rating === null || !Number.isFinite(rating) || rating <= 1900)
    return null;
  return Math.min(1, (rating - 1900) / 100);
}

function Rating({
  name,
  symbol,
  rating,
}: {
  name: string;
  symbol: string;
  rating: number | null;
}) {
  const progress = ratingProgress(rating);
  const progressStyle =
    progress === null
      ? undefined
      : ({ "--rating-fill": `${progress * 100}%` } as CSSProperties);
  return (
    <section
      className="home-rating"
      aria-label={`${name}${rating === null ? "" : ` ${rating}`}`}
    >
      <span className="lichess-icon" aria-hidden="true">
        {symbol}
      </span>
      {rating !== null && <span className="home-rating-value">{rating}</span>}
      {progress !== null && (
        <div
          className="home-rating-progress"
          role="progressbar"
          aria-label={`${name}: до 2000`}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(progress * 100)}
          style={progressStyle}
        >
          <span />
        </div>
      )}
    </section>
  );
}

export function Home({
  account,
  onAuthenticate,
  onCancel,
  busy,
}: {
  account: AccountStatus;
  onAuthenticate: () => void;
  onCancel: () => void;
  busy: boolean;
}) {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 1000);
    return () => window.clearInterval(timer);
  }, []);
  const connected = account.status === "connected";
  const authenticating = account.status === "authenticating";
  const error =
    account.status === "error" || account.status === "invalid_token"
      ? account.message
      : null;
  return (
    <main className="home home-page">
      <div className="home-dashboard">
        <time
          className="home-clock"
          dateTime={now.toISOString()}
          aria-label="Текущее время"
        >
          {now.toLocaleTimeString("ru-RU", {
            hour: "2-digit",
            minute: "2-digit",
          })}
        </time>
        <div className="home-ratings">
          <Rating
            name="Rapid"
            symbol={"\ue002"}
            rating={connected ? account.rapid : null}
          />
          <Rating
            name="Blitz"
            symbol={"\ue02f"}
            rating={connected ? account.blitz : null}
          />
        </div>
        {!connected && (
          <div className="home-auth">
            <button
              className="primary"
              onClick={onAuthenticate}
              disabled={busy || authenticating}
            >
              {busy || authenticating
                ? "Ожидание входа…"
                : "Войти через Lichess"}
            </button>
            {authenticating && (
              <button onClick={onCancel} disabled={busy}>
                Отменить вход
              </button>
            )}
          </div>
        )}
        {error && (
          <div className="error home-account-error" role="alert">
            {error}
          </div>
        )}
      </div>
    </main>
  );
}
