// Use the existing disposable native-profile lifecycle.
process.env.CHESSPUSH_TRAINING_TEST_SCRIPT ??=
  "tests/statistics-layout-native.mjs";
await import("./check-repertoire-training.mjs");
