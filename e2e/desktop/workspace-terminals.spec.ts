import { join } from 'node:path'
import { writeFile } from 'node:fs/promises'
import { expect, test, createGitRepo } from './fixtures'

const TINY_PNG_BASE64 =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='

async function writeToTerminal(
  page: import('@playwright/test').Page,
  sessionId: string,
  text: string,
) {
  const bytes = Array.from(new TextEncoder().encode(text))
  await expect
    .poll(
      async () => {
        try {
          await page.evaluate(
            async ({ sid, data }) => {
              const win = window as unknown as {
                workspaceTestInvoke: (command: string, args: unknown) => Promise<unknown>
              }
              await win.workspaceTestInvoke('terminal_write', { sessionId: sid, data })
            },
            { sid: sessionId, data: bytes },
          )
          return true
        } catch {
          return false
        }
      },
      { timeout: 10000, intervals: [150, 300, 500] },
    )
    .toBe(true)
}

async function openRightDock(page: import('@playwright/test').Page) {
  const toggle = page.getByTestId('right-dock-toggle-terminal')
  await expect(toggle).toBeVisible()
  if ((await toggle.getAttribute('aria-pressed')) !== 'true') {
    await toggle.click()
  }
  await expect(page.getByTestId('terminal-pane')).toBeVisible()
  await expect(page.getByTestId('terminal-tab')).toHaveCount(1, { timeout: 15000 })
  await expect(page.getByTestId('terminal-surface')).toHaveCount(1, { timeout: 15000 })
  await expect(page.getByTestId('terminal-surface')).toHaveAttribute(
    'data-terminal-ready',
    'true',
    {
      timeout: 15000,
    },
  )
}

