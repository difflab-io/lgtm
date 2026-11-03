import {
  test,
  expect,
  entry,
  createFile,
  createFolder,
  renameEntry,
  deleteEntry,
  exists,
} from './fixtures'
import { readFile, readdir, stat, writeFile } from 'node:fs/promises'
import { basename, join } from 'node:path'

// Tests -----------------------------------------------------------------------
test.describe('Given a new workspace', { tag: ['@desktop', '@file-tree'] }, () => {
  test.use({ initialWorkspace: 'empty' })

  test(
    'should create the workspace on disk, remember opened workspaces, and reopen them',
    { tag: '@smoke' },
    async ({ page, workspace }) => {
      // Arrange
      const defaultRoot = workspace.root
      const externalDir = await workspace.createExternalDir()
      await writeFile(join(externalDir, 'Cargo.toml'), '[package]\nname = "demo"\n')
      workspace.pickedFolder = externalDir
      const chooser = page
        .locator('.titlebar')
        .getByRole('combobox', { name: 'Workspace selector', exact: true })

      // Assert
      expect((await stat(defaultRoot)).isDirectory()).toBe(true)
      expect(await readdir(defaultRoot)).toEqual([])
      await expect(page.getByText('Your workspace is empty.', { exact: false })).toBeVisible()
      await expect(page.getByText('No file selected')).toBeVisible()
      await expect(page.locator('.cm-editor')).toHaveCount(0)

      // Act
      await createFile(page)

      // Assert
      await expect(entry(page, 'Untitled.md')).toBeVisible()
      expect(await readFile(join(defaultRoot, 'Untitled.md'), 'utf8')).toBe('# Untitled\n\n')

      // Act
      await chooser.click()
      await page.getByRole('option', { name: 'Open folder…', exact: true }).click()

      // Assert
      await expect(entry(page, 'Cargo.toml')).toBeVisible()
      await expect(chooser).toContainText(basename(externalDir))

      // Act
      await chooser.click()
      await page.getByRole('option', { name: 'Local workspace', exact: true }).click()

      // Assert
      await expect(entry(page, 'Untitled.md')).toBeVisible()

      // Act
      await chooser.click()
      await page.getByRole('option', { name: basename(externalDir), exact: true }).click()

      // Assert
      await expect(entry(page, 'Cargo.toml')).toBeVisible()
      await entry(page, 'Cargo.toml').click()
      await expect(page.locator('.cm-content')).toContainText('name = "demo"')
    },
  )
})

