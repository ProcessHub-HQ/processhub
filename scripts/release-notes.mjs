#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import process from "node:process";
import { pathToFileURL } from "node:url";

const DEFAULT_REPO = "ProcessHub-HQ/processhub";

const CATEGORIES = [
  ["feat", "🚀", "Features"],
  ["fix", "🐛", "Bug Fixes"],
  ["perf", "⚡", "Performance"],
  ["refactor", "♻️", "Refactoring"],
  ["docs", "📝", "Documentation"],
  ["test", "🧪", "Tests"],
  ["style", "🎨", "Style"],
  ["build", "📦", "Build"],
  ["ci", "🔧", "Continuous Integration"],
  ["chore", "🧹", "Chores"],
];

const NOISE_PATTERNS = [
  /^chore\(release\):/i,
  /^merge (branch|pull request|tag)/i,
  /^merge remote-tracking branch/i,
  /^auto-?format/i,
  /^wip\b/i,
];

function git(args) {
  return execFileSync("git", args, {
    cwd: process.cwd(),
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

export function latestTag() {
  const tags = git(["tag", "--sort=-v:refname"])
    .split("\n")
    .filter(Boolean);
  return tags[0] ?? "";
}

export function previousTag(tag) {
  const tags = git(["tag", "--sort=-v:refname"])
    .split("\n")
    .filter((item) => /^v\d/.test(item));
  const index = tags.indexOf(tag);
  if (index === -1) {
    return tags.find((item) => item !== tag) ?? "";
  }
  return tags[index + 1] ?? "";
}

function repoSlug() {
  try {
    const remote = git(["remote", "get-url", "origin"]);
    const match = remote.match(
      /github\.com[/:]([^/]+\/[^/]+?)(?:\.git)?$/,
    );
    return match?.[1] ?? DEFAULT_REPO;
  } catch {
    return DEFAULT_REPO;
  }
}

function isNoise(subject) {
  return NOISE_PATTERNS.some((pattern) => pattern.test(subject));
}

function parseSubject(subject) {
  const match = subject.match(/^(\w+)(?:\([^)]*\))?(!)?:\s*(.+)$/);
  if (!match) return ["other", subject];
  return [match[1].toLowerCase(), match[3].trim()];
}

function commitsBetween(from, to) {
  const range = from ? `${from}..${to}` : to;
  const output = git([
    "log",
    range,
    "--pretty=format:%H|%s|%h|%an",
    "--no-merges",
  ]);
  if (!output) return [];
  return output
    .split("\n")
    .filter(Boolean)
    .map((line) => {
      const [hash, subject, short, author] = line.split("|");
      return { hash, subject, short, author };
    })
    .filter((commit) => commit.hash && commit.subject);
}

function initialReleaseNotes(to, repo) {
  return [
    "## 🎉 Initial Release",
    "",
    `First tagged release of ProcessHub — **${to}**.`,
    "",
    `**Release**: https://github.com/${repo}/releases/tag/${to}`,
  ].join("\n");
}

export function generateReleaseNotes({
  from = "",
  to = "HEAD",
  repo = repoSlug(),
} = {}) {
  if (!from) return initialReleaseNotes(to, repo);

  const commits = commitsBetween(from, to);
  if (commits.length === 0) {
    return [
      "## What's Changed",
      "",
      `_No notable changes since ${from}._`,
      "",
      "---",
      "",
      `**Full Changelog**: https://github.com/${repo}/compare/${from}...${to}`,
    ].join("\n");
  }

  const groups = Object.fromEntries(CATEGORIES.map(([type]) => [type, []]));
  groups.other = [];
  const seen = new Set();

  for (const commit of commits) {
    if (isNoise(commit.subject)) continue;
    const [type, message] = parseSubject(commit.subject);
    if (!message) continue;

    const key = `${type}:${message.toLowerCase()}`;
    if (seen.has(key)) continue;
    seen.add(key);

    const bucket = groups[type] ?? groups.other;
    bucket.push(
      `- ${message} — [\`${commit.short}\`](https://github.com/${repo}/commit/${commit.hash})`,
    );
  }

  const body = ["## What's Changed", ""];
  for (const [type, emoji, label] of CATEGORIES) {
    if (groups[type].length > 0) {
      body.push(`### ${emoji} ${label}`, "", ...groups[type], "");
    }
  }
  if (groups.other.length > 0) {
    body.push("### ✨ Other Changes", "", ...groups.other, "");
  }

  body.push(
    "---",
    "",
    `**Full Changelog**: https://github.com/${repo}/compare/${from}...${to}`,
  );

  return body.join("\n");
}

function parseArgs(argv) {
  const options = { from: "", to: "HEAD", tag: "", output: "", repo: "" };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "-h" || arg === "--help") {
      printUsage(0);
    } else if (arg === "--tag") {
      options.tag = argv[++index] ?? "";
    } else if (arg.startsWith("--tag=")) {
      options.tag = arg.slice("--tag=".length);
    } else if (arg === "--from") {
      options.from = argv[++index] ?? "";
    } else if (arg.startsWith("--from=")) {
      options.from = arg.slice("--from=".length);
    } else if (arg === "--to") {
      options.to = argv[++index] ?? "HEAD";
    } else if (arg.startsWith("--to=")) {
      options.to = arg.slice("--to=".length);
    } else if (arg === "--output") {
      options.output = argv[++index] ?? "";
    } else if (arg.startsWith("--output=")) {
      options.output = arg.slice("--output=".length);
    } else if (arg === "--repo") {
      options.repo = argv[++index] ?? "";
    } else if (arg.startsWith("--repo=")) {
      options.repo = arg.slice("--repo=".length);
    } else {
      console.error(`Unknown argument: ${arg}`);
      printUsage(1);
    }
  }
  return options;
}

function printUsage(code) {
  console.log(
    [
      "Usage: node scripts/release-notes.mjs [options]",
      "",
      "Options:",
      "  --tag <vX.Y.Z>   Generate notes from the previous tag to this tag",
      "  --from <ref>     Start ref (default: latest tag)",
      "  --to <ref>       End ref (default: HEAD)",
      "  --repo <owner/repo>",
      "  --output <path>  Write notes to a file instead of stdout",
      "",
      "Examples:",
      "  node scripts/release-notes.mjs",
      "  node scripts/release-notes.mjs --tag v2.3.0",
      "  node scripts/release-notes.mjs --from v2.2.0 --to HEAD",
    ].join("\n"),
  );
  process.exit(code);
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const repo = options.repo || repoSlug();
  let from = options.from;
  let to = options.to;

  if (options.tag) {
    to = options.tag;
    from = previousTag(options.tag);
  } else if (!from) {
    from = latestTag();
  }

  const notes = generateReleaseNotes({ from, to, repo });
  if (options.output) {
    writeFileSync(options.output, `${notes}\n`, "utf8");
  } else {
    process.stdout.write(`${notes}\n`);
  }
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href
) {
  main();
}
