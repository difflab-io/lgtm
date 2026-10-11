import { execFileSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  rmSync,
} from "node:fs";
import { resolve, join } from "node:path";
import { createHash } from "node:crypto";
const root = resolve(import.meta.dirname, "..", ".tmp");
const names = ["premise", "premise-cargo", "readership"];
const manifestPath = join(root, "project-examples.json");
const manifest = existsSync(manifestPath)
  ? JSON.parse(readFileSync(manifestPath, "utf8"))
  : {};
function git(dir, ...args) {
  return execFileSync("git", ["-C", dir, ...args], { encoding: "utf8" });
}
function fingerprint(dir) {
  const hash = createHash("sha256");
  for (const args of [
    ["rev-parse", "HEAD"],
    ["remote", "get-url", "origin"],
    ["diff", "--binary"],
    ["diff", "--cached", "--binary"],
  ])
    hash.update(git(dir, ...args));
  for (const path of git(
    dir,
    "ls-files",
    "--others",
    "--exclude-standard",
    "-z",
  )
    .split("\0")
    .filter(Boolean)
    .sort()) {
    hash.update(path);
    hash.update(readFileSync(join(dir, path)));
  }
  return hash.digest("hex");
}
mkdirSync(root, { recursive: true });
if (process.argv[2] === "recreate") {
  // Check every managed checkout before changing any of them.
  for (const name of names)
    if (
      existsSync(join(root, name)) &&
      (!manifest[name] ||
        fingerprint(join(root, name)) !== manifest[name].fingerprint)
    )
      throw new Error(
        `Preserve additional edits in .tmp/${name} before recreation.`,
      );
  for (const name of names) {
    const dir = join(root, name);
    if (!existsSync(dir)) continue;
    git(dir, "restore", "--staged", "--worktree", ".");
    rmSync(join(dir, "lgtm-example-note.md"), { force: true });
    rmSync(join(dir, ".lgtm"), { recursive: true, force: true });
    delete manifest[name].fingerprint;
  }
}
for (const name of names) {
  const dir = join(root, name);
  if (!existsSync(dir))
    execFileSync("gh", ["repo", "clone", `cloudvoyant/${name}`, dir], {
      stdio: "inherit",
    });
  if (manifest[name]?.fingerprint) {
    console.log(`Keeping .tmp/${name}`);
    continue;
  }
  if (git(dir, "status", "--porcelain").trim())
    throw new Error(`Refusing to edit existing changes in ${dir}`);
  const commit = git(dir, "rev-parse", "HEAD").trim();
  const exclude = join(dir, ".git", "info", "exclude");
  writeFileSync(exclude, readFileSync(exclude, "utf8") + "\n.lgtm/\n");
  const readme = git(dir, "ls-files")
    .split("\n")
    .find((p) => /^readme\.md$/i.test(p));
  if (readme)
    writeFileSync(
      join(dir, readme),
      readFileSync(join(dir, readme), "utf8") +
        "\n## Local review example\n\nDocument the expected behavior before approving this change.\n",
    );
  writeFileSync(
    join(dir, "lgtm-example-note.md"),
    "# Standalone review note\n\nTry commenting here, then promote the note into a local or GitHub draft.\n",
  );
  manifest[name] = {
    repo: `cloudvoyant/${name}`,
    commit,
    fingerprint: fingerprint(dir),
  };
  writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n");
  console.log(`Ready: .tmp/${name} (${commit})`);
}
