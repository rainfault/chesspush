// Local Lichess protocol fixture. It never proxies requests or uses real credentials.
import { createServer } from "node:http";
export async function statisticsApiFixture() {
  const games = new Map(),
    requests = [];
  let accountReads = 0;
  const account = {
    id: "flowfixture",
    username: "FlowFixture",
    perfs: {
      blitz: { rating: 1500, games: 1 },
      rapid: { rating: 1600, games: 1 },
      bullet: { rating: 1400, games: 1 },
    },
  };
  const definition = [
    ["flow0001", "1. f3 e5 2. g4 Qh4#", "mate", "black"],
    ["flow0002", "1. Nf3 Nf6 2. g3 g6 3. Bg2 Bg7", "draw", null],
    ["flow0003", "1. g3 g6 2. Nf3 Nf6 3. Bg2 Bg7", "draw", null],
  ];
  function publish(index) {
    const [id, moves, status, winner] = definition[index];
    if (games.has(id)) return;
    const result =
      winner === "black" ? "0-1" : winner === "white" ? "1-0" : "1/2-1/2";
    const createdAt = Date.now();
    games.set(id, {
      id,
      createdAt,
      lastMoveAt: createdAt,
      status,
      winner,
      speed: "blitz",
      variant: "standard",
      rated: true,
      clock: { initial: 300, increment: 2 },
      players: {
        white: {
          user: { id: "flowfixture", name: "FlowFixture" },
          rating: 1500,
          ratingDiff: index === 0 ? -5 : 0,
        },
        black: {
          user: { id: "fixtureopponent", name: "FixtureOpponent" },
          rating: 1500,
          ratingDiff: index === 0 ? 5 : 0,
        },
      },
      moves,
      pgn: `[Event "API flow fixture"]\n[Site "https://lichess.org/${id}"]\n[White "FlowFixture"]\n[Black "FixtureOpponent"]\n[Result "${result}"]\n\n${moves} ${result}\n`,
      clocks:
        index === 0
          ? [30000, 30000, 29800, 29900]
          : [30000, 30000, 29800, 29900, 29600, 29700],
    });
  }
  const server = createServer(async (req, res) => {
    const url = new URL(req.url, "http://127.0.0.1");
    requests.push({
      path: url.pathname,
      query: Object.fromEntries(url.searchParams),
      at: Date.now(),
    });
    if (url.pathname === "/_fixture/publish" && req.method === "POST") {
      const index = Number(url.searchParams.get("index"));
      if (index < 0 || index >= definition.length) {
        res.writeHead(400);
        res.end();
        return;
      }
      publish(index);
      res.end("ok");
      return;
    }
    if (req.headers.authorization !== "Bearer synthetic_flow_credential") {
      res.writeHead(401);
      res.end();
      return;
    }
    if (url.pathname === "/api/account") {
      accountReads++;
      res.setHeader("Content-Type", "application/json");
      res.end(JSON.stringify(account));
      return;
    }
    if (url.pathname.startsWith("/api/games/user/")) {
      res.setHeader("Content-Type", "application/x-ndjson");
      const since = Number(url.searchParams.get("since") ?? 0),
        until = Number(
          url.searchParams.get("until") ?? Number.MAX_SAFE_INTEGER,
        );
      const activeOnly = url.searchParams.get("finished") === "false";
      const selected = activeOnly
        ? []
        : [...games.values()]
            .filter((g) => g.createdAt >= since && g.createdAt < until)
            .sort(
              (a, b) => a.createdAt - b.createdAt || a.id.localeCompare(b.id),
            );
      for (const game of selected) res.write(JSON.stringify(game) + "\n");
      res.end();
      return;
    }
    if (url.pathname.startsWith("/game/export/")) {
      const game = games.get(url.pathname.split("/").at(-1));
      if (!game) {
        res.writeHead(404);
        res.end();
        return;
      }
      res.setHeader("Content-Type", "application/json");
      res.end(JSON.stringify(game));
      return;
    }
    res.writeHead(404);
    res.end();
  });
  await new Promise((resolve, reject) =>
    server.once("error", reject).listen(0, "127.0.0.1", resolve),
  );
  return {
    endpoint: `http://127.0.0.1:${server.address().port}`,
    publish,
    requests,
    get accountReads() {
      return accountReads;
    },
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}
