// Restores only the disposable profile created by the native runner.
process.env.CHESSPUSH_TRAINING_TEST_SCRIPT ??= "tests/profile-native.mjs";
await import("./check-repertoire-training.mjs");
