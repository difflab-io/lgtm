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
test.describe('When opening an existing workspace', { tag: ['@desktop', '@file-tree'] }, () => {
  test('should start unselected with collapsed folders and navigate entries via keyboard', async ({
    page,
  }) => {
    // Arrange
    const drafts = page.getByRole('treeitem', { name: 'Drafts', exact: true })
    const draftsControl = drafts.getByRole('button', { name: 'Drafts', exact: true })

    // Act
    await draftsControl.focus()
    await page.keyboard.press('ArrowRight')
    await expect(drafts).toHaveAttribute('aria-expanded', 'true')
    await page.keyboard.press('ArrowRight')
    await expect(page.locator('[role="treeitem"][data-value$="one.md"]')).toBeFocused()
    await page.keyboard.press('Enter')

    // Assert
    await expect(page.locator('.cm-content')).toContainText('# One')
    await page.locator('[role="treeitem"][data-value$="same.md"]').click()
    await expect(page.locator('.cm-content')).toContainText('# Markdown')
    await page.locator('[role="treeitem"][data-value$="notes.txt"]').click()
    await expect(page.locator('.cm-content')).toContainText('Plain text note.')
    expect(await draftsControl.evaluate((el) => el.getBoundingClientRect().height)).toBe(24)
  })

  test('should create and rename folders inline, cancel new items, and retain input after errors', async ({
    page,
    workspace,
  }) => {
    // Arrange
    const name = page.getByRole('textbox', { name: 'Name', exact: true })

    // Act
    await createFolder(page, 'Research')
    const research = page.getByRole('treeitem', { name: 'Research', exact: true })

    // Assert
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

  test(
    'should open an external directory and switch between workspaces via the titlebar chooser',
    { tag: '@smoke' },
    async ({ page, workspace }) => {
      // Arrange
      const externalDir = await workspace.createExternalDir()
      await writeFile(join(externalDir, 'Cargo.toml'), '[package]\nname = "demo"\n')
      workspace.pickedFolder = externalDir
      const chooser = page
        .locator('.titlebar')
        .getByRole('combobox', { name: 'Workspace selector', exact: true })

      // Act
      await chooser.click()
      await page.getByRole('option', { name: 'Open folder…', exact: true }).click()

      // Assert
      await expect(entry(page, 'Cargo.toml')).toBeVisible()
      await expect(chooser).toContainText(basename(externalDir))
      await entry(page, 'Cargo.toml').click()
      await expect(page.locator('.cm-content')).toContainText('name = "demo"')

      // Act
      await chooser.click()
      await page.getByRole('option', { name: 'Local workspace', exact: true }).click()

      // Assert
      await expect(entry(page, 'same.md')).toBeVisible()
      await expect(page.getByText('No file selected')).toBeVisible()
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
      'should render the local tree and unselected file placeholder',
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

    test('should create, rename, and delete folders on disk', async ({ page, workspace }) => {
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
