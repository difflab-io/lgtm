import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  renameSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const temporary = join(root, ".tmp");
const manifestPath = join(temporary, "lgtm-fixtures.json");
const fixtures = [
  {
    language: "rust",
    repo: "rust-lang/regex",
    tag: "1.11.1",
    commit: "9870c06e6c772daaad7ab612faab29130753e41c",
    source: "src/regex/string.rs",
    readme: "README.md",
    before: "        self.is_match_at(haystack, 0)\n",
    after: "        !self.is_match_at(haystack, 0)\n",
    question:
      "Matching is now inverted. What happens to matching and non-matching inputs?",
  },
  {
    language: "go",
    repo: "go-chi/chi",
    tag: "v5.2.1",
    commit: "71307f9b7e4e9527638bc951c42b782cd1560331",
    source: "middleware/timeout.go",
    readme: "README.md",
    before: "w.WriteHeader(http.StatusGatewayTimeout)",
    after: "w.WriteHeader(http.StatusServiceUnavailable)",
    question:
      "Deadline failures now return 503 rather than the documented 504. Is that the intended contract?",
  },
  {
    language: "ts",
    repo: "sindresorhus/type-fest",
    tag: "v4.41.0",
    commit: "68469722a9521122b6cb40376c4c4544aef28228",
    source: "source/is-any.d.ts",
    readme: "readme.md",
    before: "export type IsAny<T> = 0 extends 1 & NoInfer<T> ? true : false;",
    after: "export type IsAny<T> = [T] extends [never] ? true : false;",
    question:
      "The any detector now checks for never. Check any, never, string, and unknown against the existing type tests.",
  },
];

function git(directory, ...args) {
  return execFileSync("git", ["-C", directory, ...args], { encoding: "utf8" });
}

function assertDirectory(path) {
  if (
    existsSync(path) &&
    (!lstatSync(path).isDirectory() || lstatSync(path).isSymbolicLink())
  ) {
    throw new Error(`Expected a real directory, refusing to modify ${path}`);
  }
}

function assertCheckout(fixture, base = temporary) {
  const directory = join(base, fixture.language);
  assertDirectory(directory);
  assertDirectory(join(directory, ".git"));
  if (
    git(directory, "rev-parse", "HEAD").trim() !== fixture.commit ||
    git(directory, "remote", "get-url", "origin").trim() !==
      `https://github.com/${fixture.repo}.git`
  ) {
    throw new Error(
      `${directory} is not the expected pinned checkout; preserve it before creating fresh fixtures.`,
    );
  }
}

// Include staging state and untracked file contents, not just filenames.
// Ignored build outputs and .lgtm review state are disposable on recreation.
function signature(fixture, base = temporary) {
  assertCheckout(fixture, base);
  const directory = join(base, fixture.language);
  const hash = createHash("sha256");
  hash.update(git(directory, "status", "--porcelain=v1", "-z"));
  hash.update(git(directory, "diff", "HEAD", "--binary"));
  hash.update(git(directory, "diff", "--cached", "--binary"));
  for (const path of git(
    directory,
    "ls-files",
    "--others",
    "--exclude-standard",
    "-z",
  )
    .split("\0")
    .filter(Boolean)
    .sort()) {
    hash.update(path + "\0");
    const full = join(directory, path);
    if (lstatSync(full).isSymbolicLink())
      throw new Error(`Preserve untracked symlink before recreating: ${full}`);
    hash.update(readFileSync(full));
  }
  return hash.digest("hex");
}

function loadManifest(base = temporary) {
  const manifest = JSON.parse(
    readFileSync(join(base, "lgtm-fixtures.json"), "utf8"),
  );
  if (
    manifest.version !== 1 ||
    fixtures.some((f) => manifest.fixtures[f.language]?.commit !== f.commit)
  ) {
    throw new Error(
      "Fixture manifest does not match this script; preserve .tmp before creating new fixtures.",
    );
  }
  return manifest;
}

