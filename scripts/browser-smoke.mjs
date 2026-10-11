import ts from "typescript";
import { readFileSync, writeFileSync } from "node:fs";
import { chromium, expect } from "@playwright/test";
import { spawn } from "node:child_process";
import { mkdtemp, rm, mkdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
const dir = await mkdtemp(join(tmpdir(), "lgtm-browser-"));
const server = spawn(
  "./bin/lgtm",
  [
    "serve",
    "--repo",
    ".tmp/go",
    "--db",
    join(dir, "lgtm.db"),
    "--http",
    "127.0.0.1:0",
    "--grpc",
    "127.0.0.1:0",
  ],
  { stdio: ["ignore", "pipe", "pipe"] },
);
let logs = "";
server.stdout.on("data", (data) => (logs += data));
server.stderr.on("data", (data) => (logs += data));
let browser;
try {
  for (let i = 0; !logs.includes("web http://"); i++) {
    if (server.exitCode !== null || i === 100) throw new Error(logs);
    await new Promise((r) => setTimeout(r, 100));
  }
  browser = await chromium.launch({
    headless: true,
    ...(process.env.LGTM_CHROME
      ? { executablePath: process.env.LGTM_CHROME }
      : {}),
  });
  const page = await browser.newPage({
    viewport: { width: 1440, height: 940 },
  });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e));
  page.on("dialog", (dialog) => dialog.accept());
  const { create, toBinary } = await import("@bufbuild/protobuf");
  await mkdir(".tmp/proto-test", { recursive: true });
  const code = ts.transpileModule(
    readFileSync("apps/lgtm-ts-client/gen/lgtm/v1/review_pb.ts", "utf8"),
    {
      compilerOptions: {
        module: ts.ModuleKind.ESNext,
        target: ts.ScriptTarget.ES2022,
      },
    },
  ).outputText;
  writeFileSync(".tmp/proto-test/review_pb.mjs", code);
  const { ListPullRequestsResponseSchema } =
    await import("../.tmp/proto-test/review_pb.mjs");
  await page.route(
    "**/lgtm.v1.ReviewService/ListPullRequests",
    async (route) => {
      const message = create(ListPullRequestsResponseSchema, {
        workspace: { state: {} },
        repository: "go-chi/chi",
        pulls: [{ number: 123, title: "Example open PR", draft: false }],
      });
      await route.fulfill({
        contentType: "application/proto",
        body: Buffer.from(toBinary(ListPullRequestsResponseSchema, message)),
      });
    },
  );
  await page.goto(logs.match(/web (http:\/\/[\d.]+:\d+)/)[1]);
  await expect(
    page.getByRole("heading", { name: "Choose a review" }),
  ).toBeVisible();
  await expect(page.getByLabel("GitHub repository")).toHaveCount(0);
  await expect(page.getByLabel("PR number")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "#123 · Example open PR" }),
  ).toBeVisible();
  await mkdir(".tmp/screenshots", { recursive: true });
  await page.screenshot({ path: ".tmp/screenshots/select-review.png" });
  await page
    .getByRole("button", { name: "Review worktree", exact: true })
    .click();
  await expect(
    page.locator('.file-list [title="middleware/timeout.go"]'),
  ).toBeVisible();
  await page.locator('.file-list [title="middleware/timeout.go"]').click();
  await expect(
    page
      .locator(".diff-row.added")
      .filter({ hasText: "StatusServiceUnavailable" }),
  ).toHaveCount(1);
  await expect(
    page
      .locator(".diff-row.removed")
      .filter({ hasText: "StatusGatewayTimeout" }),
  ).toHaveCount(1);
  await expect(
    page.locator(".diff-row.added code span[style]"),
  ).not.toHaveCount(0);
  await page
    .getByRole("button", { name: "Comment on new line 39", exact: true })
    .click();
  await page
    .getByLabel("Comment body")
    .fill(
      "This changes timeout responses from 504 to 503.\nPlease keep the documented status.",
    );
  await page.getByRole("button", { name: "Save comment", exact: true }).click();
  await expect(page.locator(".threads-list .thread-card")).toHaveCount(1);
  await expect(page.locator(".inline-thread .thread-card")).toHaveCount(1);
  await page
    .locator(".identity")
    .getByRole("button", { name: "Change" })
    .click();
  await page.getByLabel("Author name").fill("Codex");
  await page.getByLabel("Author type").selectOption("agent");
  await page
    .locator(".identity")
    .getByRole("button", { name: "Save", exact: true })
    .click();
  const thread = page.locator(".threads-list .thread-card").first();
  await thread.getByRole("button", { name: "Reply", exact: true }).click();
  await thread
    .getByLabel("Reply body")
    .fill("Confirmed: this also conflicts with the middleware documentation.");
  await thread.getByRole("button", { name: "Post reply", exact: true }).click();
  await expect(thread.locator(".actor-kind.agent")).toBeVisible();
  await thread.getByRole("button", { name: "Resolve", exact: true }).click();
  await expect(thread).toHaveClass(/resolved/);
  await page.getByLabel("open only").check();
  await expect(page.locator(".threads-list .thread-card")).toHaveCount(0);
  await page.getByLabel("open only").uncheck();
  await thread.getByRole("button", { name: "Reopen", exact: true }).click();
  await expect(thread.locator(".thread-status")).toHaveText("open");
  await expect(
    page.getByRole("button", { name: "Finish review", exact: true }),
  ).toBeEnabled();
  await page.screenshot({ path: ".tmp/screenshots/review-gpui-layout.png" });
  await page
    .getByRole("button", { name: "Comment on old line 39", exact: true })
    .click();
  await page.getByLabel("Comment body").fill("Old-side anchor");
  await page.getByRole("button", { name: "Save comment", exact: true }).click();
  await expect(page.locator(".threads-list .thread-card")).toHaveCount(2);
  await page.locator('.file-list [title="lgtm-notes.md"]').click();
  await expect(page.getByTestId("diff-body")).toContainText(
    "Standalone comment exercise",
  );
  await page
    .getByRole("button", { name: "Comment on new line 1", exact: true })
    .click();
  await page.getByLabel("Standalone note").check();
  await page.getByLabel("Comment body").fill("Agent checkout note to promote");
  await page.getByRole("button", { name: "Save comment", exact: true }).click();
  const note = page.locator(".threads-list .thread-card");
  await expect(note).toContainText("Agent checkout note to promote");
  const select = note.getByLabel("Promote comment");
  await select.selectOption(
    await select.locator("option").nth(1).getAttribute("value"),
  );
  await expect(note.locator(".thread-status")).toHaveText("open");
  await page
    .getByRole("button", { name: "Finish review", exact: true })
    .click();
  await page.getByLabel("Review summary").fill("Reviewed locally");
  await page.getByRole("button", { name: "Approve", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Finish review", exact: true }),
  ).toBeDisabled();
  await page.reload();
  await expect(
    page.getByRole("heading", { name: "Choose a review" }),
  ).toBeVisible();
  await page.locator(".recent-reviews").getByRole("button").first().click();
  await expect(
    page.getByRole("button", { name: "Finish review", exact: true }),
  ).toBeDisabled();
  if (errors.length) throw errors[0];
  console.log(
    "Browser: selection screen, three panes, highlighted per-file diffs, both line anchors, inline/sidebar threads, replies, resolution, notes, promotion, approval, and reload passed",
  );
} finally {
  if (browser) await browser.close();
  server.kill("SIGINT");
  if (server.exitCode === null)
    await new Promise((r) => server.once("exit", r));
  await rm(dir, { recursive: true, force: true });
}
