import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, join } from "node:path";

// Optional external install keeps this browser-only prototype dependency-free.
const { chromium } = await import(
  process.env.LGTM_PLAYWRIGHT_MODULE || "playwright"
);
const root = dirname(fileURLToPath(import.meta.url));
const browser = await chromium.launch({
  headless: true,
  ...(process.env.LGTM_BROWSER_CHANNEL
    ? { channel: process.env.LGTM_BROWSER_CHANNEL }
    : {}),
});
const page = await browser.newPage({
  viewport: { width: 1120, height: 640 },
  deviceScaleFactor: 1,
});
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
const base = pathToFileURL(join(root, "index.html")).href;
async function open(hash) {
  await page.goto(`${base}#${hash}`, { waitUntil: "networkidle" });
  await page.reload({ waitUntil: "networkidle" });
  await page.waitForFunction(
    () =>
      typeof DOMPurify !== "undefined" &&
      typeof HSStaticMethods !== "undefined",
  );
}
try {
  await open("start");
  await page.getByRole("radio", { name: /^Unstaged changes/ }).check();
  await page.getByRole("button", { name: "Start local review" }).click();
  await page
    .getByRole("heading", {
      name: "Unstaged changes · feature/standalone-web",
      exact: true,
    })
    .waitFor();
  await page.getByRole("button", { name: "Save review", exact: true }).click();
  assert.equal(await page.locator("input[name=outcome]").count(), 0);
  await page.getByRole("button", { name: "Save locally", exact: true }).click();
  await page
    .locator(".toast")
    .filter({ hasText: "Local review saved" })
    .waitFor();

  await open("start");
  await page.getByRole("radio", { name: /^Local directory/ }).check();
  await page.getByRole("button", { name: "Start local review" }).click();
  await page.getByLabel("Directory path").fill("~/Projects/agent-work");
  await page.getByRole("button", { name: "Open directory review" }).click();
  await page
    .getByRole("heading", {
      name: "Agent plans · ~/Projects/agent-work",
      exact: true,
    })
    .waitFor();
  assert.equal(await page.locator(".tree [data-file]").count(), 3);
  assert.equal(
    await page.locator(".file-top b").textContent(),
    "agent-plans/review-workflow.md",
  );
  assert.ok((await page.locator(".diff").textContent()).includes("Neovim"));
  await page
    .getByRole("button", { name: "＋ File comment", exact: true })
    .click();
  await page.getByLabel("Comment body").fill("Review this ignored agent plan.");
  await page.getByRole("button", { name: "Save note", exact: true }).click();
  assert.equal(
    await page
      .locator(".thread-summary")
      .filter({ hasText: "Review this ignored agent plan." })
      .count(),
    2,
  );
  await page.getByRole("button", { name: "Save review", exact: true }).click();
  assert.equal(await page.locator("input[name=outcome]").count(), 0);
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.getByRole("button", { name: "Close review", exact: true }).click();
  await page
    .getByRole("button", { name: "Save and close review", exact: true })
    .click();
  assert.equal(
    await page
      .getByRole("button", { name: "Save review", exact: true })
      .isDisabled(),
    true,
  );
  assert.equal(
    await page
      .getByRole("button", { name: "＋ File comment", exact: true })
      .isDisabled(),
    true,
  );
  assert.ok(
    (await page.locator(".top-right").textContent()).includes("Closed"),
  );
  assert.equal(
    await page
      .locator(".thread-summary")
      .filter({ hasText: "Review this ignored agent plan." })
      .count(),
    2,
  );
  await open("start");
  await page.getByLabel("Include ignored files").check();
  await page.getByRole("button", { name: "Start local review" }).click();
  await page.locator('[data-file=".plans/review-workflow.md"]').click();
  assert.ok(
    (await page.locator(".diff").textContent()).includes("Review workflow"),
  );
  assert.equal(await page.locator(".tree [data-file]").count(), 7);

  await open("start");
  await page.getByRole("button", { name: "Choose pull request…" }).click();
  await page.getByLabel("Search pull requests").fill("39");
  assert.equal(await page.locator("[data-pr]").count(), 1);
  await page.locator('[data-pr="39"]').click();
  await page
    .getByRole("heading", { name: "#39 · Add file-level review comments" })
    .waitFor();

  await open("start");
  await page.getByRole("button", { name: "Continue session…" }).click();
  await page.getByLabel("Search local sessions").fill("Closed");
  assert.equal(await page.locator("[data-session]").count(), 1);
  await page.locator('[data-session="closed"]').click();
  assert.equal(
    await page
      .getByRole("button", { name: "Save review", exact: true })
      .isDisabled(),
    true,
  );
  await open("local-orphans");
  await page.locator('[data-thread="6"]').click();
  assert.ok(
    (await page.locator(".diff").textContent()).includes("no longer available"),
  );
  await open("local");
  await page.getByLabel("Review author identity").selectOption("agent-codex");
  await page
    .getByRole("button", { name: "＋ File comment", exact: true })
    .click();
  await page.getByLabel("Comment body").fill("Feedback from an agent.");
  await page.getByRole("button", { name: "Save note", exact: true }).click();
  assert.ok(
    (
      await page
        .locator(".thread-summary")
        .filter({ hasText: "Feedback from an agent." })
        .first()
        .textContent()
    ).includes("Agent"),
  );

  await open("review");
  assert.equal(await page.locator(".thread").count(), 2);
  assert.equal(await page.locator(".reply").count(), 0);
  await page.locator('[data-thread="1"]').click();
  assert.equal(await page.locator(".reply").count(), 2);
  await page.getByLabel("Reply to Maya").fill("Add the regression test.");
  await page.getByRole("button", { name: "Reply", exact: true }).click();
  assert.equal(await page.locator(".reply").count(), 3);
  await page.getByRole("button", { name: "Resolve", exact: true }).click();
  assert.equal(await page.locator(".thread").count(), 1);
  await page.getByLabel("Open only").uncheck();
  assert.equal(await page.locator(".thread").count(), 3);
  await page.locator('[data-thread="3"]').click();
  assert.equal(
    await page.locator(".file-top b").textContent(),
    "internal/review/service.go",
  );

  await open("review");
  await page.getByRole("button", { name: "Side by side", exact: true }).click();
  assert.ok((await page.locator(".split-row").count()) > 0);
  await page.getByRole("button", { name: "Stacked", exact: true }).click();
  await page
    .getByRole("button", {
      name: "Expand 38 unchanged lines above",
      exact: false,
    })
    .click();
  assert.ok((await page.locator(".diff-row").count()) > 50);
  await page.getByRole("button", { name: "Collapse unchanged lines" }).click();
  await page
    .getByLabel("Comment on new line 43", { exact: true })
    .click({ force: true });
  await page
    .getByLabel("Comment on new line 45", { exact: true })
    .click({ force: true, modifiers: ["Shift"] });
  await page.getByLabel("Comment body").fill("**Check both sides.**");
  await page.getByRole("button", { name: "Preview", exact: true }).click();
  assert.equal(
    await page.locator("#comment-preview strong").textContent(),
    "Check both sides.",
  );
  await page.getByRole("button", { name: "Write", exact: true }).click();
  await page.getByRole("button", { name: "Add pending comment" }).click();
  assert.ok(
    await page.locator(".thread-summary").filter({ hasText: ":43–45" }).count(),
  );
  await page
    .getByRole("button", { name: "＋ File comment", exact: true })
    .click();
  await page.getByLabel("Comment body").fill("File-wide feedback.");
  await page.getByRole("button", { name: "Add pending comment" }).click();
  assert.equal(
    await page
      .locator(".thread-summary")
      .filter({ hasText: "File-wide feedback." })
      .count(),
    2,
  );
  await page.getByLabel("Viewed", { exact: true }).check();
  assert.ok(
    (await page.locator("#review-heading").textContent()).includes(
      "1 / 6 files viewed",
    ),
  );
  await page.getByLabel("Filter files").fill("anchors_test");
  assert.equal(await page.locator(".tree [data-file]").count(), 1);
  await page.getByRole("button", { name: "Collapse threads" }).click();
  assert.equal(await page.locator("#threads-panel").count(), 0);
  await page.getByRole("button", { name: /^Threads / }).click();
  assert.equal(await page.locator("#threads-panel").count(), 1);
  await page
    .getByRole("button", { name: "Submit review", exact: true })
    .click();
  await page.getByLabel("Request changes", { exact: false }).check();
  await page
    .locator("dialog")
    .getByRole("button", { name: "Submit review", exact: true })
    .click();
  assert.equal(
    await page.locator("#submit-error").textContent(),
    "Add an overall comment before submitting this outcome.",
  );
  await page
    .getByLabel("Overall review comment")
    .fill("Please cover renamed files.");
  await page
    .locator("dialog")
    .getByRole("button", { name: "Submit review", exact: true })
    .click();
  assert.equal(
    await page.locator(".top-right .badge").first().textContent(),
    "Submitted",
  );

  await mkdir(join(root, "snapshots"), { recursive: true });
  const screens = [
    ["start", "start"],
    ["pull-requests", "pulls"],
    ["review-stacked", "review"],
    ["review-side-by-side", "split"],
    ["review-context", "context"],
    ["review-thread", "thread"],
    ["review-file-comment", "file-comment"],
    ["review-threads-collapsed", "collapsed"],
    ["submit-review", "submit"],
    ["local-review", "local"],
    ["save-local-review", "local-save"],
    ["directory-picker", "directory-picker"],
    ["local-directory-review", "local-directory"],
    ["save-directory-review", "local-directory-save"],
    ["local-include-ignored", "local-ignored"],
    ["local-review-closed", "local-closed"],
    ["local-session-picker", "local-sessions"],
    ["local-orphaned-comments", "local-orphans"],
    ["review-agent-identity", "review-agent"],
    ["close-local-review", "local-close-dialog"],
  ];
  for (const [name, hash] of screens) {
    await open(hash);
    if (hash === "local-close-dialog")
      await page
        .getByRole("button", { name: "Close review", exact: true })
        .click();
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth),
      1120,
    );
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollHeight),
      640,
    );
    await page.screenshot({ path: join(root, "snapshots", `${name}.png`) });
  }
  await open("review");
  await page
    .getByLabel("Comment on new line 44", { exact: true })
    .click({ force: true });
  await page
    .getByLabel("Comment body")
    .fill("Should this include comments on the deleted side?");
  await page.screenshot({
    path: join(root, "snapshots", "review-line-comment.png"),
  });
  // Verify the smaller-screen drawer remains accessible and the page fits.
  await page.setViewportSize({ width: 768, height: 720 });
  await open("review");
  assert.equal(
    await page.evaluate(() => document.documentElement.scrollWidth),
    768,
  );
  await page.getByRole("button", { name: "Collapse threads" }).click();
  await page.getByRole("button", { name: /^Threads / }).click();
  assert.equal(await page.locator("#threads-panel").count(), 1);
  assert.deepEqual(errors, []);
  console.log(
    "Interaction checks passed. Captured 21 snapshots at 1120 × 640.",
  );
} finally {
  await browser.close();
}
