// Generate a bounded synthetic ZST input from the frozen parser fixture.
import { createWriteStream } from "node:fs";
import { mkdir, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { createZstdCompress } from "node:zlib";

const project = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repetitions = Number(process.argv[2] ?? 10000);
if (
  !Number.isSafeInteger(repetitions) ||
  repetitions < 1 ||
  repetitions > 1000000
) {
  throw new Error("Usage: node tests/benchmark.mjs [repetitions: 1..1000000]");
}
const fixture = resolve(project, "tests/fixtures/corpus.pgn");
const target = resolve(project, ".qa/benchmark.pgn.zst");
const corpus = await readFile(fixture);
const reference = JSON.parse(
  await readFile(resolve(project, "tests/fixtures/reference.json"), "utf8"),
);
const defaults = reference.cases.find((entry) => entry.name === "defaults");
if (!defaults) throw new Error("Frozen defaults reference is missing");
await mkdir(dirname(target), { recursive: true });
const started = performance.now();
await pipeline(
  Readable.from(
    (function* () {
      for (let i = 0; i < repetitions; i++) yield corpus;
    })(),
  ),
  createZstdCompress(),
  createWriteStream(target),
);
console.log(
  JSON.stringify({
    path: target,
    repetitions,
    expected_scanned: defaults.scanned * repetitions,
    expected_matched: defaults.matches.length * repetitions,
    generation_seconds: (performance.now() - started) / 1000,
  }),
);
