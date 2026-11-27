#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import process from "node:process";
import { generateReleaseNotes, latestTag } from "./release-notes.mjs";

const ROOT = resolve(import.meta.dirname, "..");
const PACKAGE_JSON_PATH = resolve(ROOT, "package.json");
const TAURI_CONF_PATH = resolve(ROOT, "src-tauri/tauri.conf.json");
const CARGO_TOML_PATH = resolve(ROOT, "src-tauri/Cargo.toml");
const CARGO_MANIFEST_PATH = "src-tauri/Cargo.toml";

const SEMVER_RE =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/;

function run(cmd, args, options = {}) {
  return execFileSync(cmd, args, {
    cwd: ROOT,
    stdio: options.capture ? "pipe" : "inherit",
    encoding: "utf8",
    ...options,
  });
}

function parseArgs(argv) {
  let target = null;
  let dryRun = false;
  let noPush = false;
  let draft = false;

  for (const arg of argv) {
    if (arg === "--dry-run") {
      dryRun = true;
    } else if (arg === "--no-push") {
      noPush = true;
    } else if (arg === "--draft") {
      draft = true;
    } else if (arg === "-h" || arg === "--help") {
      printUsage(0);
    } else if (!arg.startsWith("-") && !target) {
      target = arg;
    } else {
      console.error(`Unknown argument: ${arg}`);
      printUsage(1);
    }
  }

  if (!target) {
    printUsage(1);
  }

  return { target, dryRun, noPush, draft };
}

function printUsage(code) {
  console.log(
    [
      "Usage: pnpm release <version|patch|minor|major> [--dry-run] [--no-push] [--draft]",
      "",
      "Examples:",
      "  pnpm release 0.1.0",
      "  pnpm release patch",
      "  pnpm release 0.2.0 --dry-run",
      "  pnpm release patch --draft",
      "",
      "Requires the GitHub CLI (gh), authenticated with push access.",
    ].join("\n"),
  );
  process.exit(code);
}

function resolveNextVersion(currentVersion, target) {
  const match = SEMVER_RE.exec(currentVersion);
  if (!match) {
    throw new Error(`Current package.json version is not valid semver: ${currentVersion}`);
  }
  const major = Number(match[1]);
  const minor = Number(match[2]);
  const patch = Number(match[3]);

  if (target === "patch") return `${major}.${minor}.${patch + 1}`;
  if (target === "minor") return `${major}.${minor + 1}.0`;
  if (target === "major") return `${major + 1}.0.0`;

  const cleaned = target.startsWith("v") ? target.slice(1) : target;
  if (!SEMVER_RE.test(cleaned)) {
    throw new Error(`Invalid semver version: ${target}`);
  }
  return cleaned;
}

function updateJsonVersion(filePath, nextVersion, dryRun) {
  const raw = readFileSync(filePath, "utf8");
  const data = JSON.parse(raw);
  const changed = data.version !== nextVersion;
  if (changed && !dryRun) {
    data.version = nextVersion;
    writeFileSync(filePath, `${JSON.stringify(data, null, 2)}\n`, "utf8");
  }
  return changed;
}

function updateCargoTomlVersion(filePath, nextVersion, dryRun) {
  const raw = readFileSync(filePath, "utf8");
  const lines = raw.split("\n");
  let inPackage = false;
  let updated = false;
  let changed = false;

  const nextLines = lines.map((line) => {
    const trimmed = line.trim();
    if (trimmed.startsWith("[")) {
      inPackage = trimmed === "[package]";
    } else if (inPackage && /^version\s*=\s*"[^"]+"/.test(trimmed)) {
      updated = true;
      const replacement = `version = "${nextVersion}"`;
      if (trimmed !== replacement) {
        changed = true;
        return line.replace(/^(\s*)version\s*=\s*"[^"]+"/, `$1${replacement}`);
      }
    }
    return line;
  });

  if (!updated) {
    throw new Error(`Could not find [package] version in ${filePath}`);
  }

  if (changed && !dryRun) {
    writeFileSync(filePath, nextLines.join("\n"), "utf8");
  }
  return changed;
}

function ensureCleanWorkingTree() {
  const status = run("git", ["status", "--porcelain"], { capture: true }).trim();
  if (status) {
    throw new Error(
      `Working tree is not clean. Commit or stash changes first:\n${status}`,
    );
  }
}

function ensureTagDoesNotExist(tag) {
  const localTag = run("git", ["tag", "--list", tag], { capture: true }).trim();
  if (localTag) {
    throw new Error(`Git tag ${tag} already exists locally.`);
  }
  const remoteTag = run("git", ["ls-remote", "--tags", "origin", tag], {
    capture: true,
  }).trim();
  if (remoteTag) {
    throw new Error(`Git tag ${tag} already exists on origin.`);
  }
}

