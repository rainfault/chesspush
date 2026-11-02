// Reuse the isolated native-profile lifecycle used by training checks.
process.env.CHESSPUSH_TRAINING_TEST_SCRIPT ??= "tests/knowledge-native.mjs";
await import("./check-repertoire-training.mjs");
