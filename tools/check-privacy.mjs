import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// Only this frozen, public opening dictionary is allowed as a historical SQLite asset.
const publicOpeningDatabase = "fdef99214987a152e0970ec79f570641ec3807d9";
const fixturePath = /^(?:v2\/)?tests\/fixtures\/[^/]+\.(?:pgn|zst)$/i;

export function inspectPrivacy(filePath, contents, privateValues = []) {
  const name = filePath.replaceAll("\\", "/");
  const issues = [];
  const publicDatabase =
    name === "database.db" &&
    createHash("sha1")
      .update(`blob ${contents.length}\0`)
      .update(contents)
      .digest("hex") === publicOpeningDatabase;
  if (
    /^(?:data|outputs?|credentials|\.qa|node_modules|dist|test-results|playwright-report)(?:\/|$)/i.test(
      name,
    ) ||
    /(?:^|\/)src-tauri\/(?:target|gen)\//i.test(name) ||
    /(?:^|\/)__pycache__\//i.test(name) ||
    /\.(?:pyc|pyo|log|lnk|pem|key)$/i.test(name) ||
    (/(?:^|\/)\.env(?:\..+)?$/i.test(name) && !name.endsWith(".env.example")) ||
    /^(?:config\.json|bot_usernames\.txt|output(?:_\d+)?\.ini)$/i.test(name) ||
    /(?:^|\/)(?:credentials|secrets|token)(?:[._-][^/]*)?\.(?:json|toml|ya?ml|txt)$/i.test(
      name,
    )
  ) {
    issues.push("private-or-generated-path");
  }
  if (
    !publicDatabase &&
    (/\.(?:sqlite[^/]*|db(?:-[^/]*)?)$/i.test(name) ||
      (/\.(?:pgn|zst)$/i.test(name) && !fixturePath.test(name)))
  ) {
    issues.push("local-database-or-game-export");
  }
  if (
    !publicDatabase &&
    contents.subarray(0, 16).toString() === "SQLite format 3\0"
  ) {
    issues.push("sqlite-content");
  }
  const text = contents.toString("utf8");
  if (
    /\b(?:lip_[A-Za-z0-9]{16,}|gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|AKIA[A-Z0-9]{16})\b/.test(
      text,
    ) ||
    /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/.test(text)
  ) {
    issues.push("credential");
  }
  if (
    /[A-Z]:[\\/](?:Users|Documents and Settings)[\\/](?!Public(?:[\\/]|\b)|Default(?:[\\/]|\b)|<[^>]+>)[^\\/\s"'<>]+[\\/]/i.test(
      text,
    )
  ) {
    issues.push("personal-home-path");
  }
  for (const entry of privateValues) {
    if (
      entry.value.length >= 4 &&
      text.toLowerCase().includes(entry.value.toLowerCase())
    ) {
      issues.push(entry.category);
    }
  }
  return [...new Set(issues)];
}

export function loadPrivateValues(settingsFile) {
  if (!fs.existsSync(settingsFile)) return [];
  const settings = JSON.parse(fs.readFileSync(settingsFile, "utf8"));
  return [
    ["lichess_username", "personal-account"],
    ["lichess_api_token", "personal-token"],
  ].flatMap(([key, category]) =>
    typeof settings[key] === "string" && settings[key].trim().length >= 4
      ? [{ category, value: settings[key].trim() }]
      : [],
  );
}

export function inspectGitIdentity(identity) {
  const match = identity
    .trim()
    .match(/^([^<>\r\n]+) <([^<>\s]+)> \d+ [+-]\d{4}$/);
  return match &&
    match[1] === "rainfault" &&
    /@(?:users\.)?noreply\.github\.com$/i.test(match[2])
    ? []
    : ["personal-commit-identity"];
}