function ensureGhReady() {
  try {
    const branch = run(
      "gh",
      [
        "repo",
        "view",
        "--json",
        "defaultBranchRef",
        "--jq",
        ".defaultBranchRef.name",
      ],
      { capture: true },
    ).trim();
    if (!branch) {
      throw new Error("empty default branch");
    }
    return branch;
  } catch {
    throw new Error(
      "GitHub CLI is required to dispatch the release workflow. Install gh and run `gh auth login`.",
    );
  }
}

function main() {
  const { target, dryRun, noPush, draft } = parseArgs(process.argv.slice(2));
  const pkg = JSON.parse(readFileSync(PACKAGE_JSON_PATH, "utf8"));
  const nextVersion = resolveNextVersion(pkg.version, target);
  const tag = `v${nextVersion}`;

  console.log(`Preparing release ${tag} (current: v${pkg.version})${dryRun ? " [dry-run]" : ""}`);

  if (!dryRun) {
    ensureCleanWorkingTree();
  }
  ensureTagDoesNotExist(tag);
  const dispatchRef = !dryRun && !noPush ? ensureGhReady() : "";

  const previousTag = latestTag();
  const notes = generateReleaseNotes({
    from: previousTag,
    to: "HEAD",
  });
  console.log("\nRelease notes preview:\n");
  console.log(notes);
  console.log("");

  const pkgChanged = updateJsonVersion(PACKAGE_JSON_PATH, nextVersion, dryRun);
  const tauriChanged = updateJsonVersion(TAURI_CONF_PATH, nextVersion, dryRun);
  const cargoChanged = updateCargoTomlVersion(CARGO_TOML_PATH, nextVersion, dryRun);

  console.log(
    [
      `  package.json:           ${pkgChanged ? `updated -> ${nextVersion}` : "already in sync"}`,
      `  src-tauri/tauri.conf.json: ${tauriChanged ? `updated -> ${nextVersion}` : "already in sync"}`,
      `  src-tauri/Cargo.toml:   ${cargoChanged ? `updated -> ${nextVersion}` : "already in sync"}`,
    ].join("\n"),
  );

  if (dryRun) {
    console.log(
      `\nDry run complete. Would tag ${tag} and ${
        noPush ? "skip push" : "push to origin, then dispatch release.yml"
      }${draft ? " as a draft release" : ""}.`,
    );
    return;
  }

  if (cargoChanged) {
    console.log("Syncing src-tauri/Cargo.lock...");
    run("cargo", [
      "update",
      "-p",
      "processhub",
      "--offline",
      "--manifest-path",
      CARGO_MANIFEST_PATH,
    ]);
  }

  console.log("Running verification checks...");
  run("pnpm", ["lint"]);
  run("pnpm", ["format:check"]);
  run("pnpm", ["build"]);
  run("cargo", ["fmt", "--manifest-path", CARGO_MANIFEST_PATH, "--", "--check"]);

  const pendingStatus = run("git", ["status", "--porcelain"], { capture: true }).trim();
  if (pendingStatus) {
    run("git", [
      "add",
      "package.json",
      "src-tauri/tauri.conf.json",
      "src-tauri/Cargo.toml",
      "src-tauri/Cargo.lock",
    ]);
    run("git", ["commit", "-m", `chore(release): ${tag}`]);
  }

  run("git", ["tag", "-a", tag, "-m", `Release ${tag}`]);
  console.log(`Created tag ${tag}.`);

  if (!noPush) {
    const branch = run("git", ["branch", "--show-current"], { capture: true }).trim();
    run("git", ["push", "origin", branch]);
    run("git", ["push", "origin", tag]);
    console.log(`Pushed ${branch} and ${tag} to origin.`);

    const dispatchArgs = [
      "workflow",
      "run",
      "release.yml",
      "--ref",
      dispatchRef,
      "-f",
      `tag=${tag}`,
    ];
    if (draft) {
      dispatchArgs.push("-f", "draft=true");
    }

    try {
      run("gh", dispatchArgs);
    } catch {
      throw new Error(
        `Tag ${tag} was pushed, but dispatching the release workflow failed. Retry with:\n  gh ${dispatchArgs.join(" ")}`,
      );
    }
    console.log(
      `Dispatched the release workflow for ${tag} from ${dispatchRef}${
        draft ? " as a draft" : ""
      }. Follow it with: gh run watch`,
    );
  }
}

try {
  main();
} catch (error) {
  console.error(`\nRelease failed: ${error.message}`);
  process.exit(1);
}
