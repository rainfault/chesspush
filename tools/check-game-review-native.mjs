// Reuse the isolated native-profile lifecycle used by training checks.
process.env.CHESSPUSH_TRAINING_TEST_SCRIPT ??= process.argv.includes("--engine")
  ? "tests/review-engine-native.mjs"
  : "tests/game-review-native.mjs";
await import("./check-repertoire-training.mjs");
