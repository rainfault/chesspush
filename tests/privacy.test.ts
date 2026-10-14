import { describe, it, expect } from "vitest";
import { inspectPrivacy, inspectGitIdentity } from "../tools/check-privacy.mjs";

const inspect = (
  name: string,
  content: string,
  privateValues: { value: string; category: string }[] = [],
) => inspectPrivacy(name, Buffer.from(content), privateValues);

describe("Privacy guard", () => {
  it("requires a public identity before a commit", () => {
    expect(
      inspectGitIdentity(
        "rainfault <rainfault@users.noreply.github.com> 1234567890 +0300",
      ),
    ).toEqual([]);
    expect(
      inspectGitIdentity(
        "rainfault <123+rainfault@users.noreply.github.com> 1234567890 +0300",
      ),
    ).toEqual([]);
    for (const identity of [
      "rainfault <example@example.org> 1234567890 +0300",
      "Example Person <rainfault@users.noreply.github.com> 1234567890 +0300",
      "malformed identity",
    ])
      expect(inspectGitIdentity(identity)).toEqual([
        "personal-commit-identity",
      ]);
  });
  it("blocks local data and exports, including renamed SQLite files", () => {
    for (const name of [
      "data/settings.json",
      ".qa/privacy-original.bundle",
      "src-tauri/target/chesspush.exe",
      "node_modules/example/config.json",
      "dist/assets/app.js",
      "outputs/a.pgn",
      "output/a.ini",
      "config.json",
      ".env",
      "secrets.json",
      "a.db",
      "a.sqlite3",
      "a.pgn",
      "a.zst",
      "__pycache__/a.pyc",
    ]) {
      expect(inspect(name, "").length, name).toBeGreaterThan(0);
    }
    expect(inspect("accidental-copy.bin", "SQLite format 3\0rest")).toContain(
      "sqlite-content",
    );
  });
  it("permits synthetic parser fixtures and public licensing attribution", () => {
    expect(
      inspect("tests/fixtures/corpus.pgn", '[White "DemoPlayer"]'),
    ).toEqual([]);
    expect(
      inspect("v2/tests/fixtures/corpus.pgn.zst", "compressed sample"),
    ).toEqual([]);
    expect(
      inspect(
        "THIRD_PARTY.md",
        "Copyright Example Maintainer <maintainer@example.org>",
      ),
    ).toEqual([]);
    expect(inspect(".env.example", "LICHESS_TOKEN=")).toEqual([]);
  });
  it("finds token content, exact private settings, and personal home paths", () => {
    expect(
      inspect("src/config.ts", `const leaked = "${"lip_"}${"A".repeat(24)}";`),
    ).toContain("credential");
    expect(
      inspect("notes.md", "C:" + "/Users/" + "PrivatePerson/Documents/a.pgn"),
    ).toContain("personal-home-path");
    expect(
      inspectPrivacy("notes.md", Buffer.from("PrivateAccount"), [
        { value: "PrivateAccount", category: "personal-account" },
      ]),
    ).toContain("personal-account");
  });
});
