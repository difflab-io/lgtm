import type { Page } from '@playwright/test'
import { test, expect, createFile } from './fixtures'
import { mkdtemp, readFile, readdir, rm, stat, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { basename, join } from 'node:path'

// Tests -----------------------------------------------------------------------
test.describe('When opening an existing workspace', { tag: ['@desktop', '@file-tree'] }, () => {
  test('User starts unselected with collapsed folders and accessible keyboard navigation', async ({
    page,
  }) => {
    // Arrange: the fixture opens a seeded workspace.
    // Assert
    await expect(page.getByText('No file selected')).toBeVisible()
    await expect(page.locator('[role="treeitem"][data-value$="one.md"]')).toHaveCount(0)
    const drafts = page.getByRole('treeitem', { name: 'Drafts', exact: true })
    const draftsControl = drafts.getByRole('button', { name: 'Drafts', exact: true })
    await draftsControl.focus()
    await page.keyboard.press('ArrowRight')
    await expect(drafts).toHaveAttribute('aria-expanded', 'true')
    await page.keyboard.press('ArrowRight')
    await expect(page.locator('[role="treeitem"][data-value$="one.md"]')).toBeFocused()
    await page.keyboard.press('Enter')
    await expect(page.locator('.cm-content')).toContainText('# One')
    await page.locator('[role="treeitem"][data-value$="same.md"]').click()
    await expect(page.locator('.cm-content')).toContainText('# Markdown')
    await page.locator('[role="treeitem"][data-value$="notes.txt"]').click()
    await expect(page.locator('.cm-content')).toContainText('Plain text note.')
    expect(await draftsControl.evaluate((el) => el.getBoundingClientRect().height)).toBe(24)
  })

  test('User creates and renames inline, cancels new items, and retains names after errors', async ({
    page,
    workspace,
  }) => {
    // Arrange
    await page.getByRole('button', { name: 'New folder', exact: true }).click()
    const name = page.getByRole('textbox', { name: 'Name', exact: true })

    // Act & Assert
    await expect(name).toBeFocused()
    await name.fill('Research')
    await name.press('Enter')
    const research = page.getByRole('treeitem', { name: 'Research', exact: true })
    await expect(research).toHaveAttribute('aria-expanded', 'false')

    await page.getByRole('button', { name: 'New folder', exact: true }).click()
    await name.fill('Research')
    await name.press('Enter')
    await expect(page.getByRole('alert')).toContainText('already exists')
    await expect(name).toHaveValue('Research')
    await expect(name).toHaveAttribute('aria-invalid', 'true')

    await name.fill('More research')
    workspace.failWrites = true
    await name.press('Enter')
    await expect(page.getByRole('alert')).toContainText('Disk unavailable')
    workspace.failWrites = false
    await name.press('Enter')
    await expect(page.getByRole('treeitem', { name: 'More research', exact: true })).toBeVisible()

    await page.getByRole('button', { name: 'New folder', exact: true }).click()
    await name.fill('Cancelled')
    await name.press('Escape')
    expect(await readdir(workspace.root)).not.toContain('Cancelled')

    await research.getByRole('button', { name: 'Research', exact: true }).focus()
    await page.keyboard.press('F2')
    await name.fill('Renamed research')
    await name.press('Enter')
    await expect(
      page.getByRole('treeitem', { name: 'Renamed research', exact: true }),
    ).toBeVisible()
  })

  test('User can manage nested files and folders through context menus', async ({
    page,
    workspace,
  }) => {
    // Arrange
    const drafts = page.getByRole('treeitem', { name: 'Drafts', exact: true })

    // Act
    await drafts.getByRole('button', { name: 'Drafts', exact: true }).click({ button: 'right' })

    // Assert
    await expect
      .poll(async () => (await page.getByRole('menu').boundingBox())?.x ?? 0)
      .toBeGreaterThan(0)
    await page.getByRole('menuitem', { name: 'New file', exact: true }).click()
    await expect(page.locator('.cm-content')).toContainText('# Untitled')
    expect(await readFile(join(workspace.root, 'Drafts/Untitled.md'), 'utf8')).toBe(
      '# Untitled\n\n',
    )

    const name = page.getByRole('textbox', { name: 'Name', exact: true })
    const file = page.locator('[role="treeitem"][data-value="Drafts/Untitled.md"]')
    await file.click({ button: 'right' })
    await page.getByRole('menuitem', { name: 'Rename', exact: false }).click()
    await name.fill('Renamed.md')
    await name.press('Enter')
    await expect(page.locator('.document-path')).toHaveAttribute('title', 'Drafts/Renamed.md')

    await drafts.getByRole('button', { name: 'Drafts', exact: true }).click({ button: 'right' })
    await page.getByRole('menuitem', { name: 'Move to Trash', exact: true }).click()
    await expect(drafts).toHaveCount(0)
    await expect(page.getByText('No file selected')).toBeVisible()
  })

  test('User can toggle the sidebar while retaining the selected file', async ({ page }) => {
    // Arrange
    await page.locator('[role="treeitem"][data-value="same.md"]').click()
    await expect(page.locator('.cm-content')).toContainText('# Markdown')
    const before = await page.locator('.document-pane').boundingBox()

    // Act
    await page.getByRole('button', { name: 'Toggle Sidebar', exact: true }).click()

    // Assert
    await expect.poll(async () => (await page.locator('.document-pane').boundingBox())?.x).toBe(0)
    await expect(page.locator('.cm-content')).toContainText('# Markdown')

    // Act
    await page.getByRole('button', { name: 'Toggle Sidebar', exact: true }).click()

    // Assert
    await expect
      .poll(async () => (await page.locator('.document-pane').boundingBox())?.x)
      .toBe(before!.x)
    await expect(page.locator('.cm-content')).toContainText('# Markdown')
  })

  test(
    'User can open any directory on disk and switch between workspaces via the titlebar workspace chooser',
    { tag: '@smoke' },
    async ({ page, workspace }) => {
      // Arrange
      const externalDir = await mkdtemp(join(tmpdir(), 'lgtm-external-repo-'))
      try {
        await writeFile(join(externalDir, 'Cargo.toml'), '[package]\nname = "demo"\n')
        workspace.pickedFolder = externalDir
        const chooser = page
          .locator('.titlebar')
          .getByRole('combobox', { name: 'Workspace selector', exact: true })

        // Act: open the external directory via "Open folder…"
        await chooser.click()
        await page.getByRole('option', { name: 'Open folder…', exact: true }).click()

        // Assert: tree updates to the external directory's files and selector shows its name
        await expect(entry(page, 'Cargo.toml')).toBeVisible()
        await expect(chooser).toContainText(basename(externalDir))
        await entry(page, 'Cargo.toml').click()
        await expect(page.locator('.cm-content')).toContainText('name = "demo"')

        // Act: switch back to the initial Local workspace
        await chooser.click()
        await page.getByRole('option', { name: 'Local workspace', exact: true }).click()

        // Assert: initial workspace entries are restored
        await expect(entry(page, 'same.md')).toBeVisible()
        await expect(page.getByText('No file selected')).toBeVisible()
      } finally {
        await rm(externalDir, { recursive: true, force: true })
      }
    },
  )
})

for (const initialWorkspace of ['empty', 'existing'] as const) {
  const context =
    initialWorkspace === 'empty'
      ? 'When using the app for the first time with an empty workspace'
      : 'When working in an existing local workspace'

  test.describe(context, { tag: ['@desktop', '@file-tree'] }, () => {
    test.use({ initialWorkspace })

    test(
      'User sees the local tree and an unselected file placeholder',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const tree = page.getByRole('tree')

        // Assert
        await expect(page.getByText('No file selected')).toBeVisible()
        await expect(page.locator('.cm-editor')).toHaveCount(0)
        if (initialWorkspace === 'empty') {
          await expect(tree.getByRole('treeitem')).toHaveCount(0)
          await expect(page.getByText('Your workspace is empty.', { exact: false })).toBeVisible()
          expect(await readdir(workspace.root)).toEqual([])
        } else {
          await expect(entry(page, 'Drafts')).toHaveAttribute('aria-expanded', 'false')
          await expect(entry(page, 'same.md')).toBeVisible()
          await expect(entry(page, 'Drafts/one.md')).toHaveCount(0)
          // Act
          await entry(page, 'Drafts').getByRole('button', { name: 'Drafts', exact: true }).click()
          // Assert
          await expect(entry(page, 'Drafts/one.md')).toBeVisible()
          await expect(entry(page, 'Drafts/two.md')).toBeVisible()
          expect(await readFile(join(workspace.root, 'Drafts/one.md'), 'utf8')).toContain('# One')
        }
      },
    )

    test(
      'User can create, rename, reopen and delete a file on disk',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const original = join(workspace.root, 'Untitled.md')
        const renamed = join(workspace.root, 'Renamed.md')

        // Act
        await createFile(page)
        // Assert
        await expect(page.locator('.cm-content')).toContainText('# Untitled')
        expect(await readFile(original, 'utf8')).toBe('# Untitled\n\n')
        await expect(page.getByRole('textbox', { name: 'Name', exact: true })).toHaveCount(0)

        // Act
        await renameEntry(page, 'Untitled.md', 'Renamed.md')
        // Assert
        await expect(page.locator('.document-path')).toHaveAttribute('title', 'Renamed.md')
        expect(await exists(original)).toBe(false)
        expect(await readFile(renamed, 'utf8')).toContain('# Untitled')

        // Act
        await deleteEntry(page, 'Renamed.md')
        // Assert
        await expect(entry(page, 'Renamed.md')).toHaveCount(0)
        expect(await exists(renamed)).toBe(false)
        await expect(page.getByText('No file selected')).toBeVisible()
      },
    )

    test('User can create, rename and delete folders on disk', async ({ page, workspace }) => {
      // Arrange
      const original = join(workspace.root, 'Research')
      const renamed = join(workspace.root, 'Reference')

      // Act
      await createFolder(page, 'Research')
      // Assert
      expect((await stat(original)).isDirectory()).toBe(true)
      await expect(entry(page, 'Research')).toHaveAttribute('aria-expanded', 'false')

      // Act
      await renameEntry(page, 'Research', 'Reference')
      // Assert
      await expect.poll(() => exists(original)).toBe(false)
      expect((await stat(renamed)).isDirectory()).toBe(true)

      // Act
      await deleteEntry(page, 'Reference')
      // Assert
      await expect(entry(page, 'Reference')).toHaveCount(0)
      expect(await exists(renamed)).toBe(false)
    })
  })
}