test.describe('Given a workspace with integrated terminals', () => {
  test('should open right-dock multi-tab terminals, enforce single mounted surface, and resize dock width', async ({
    page,
  }, testInfo) => {
    // Arrange
    const consoleErrors: string[] = []
    page.on('pageerror', (err) => consoleErrors.push(err.message))
    await openRightDock(page)

    // Act
    await page.getByTestId('terminal-new-button').click()
    await expect(page.getByTestId('terminal-tab')).toHaveCount(2, { timeout: 15000 })

    const dockPane = page.getByTestId('right-dock-pane')
    const initialBox = await dockPane.boundingBox()
    expect(initialBox).not.toBeNull()

    const resizer = page.getByTestId('right-dock-resizer')
    const resizerBox = await resizer.boundingBox()
    expect(resizerBox).not.toBeNull()
    await page.mouse.move(resizerBox!.x + resizerBox!.width / 2, resizerBox!.y + 100)
    await page.mouse.down()
    await page.mouse.move(resizerBox!.x - 80, resizerBox!.y + 100, { steps: 5 })
    await page.mouse.up()

    // Assert
    const resizedBox = await dockPane.boundingBox()
    expect(resizedBox).not.toBeNull()
    expect(resizedBox!.width).toBeGreaterThan(initialBox!.width + 40)
    await expect(page.getByTestId('terminal-surface')).toHaveCount(1)

    const surfaceCount = await page.getByTestId('terminal-surface').count()
    await testInfo.attach('01-right-dock-terminals.png', {
      body: await page.screenshot(),
      contentType: 'image/png',
    })
    await testInfo.attach('dom-invariants.json', {
      body: Buffer.from(
        JSON.stringify(
          {
            terminalSurfaceCount: surfaceCount,
            initialRightDockWidth: initialBox!.width,
            resizedRightDockWidth: resizedBox!.width,
            consoleErrors,
          },
          null,
          2,
        ),
        'utf8',
      ),
      contentType: 'application/json',
    })
    expect(consoleErrors).toEqual([])
  })

  test('should navigate into right-dock with Ctrl+w, cycle terminal tabs with tab shortcuts, and persist dock state on reload', async ({
    page,
  }, testInfo) => {
    // Arrange
    await openRightDock(page)
    const firstTabId = await page
      .locator('[data-testid="terminal-tab"][aria-selected="true"]')
      .getAttribute('data-terminal-tab-id')
    await page.getByTestId('terminal-new-button').click()
    await expect(page.getByTestId('terminal-tab')).toHaveCount(2, { timeout: 15000 })
    await expect
      .poll(() =>
        page
          .locator('[data-testid="terminal-tab"][aria-selected="true"]')
          .getAttribute('data-terminal-tab-id'),
      )
      .not.toBe(firstTabId)
    const secondTabId = await page
      .locator('[data-testid="terminal-tab"][aria-selected="true"]')
      .getAttribute('data-terminal-tab-id')
    expect(secondTabId).toBeTruthy()

    // Act: focus right-dock via Ctrl+w l, ensure normal mode, then cycle terminal tab via Space b n
    await page.keyboard.press('Control+w')
    await page.keyboard.press('l')
    await expect(page.getByTestId('right-dock-pane')).toHaveAttribute('data-pane-active', 'true')
    await page.keyboard.press('Escape')

    await page.keyboard.press('Space')
    await page.keyboard.press('b')
    await page.keyboard.press('n')

    // Assert: active terminal tab changed
    await expect
      .poll(() =>
        page
          .locator('[data-testid="terminal-tab"][aria-selected="true"]')
          .getAttribute('data-terminal-tab-id'),
      )
      .not.toBe(secondTabId)

    await testInfo.attach('02-ctrl-w-and-tab-nav.png', {
      body: await page.screenshot(),
      contentType: 'image/png',
    })

    // Act: reload page and confirm right-dock open state persisted in session
    await page.reload()
    await expect(page.getByTestId('right-dock-toggle-terminal')).toHaveAttribute(
      'aria-pressed',
      'true',
      { timeout: 15000 },
    )
    await expect(page.getByTestId('terminal-pane')).toBeVisible()
    await expect(page.getByTestId('terminal-surface')).toHaveCount(1)
  })

  test('should detect OSC 7501 agents in AgentsPane, show worktree dots and background toast, and jump to agent clearing blue dot', async ({
    page,
    workspace,
  }, testInfo) => {
    // Arrange: create two Git repos and add both to the session
    const repoBase = await workspace.createExternalDir()
    const repoDir = await createGitRepo(join(repoBase, 'alpha-repo'), {
      'README.md': '# Alpha\n',
    })
    const worktreeDir = await createGitRepo(join(repoBase, 'beta-repo'), {
      'README.md': '# Beta\n',
    })

    workspace.pickedFolder = repoDir
    await page.getByRole('button', { name: 'Projects view', exact: true }).click()
    await page.getByRole('button', { name: 'Add project', exact: true }).click()
    await expect(page.locator(`[data-tree-node-value=${JSON.stringify(repoDir)}]`)).toBeVisible({
      timeout: 15000,
    })

    workspace.pickedFolder = worktreeDir
    await page.getByRole('button', { name: 'Add project', exact: true }).click()
    await expect(page.locator(`[data-tree-node-value=${JSON.stringify(worktreeDir)}]`)).toBeVisible(
      { timeout: 15000 },
    )

    // Verify AgentsPane is mounted inside #projects-dock-pane
    const agentsInsideProjects = await page
      .locator('#projects-dock-pane [data-testid="agents-dock-pane"]')
      .count()
    expect(agentsInsideProjects).toBe(1)

    // Open terminal in worktreeDir
    await page.locator(`[data-tree-node-value=${JSON.stringify(worktreeDir)}]`).click()
    await openRightDock(page)
    const wtSessionId = await page
      .getByTestId('terminal-surface')
      .getAttribute('data-terminal-session-id')
    expect(wtSessionId).toBeTruthy()

    // Emit OSC 7501 working event and wait on a trigger file before emitting background done
    const doneTrigger = join(workspace.lgtmHomeDir, 'agent-done.trigger')
    await writeToTerminal(
      page,
      wtSessionId!,
      `printf '\\033]7501;app=custom-agent;state=working;msg=Analyzing\\007'; (while [ ! -f ${JSON.stringify(doneTrigger)} ]; do sleep 0.05; done; printf '\\033]7501;app=custom-agent;state=done;msg=Ready\\007') &\n`,
    )
    const agentRow = page.locator(
      `[data-testid="agent-row"][data-agent-session-id=${JSON.stringify(wtSessionId)}]`,
    )
    await expect(agentRow).toBeVisible({ timeout: 15000 })
    await expect(agentRow).toHaveAttribute('data-agent-app-id', 'custom-agent')
    await expect(agentRow).toHaveAttribute('data-agent-status', 'working')

    // Switch back to repoDir so worktreeDir is in the background before triggering done
    await page.locator(`[data-tree-node-value=${JSON.stringify(repoDir)}]`).click()
    await expect
      .poll(() => page.getByTestId('terminal-surface').getAttribute('data-terminal-session-id'))
      .not.toBe(wtSessionId)
    const worktreeDot = page.locator(
      `[data-testid="workspace-status-dot"][data-workspace-path=${JSON.stringify(worktreeDir)}]`,
    )
    await expect(worktreeDot).toHaveAttribute('data-status', 'working', { timeout: 15000 })

    // Trigger the background agent completion now that worktreeDir is unfocused
    await writeFile(doneTrigger, 'done\n', 'utf8')

    // Assert: worktree dot flips to blue ('done') and toast notification appears
    await expect(worktreeDot).toHaveAttribute('data-status', 'done', { timeout: 15000 })
    await expect(agentRow).toHaveAttribute('data-agent-status', 'done')
    await expect(page.getByText(/custom-agent finished/i)).toBeVisible({ timeout: 10000 })

    await testInfo.attach('03-agents-and-worktree-dots.png', {
      body: await page.screenshot(),
      contentType: 'image/png',
    })

    // Act: click the agent row in AgentsPane to jump to worktreeDir + clear blue dot -> green
    await agentRow.click()

    // Assert: dot transitions from done -> idle and active surface attaches to wtSessionId
    await expect(page.getByTestId('terminal-surface')).toHaveAttribute(
      'data-terminal-session-id',
      wtSessionId!,
      { timeout: 15000 },
    )
    await expect(worktreeDot).toHaveAttribute('data-status', 'idle', { timeout: 15000 })

    await testInfo.attach('04-jump-to-agent-scrollback.png', {
      body: await page.screenshot(),
      contentType: 'image/png',
    })
  })

  test('should rewrite Kitty file-based graphics sequences and render inline images without errors', async ({
    page,
    workspace,
  }, testInfo) => {
    // Arrange
    const consoleErrors: string[] = []
    page.on('pageerror', (err) => consoleErrors.push(err.message))
    await openRightDock(page)
    const sessionId = await page
      .getByTestId('terminal-surface')
      .getAttribute('data-terminal-session-id')
    expect(sessionId).toBeTruthy()

    const pngPath = join(workspace.lgtmHomeDir, 'tiny.png')
    await writeFile(pngPath, Buffer.from(TINY_PNG_BASE64, 'base64'))
    const encodedPath = Buffer.from(pngPath, 'utf8').toString('base64')

    // Act: emit Kitty Graphics APC t=f (file transmission) to exercise Rust rewriter -> @wterm/ghostty
    await writeToTerminal(
      page,
      sessionId!,
      `printf 'KITTY_BEFORE\\n\\033_Ga=T,f=100,t=f;${encodedPath}\\033\\\\KITTY_AFTER\\n'\n`,
    )
    await page.waitForTimeout(500)

    // Assert: surface remains healthy with zero uncaught page errors
    await expect(page.getByTestId('terminal-surface')).toHaveCount(1)
    expect(consoleErrors).toEqual([])

    await testInfo.attach('05-kitty-inline-image.png', {
      body: await page.screenshot(),
      contentType: 'image/png',
    })
  })
})