export function checkRepository(options = {}) {
  const repo = path.resolve(options.repo || process.cwd());
  const privateValues = loadPrivateValues(
    options.settings || path.join(repo, "data/settings.json"),
  );
  const git = (...args) =>
    execFileSync("git", ["-C", repo, ...args], {
      maxBuffer: 256 * 1024 * 1024,
    });
  const entries = new Map();
  const addTree = (tree) => {
    for (const entry of git("ls-tree", "-rz", tree).toString().split("\0")) {
      const match = entry.match(/^\d+ blob ([a-f0-9]+)\t([\s\S]+)$/);
      if (match)
        entries.set(`${match[1]}:${match[2]}`, {
          hash: match[1],
          path: match[2],
        });
    }
  };
  if (options.history) {
    for (const commit of git("rev-list", "--all")
      .toString()
      .trim()
      .split(/\r?\n/)
      .filter(Boolean))
      addTree(commit);
    for (const ref of git(
      "for-each-ref",
      "--format=%(objectname) %(objecttype)",
    )
      .toString()
      .trim()
      .split(/\r?\n/)) {
      const [hash, type] = ref.split(" ");
      if (type === "tree") addTree(hash);
    }
  } else if (options.staged) {
    for (const entry of git("ls-files", "--stage", "-z")
      .toString()
      .split("\0")) {
      const match = entry.match(/^\d+ ([a-f0-9]+) 0\t([\s\S]+)$/);
      if (match)
        entries.set(`${match[1]}:${match[2]}`, {
          hash: match[1],
          path: match[2],
        });
    }
  } else {
    for (const filePath of git("ls-files", "-z")
      .toString()
      .split("\0")
      .filter(Boolean)) {
      const absolutePath = path.join(repo, filePath);
      if (fs.existsSync(absolutePath))
        entries.set(filePath, {
          path: filePath,
          contents: fs.readFileSync(absolutePath),
        });
    }
  }
  const findings = [];
  if (options.staged) {
    for (const kind of ["AUTHOR", "COMMITTER"]) {
      if (
        inspectGitIdentity(git("var", `GIT_${kind}_IDENT`).toString()).length
      ) {
        findings.push({
          categories: [`personal-${kind.toLowerCase()}-identity`],
        });
      }
    }
  }
  const blobs = new Map();
  for (const entry of entries.values()) {
    if (entry.hash && !blobs.has(entry.hash))
      blobs.set(entry.hash, git("cat-file", "blob", entry.hash));
    const contents = entry.contents || blobs.get(entry.hash);
    const categories = inspectPrivacy(entry.path, contents, privateValues);
    if (categories.length)
      findings.push({
        path: entry.path,
        ...(entry.hash ? { blob: entry.hash } : {}),
        categories,
      });
  }
  if (options.history) {
    for (const line of git(
      "log",
      "--all",
      "--format=%H%x09%an%x09%ae%x09%cn%x09%ce",
    )
      .toString()
      .trim()
      .split(/\r?\n/)
      .filter(Boolean)) {
      const [commit, authorName, authorEmail, committerName, committerEmail] =
        line.split("\t");
      if (
        ![authorEmail, committerEmail].every((email) =>
          /@(?:users\.)?noreply\.github\.com$/i.test(email || ""),
        ) ||
        [authorName, committerName].some((name) => name !== "rainfault")
      ) {
        findings.push({ commit, categories: ["personal-commit-identity"] });
      }
    }
    for (const line of git(
      "for-each-ref",
      "refs/tags",
      "--format=%(refname)%09%(taggeremail)",
    )
      .toString()
      .trim()
      .split(/\r?\n/)
      .filter(Boolean)) {
      const [ref, email] = line.split("\t");
      if (email && !/@(?:users\.)?noreply\.github\.com>$/i.test(email))
        findings.push({ ref, categories: ["personal-tag-identity"] });
    }
  }
  return { entriesChecked: entries.size, findings };
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const args = process.argv.slice(2);
  const value = (flag) =>
    args.includes(flag) ? args[args.indexOf(flag) + 1] : undefined;
  try {
    const result = checkRepository({
      repo: value("--repo"),
      settings: value("--settings"),
      history: args.includes("--history"),
      staged: args.includes("--staged"),
    });
    if (result.findings.length) {
      for (const finding of result.findings)
        console.error(
          `${finding.path || finding.ref || finding.commit || "Git identity"}: ${finding.categories.join(", ")}`,
        );
      console.error(
        `Privacy check failed: ${result.findings.length} finding(s). Values are never printed.`,
      );
      process.exitCode = 1;
    } else
      console.log(
        `Privacy check passed: ${result.entriesChecked} file versions checked.`,
      );
  } catch {
    console.error(
      "Privacy check could not read the repository or local settings.",
    );
    process.exitCode = 1;
  }
}
