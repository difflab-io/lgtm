// SPA end-to-end: file list rendering, Shiki highlighting on the diff,
// thread creation + reply + resolve + sidebar reflection.

import { expect, test } from '@playwright/test'

import { openUrl, spawnEnv, type Env } from './env'

let env: Env

test.beforeAll(async () => {
  env = await spawnEnv()
})

test.afterAll(async () => {
  await env.teardown()
})

test('file list, diff coloring, and syntax highlighting', async ({ page }) => {
  await page.goto(openUrl(env))

  // One modified file in the fixture.
  const files = page.locator('.lgtm-fileitem')
  await expect(files).toHaveCount(1)
  await expect(files.first()).toContainText('src/lib.rs')
  await expect(files.first()).toContainText('modified')

  // Diff lines must include our added line.
  const added = page.locator('.lgtm-line-added')
  await expect(added).toContainText('pub fn hi() {}')

  // Shiki populates inline-colored spans once highlighting resolves.
  const shikiToken = page.locator('.lgtm-code span[style*="color"]').first()
  await expect(shikiToken).toBeVisible()
})

test('create thread → reply → resolve, with sidebar reflecting state', async ({ page }) => {
  await page.goto(openUrl(env))
  const files = page.locator('.lgtm-fileitem')
  await files.first().click()

  // Click the "+" button on line 2 (the new fn we added).
  const row = page.locator('.lgtm-line', { has: page.locator('.lgtm-ln', { hasText: /^2$/ }) }).first()
  await row.hover()
  await row.locator('.lgtm-addthread').click()

  // Composer should appear; type and submit.
  const composer = page.locator('.lgtm-thread-draft')
  await expect(composer).toBeVisible()
  await composer.locator('textarea').fill('nit: name is confusing')
  await composer.getByRole('button', { name: /^Comment$/ }).click()

  // Inline thread appears below the row.
  const inline = page.locator('.lgtm-inline-thread .lgtm-thread')
  await expect(inline).toBeVisible()
  await expect(inline).toContainText('nit: name is confusing')

  // Sidebar reflects it too.
  const sidebarThreads = page.locator('.lgtm-threads-list .lgtm-thread')
  await expect(sidebarThreads).toHaveCount(1)
  await expect(page.locator('.lgtm-threads-sidebar .lgtm-sidebar-header')).toContainText(
    'Threads · 1',
  )

  // Reply from the sidebar card.
  const sidebarCard = sidebarThreads.first()
  await sidebarCard.locator('textarea').fill('ack, renaming')
  await sidebarCard.getByRole('button', { name: /^Reply$/ }).click()

  await expect(sidebarCard.locator('.lgtm-comment')).toHaveCount(2)

  // Resolve from the inline card.
  await inline.getByRole('button', { name: /^Resolve$/ }).click()
  await expect(inline).toHaveClass(/is-resolved/)

  // "Open only" filter hides it.
  await page.getByLabel(/open only/i).check()
  await expect(sidebarThreads).toHaveCount(0)
})

test('CLI-created thread shows up in the SPA on refresh', async ({ page }) => {
  // Seed via the CLI to prove the agentic path and the UI share one db.
  const added = await env.cli(
    '--as',
    'claude',
    '--as-kind',
    'agent',
    'comment',
    'add',
    env.repoPath,
    '--path',
    'src/lib.rs',
    '--line',
    '1',
    '--base',
    'HEAD',
    '--body',
    'from the CLI',
  )
  expect(added.code).toBe(0)

  await page.goto(openUrl(env))
  await page.locator('.lgtm-fileitem').first().click()

  const inline = page.locator('.lgtm-inline-thread .lgtm-thread', { hasText: /from the CLI/ })
  await expect(inline).toBeVisible()
  await expect(inline.locator('.lgtm-author-agent')).toContainText('agent')
  await expect(inline.locator('.lgtm-author-name')).toContainText('claude')
})