test.describe('Given an existing workspace', { tag: ['@desktop', '@file-tree'] }, () => {
  test.use({ initialWorkspace: 'existing' })

  test(
    'should start unselected with collapsed folders, navigate entries via keyboard, and view and edit files',
    { tag: '@smoke' },
    async ({ page, workspace }) => {
      // Arrange
      const drafts = page.getByRole('treeitem', { name: 'Drafts', exact: true })
      const draftsControl = drafts.getByRole('button', { name: 'Drafts', exact: true })

      // Assert
      await expect(page.getByText('No file selected')).toBeVisible()
      await expect(page.locator('.cm-editor')).toHaveCount(0)
      await expect(drafts).toHaveAttribute('aria-expanded', 'false')
      await expect(entry(page, 'Drafts/one.md')).toHaveCount(0)

      // Act
      await draftsControl.focus()
      await page.keyboard.press('ArrowRight')
      await expect(drafts).toHaveAttribute('aria-expanded', 'true')
      await page.keyboard.press('ArrowRight')
      await expect(page.locator('[role="treeitem"][data-value$="one.md"]')).toBeFocused()
      await page.keyboard.press('Enter')

      // Assert
      await expect(page.locator('.cm-content')).toContainText('# One')
      await expect(page.locator('.cm-content .cm-line span[class]').first()).toBeVisible()
      await expect(page.locator('.document-heading .format-label')).toHaveCount(0)
      expect(await draftsControl.evaluate((el) => el.getBoundingClientRect().height)).toBe(24)

      // Act
      await page.locator('[role="treeitem"][data-value$="same.md"]').click()
      await expect(page.locator('.cm-content')).toContainText('# Markdown')
      await page.locator('.cm-content').click()
      await page.keyboard.type('Persisted across file switch.')
      await page.locator('[role="treeitem"][data-value$="notes.txt"]').click()
      await expect(page.locator('.cm-content')).toContainText('Plain text note.')
      await page.locator('[role="treeitem"][data-value$="same.md"]').click()

      // Assert
      await expect(page.locator('.cm-content')).toContainText('Persisted across file switch.')
      expect(await readFile(join(workspace.root, 'same.md'), 'utf8')).toContain(
        'Persisted across file switch.',
      )
    },
  )

  test(
    'should create, rename, reopen, and delete a file on disk',
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
      await page.locator('.cm-content').click()
      await page.keyboard.type('Saved note line.')
      await renameEntry(page, 'Untitled.md', 'Renamed.md')

      // Assert
      await expect(page.locator('.document-path')).toHaveAttribute('title', 'Renamed.md')
      await expect(page.locator('.cm-content')).toContainText('Saved note line.')
      expect(await exists(original)).toBe(false)
      expect(await readFile(renamed, 'utf8')).toContain('Saved note line.')

      // Act
      await deleteEntry(page, 'Renamed.md')

      // Assert
      await expect(entry(page, 'Renamed.md')).toHaveCount(0)
      expect(await exists(renamed)).toBe(false)
      await expect(page.getByText('No file selected')).toBeVisible()
    },
  )

  test('should create, rename, and delete folders inline, cancel new items, and retain input after errors', async ({
    page,
    workspace,
  }) => {
    // Arrange
    const name = page.getByRole('textbox', { name: 'Name', exact: true })
    const original = join(workspace.root, 'Research')
    const renamed = join(workspace.root, 'Renamed research')

    // Act
    await createFolder(page, 'Research')
    const research = page.getByRole('treeitem', { name: 'Research', exact: true })

    // Assert
    expect((await stat(original)).isDirectory()).toBe(true)
    await expect(research).toHaveAttribute('aria-expanded', 'false')

    // Act
    await page.getByRole('button', { name: 'New folder', exact: true }).click()
    await name.fill('Research')
    await name.press('Enter')

    // Assert
    await expect(page.getByRole('alert')).toContainText('already exists')
    await expect(name).toHaveValue('Research')
    await expect(name).toHaveAttribute('aria-invalid', 'true')

    // Act
    await name.fill('More research')
    workspace.failWrites = true
    await name.press('Enter')
    await expect(page.getByRole('alert')).toContainText('Disk unavailable')
    workspace.failWrites = false
    await name.press('Enter')

    // Assert
    await expect(page.getByRole('treeitem', { name: 'More research', exact: true })).toBeVisible()

    // Act
    await page.getByRole('button', { name: 'New folder', exact: true }).click()
    await name.fill('Cancelled')
    await name.press('Escape')

    // Assert
    expect(await readdir(workspace.root)).not.toContain('Cancelled')

    // Act
    await research.getByRole('button', { name: 'Research', exact: true }).focus()
    await page.keyboard.press('F2')
    await name.fill('Renamed research')
    await name.press('Enter')

    // Assert
    await expect(
      page.getByRole('treeitem', { name: 'Renamed research', exact: true }),
    ).toBeVisible()
    await expect.poll(() => exists(original)).toBe(false)
    expect((await stat(renamed)).isDirectory()).toBe(true)

    // Act
    await deleteEntry(page, 'Renamed research')

    // Assert
    await expect(entry(page, 'Renamed research')).toHaveCount(0)
    expect(await exists(renamed)).toBe(false)
  })

  test('should create, rename, and delete nested entries through context menus', async ({
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

    // Act
    await page.getByRole('menuitem', { name: 'New file', exact: true }).click()

    // Assert
    await expect(page.locator('.cm-content')).toContainText('# Untitled')
    expect(await readFile(join(workspace.root, 'Drafts/Untitled.md'), 'utf8')).toBe(
      '# Untitled\n\n',
    )

    // Act
    await renameEntry(page, 'Drafts/Untitled.md', 'Renamed.md')

    // Assert
    await expect(page.locator('.document-path')).toHaveAttribute('title', 'Drafts/Renamed.md')

    // Act
    await deleteEntry(page, 'Drafts')

    // Assert
    await expect(drafts).toHaveCount(0)
    await expect(page.getByText('No file selected')).toBeVisible()
  })

  test('should toggle the sidebar while retaining the selected file', async ({ page }) => {
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
})