// Fixtures --------------------------------------------------------------------
function entry(page: Page, path: string) {
  return page.locator(`[role="treeitem"][data-value=${JSON.stringify(path)}]`)
}

async function contextAction(page: Page, path: string, action: string) {
  const parts = path.split('/')
  for (let depth = 1; depth < parts.length; depth++) {
    const parent = entry(page, parts.slice(0, depth).join('/'))
    if ((await parent.getAttribute('aria-expanded')) === 'false') {
      await parent.getByRole('button', { name: parts[depth - 1]!, exact: true }).click()
    }
  }
  const node = entry(page, path)
  const control = node.getByRole('button', { name: path.split('/').at(-1)!, exact: true })
  await ((await control.count()) ? control : node).click({ button: 'right' })
  await page.getByRole('menuitem', { name: action, exact: action !== 'Rename' }).click()
}

async function renameEntry(page: Page, path: string, name: string) {
  await contextAction(page, path, 'Rename')
  const input = page.getByRole('textbox', { name: 'Name', exact: true })
  await expect(input).toBeFocused()
  await input.fill(name)
  await input.press('Enter')
  await expect(input).toHaveCount(0)
}

async function createFolder(page: Page, name: string) {
  await page.getByRole('button', { name: 'New folder', exact: true }).click()
  const input = page.getByRole('textbox', { name: 'Name', exact: true })
  await expect(input).toBeFocused()
  await input.fill(name)
  await input.press('Enter')
  await expect(input).toHaveCount(0)
}

async function deleteEntry(page: Page, path: string) {
  await contextAction(page, path, 'Move to Trash')
  await expect(entry(page, path)).toHaveCount(0)
}

async function exists(path: string) {
  return stat(path).then(
    () => true,
    (error: NodeJS.ErrnoException) => {
      if (error.code === 'ENOENT') return false
      throw error
    },
  )
}