function setup(base = temporary) {
  const manifestPath = join(base, "lgtm-fixtures.json");
  assertDirectory(base);
  mkdirSync(base, { recursive: true });
  if (existsSync(manifestPath)) {
    loadManifest(base);
    for (const fixture of fixtures) assertCheckout(fixture, base);
    console.log(
      "Existing fixtures retained, including your edits and review state.",
    );
    return;
  }
  // Clone all repositories before editing any of them. A partial clone run can
  // resume, but dirty checkouts without our manifest are never adopted.
  for (const fixture of fixtures) {
    const directory = join(base, fixture.language);
    if (!existsSync(directory)) {
      console.log(`Cloning ${fixture.repo} at ${fixture.tag}…`);
      execFileSync(
        "git",
        [
          "-c",
          "advice.detachedHead=false",
          "clone",
          "--quiet",
          "--depth",
          "1",
          "--branch",
          fixture.tag,
          `https://github.com/${fixture.repo}.git`,
          directory,
        ],
        {
          stdio: "inherit",
          timeout: 45000,
          env: { ...process.env, GIT_TERMINAL_PROMPT: "0" },
        },
      );
    }
    assertCheckout(fixture, base);
    if (git(directory, "status", "--porcelain=v1") !== "") {
      throw new Error(
        `Unmanaged dirty checkout at ${directory}; preserve it before continuing.`,
      );
    }
    const source = readFileSync(join(directory, fixture.source), "utf8");
    if (source.split(fixture.before).length !== 2)
      throw new Error(`Expected exactly one edit anchor in ${fixture.source}`);
  }
  const manifest = { version: 1, fixtures: {} };
  for (const fixture of fixtures) {
    const directory = join(base, fixture.language);
    const path = join(directory, fixture.source);
    const source = readFileSync(path, "utf8");
    writeFileSync(path, source.replace(fixture.before, fixture.after));
    git(directory, "add", "--", fixture.source);
    writeFileSync(
      join(directory, fixture.readme),
      readFileSync(join(directory, fixture.readme), "utf8") +
        "\n\n## Local LGTM review fixture\n\nThis checkout has an intentional source regression for testing a local review.\nReview the staged source change, then comment on this unstaged documentation.\n",
    );
    writeFileSync(
      join(directory, "lgtm-notes.md"),
      `# Standalone comment exercise\n\n${fixture.question}\n\nComment here outside a review, then promote the note to a local draft.\nThis file is deliberately untracked: open it directly rather than in the HEAD diff.\n`,
    );
    writeFileSync(
      join(directory, ".git/info/exclude"),
      readFileSync(join(directory, ".git/info/exclude"), "utf8") + "\n.lgtm/\n",
    );
    manifest.fixtures[fixture.language] = {
      repo: fixture.repo,
      commit: fixture.commit,
      source: fixture.source,
      changedLine: source.slice(0, source.indexOf(fixture.before)).split("\n")
        .length,
      signature: signature(fixture, base),
    };
  }
  writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n");
  console.log(
    "Fixtures ready. Source changes are staged; README changes are unstaged; lgtm-notes.md is untracked.",
  );
}

function recreate() {
  assertDirectory(temporary);
  if (!existsSync(manifestPath)) {
    if (fixtures.some((f) => existsSync(join(temporary, f.language)))) {
      throw new Error(
        "No fixture manifest: refusing to remove existing checkouts. Run fixtures:setup to adopt clean pinned clones.",
      );
    }
    setup();
    return;
  }
  const manifest = loadManifest();
  // Validate every checkout before removing any. Unexpected source edits,
  // staging changes, untracked files, commits, or remotes block recreation.
  for (const fixture of fixtures) {
    if (signature(fixture) !== manifest.fixtures[fixture.language].signature) {
      throw new Error(
        `Additional edits detected in .tmp/${fixture.language}; preserve them (for example, move the whole .tmp directory aside) before recreating.`,
      );
    }
  }
  const next = mkdtempSync(join(temporary, ".lgtm-recreate-"));
  const replaced = [];
  try {
    // Local clones copy the pinned committed tree, ignoring working edits.
    // Independent object files keep the replacements valid after old clones go.
    for (const fixture of fixtures) {
      const directory = join(next, fixture.language);
      execFileSync(
        "git",
        [
          "-c",
          "advice.detachedHead=false",
          "clone",
          "--quiet",
          "--local",
          "--no-hardlinks",
          join(temporary, fixture.language),
          directory,
        ],
        { stdio: "inherit" },
      );
      git(
        directory,
        "remote",
        "set-url",
        "origin",
        `https://github.com/${fixture.repo}.git`,
      );
    }
    setup(next);
    for (const fixture of fixtures) {
      if (
        signature(fixture) !== manifest.fixtures[fixture.language].signature
      ) {
        throw new Error(
          `.tmp/${fixture.language} changed during recreation; originals retained.`,
        );
      }
    }
    for (const fixture of fixtures) {
      const original = join(temporary, fixture.language);
      const backup = join(next, "previous-" + fixture.language);
      renameSync(original, backup);
      replaced.push(fixture);
      renameSync(join(next, fixture.language), original);
    }
    // If writing the new manifest fails, restore both checkouts and manifest.
    writeFileSync(manifestPath, readFileSync(join(next, "lgtm-fixtures.json")));
  } catch (error) {
    for (const fixture of replaced.reverse()) {
      const original = join(temporary, fixture.language);
      if (existsSync(original)) rmSync(original, { recursive: true });
      renameSync(join(next, "previous-" + fixture.language), original);
    }
    if (replaced.length)
      writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n");
    throw error;
  } finally {
    rmSync(next, { recursive: true });
  }
}

function status() {
  const manifest = loadManifest();
  for (const fixture of fixtures) {
    assertCheckout(fixture);
    console.log(
      `\n${fixture.language}: ${fixture.repo} @ ${fixture.commit}\nReview ${fixture.source}:${manifest.fixtures[fixture.language].changedLine}`,
    );
    console.log(
      git(join(temporary, fixture.language), "status", "--short").trim(),
    );
    console.log(
      git(join(temporary, fixture.language), "diff", "HEAD", "--stat").trim(),
    );
  }
}

try {
  const action = process.argv[2] || "setup";
  if (action === "setup") setup();
  else if (action === "recreate") recreate();
  else if (action === "status") status();
  else
    throw new Error(
      "Usage: node scripts/review-fixtures.mjs setup|recreate|status",
    );
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
