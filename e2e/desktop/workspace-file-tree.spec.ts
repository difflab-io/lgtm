import {
  test,
  expect,
  entry,
  createFile,
  createFolder,
  contextAction,
  renameEntry,
  deleteEntry,
  exists,
} from './fixtures'
import { readFile, readdir, stat, writeFile } from 'node:fs/promises'
import { basename, join } from 'node:path'

// Tests -----------------------------------------------------------------------
test.describe('Given a new workspace', { tag: ['@desktop', '@file-tree'] }, () => {
  test.use({ initialWorkspace: 'empty' })

  test.describe('When launching for the first time', () => {
    test(
      'should create the default workspace on disk and render the empty state',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const tree = page.getByRole('tree')

        // Act
        const info = await stat(workspace.root)
        const files = await readdir(workspace.root)

        // Assert
        expect(info.isDirectory()).toBe(true)
        expect(files).toEqual([])
        await expect(tree.getByRole('treeitem')).toHaveCount(0)
        await expect(page.getByText('Your workspace is empty.', { exact: false })).toBeVisible()
        await expect(page.getByText('No file selected')).toBeVisible()
        await expect(page.locator('.cm-editor')).toHaveCount(0)
      },
    )
  })

  test.describe('When opening and switching workspaces', () => {
    test(
      'should remember opened workspaces in the selector and reopen them',
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
})

test.describe('Given an existing workspace', { tag: ['@desktop', '@file-tree'] }, () => {
  test.use({ initialWorkspace: 'existing' })

  test.describe('When browsing the file tree and shell', () => {
    test(
      'should start unselected with collapsed folders and navigate entries via keyboard',
      { tag: '@smoke' },
      async ({ page }) => {
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
        expect(await draftsControl.evaluate((el) => el.getBoundingClientRect().height)).toBe(24)
      },
    )

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

  test.describe('When viewing and editing files', () => {
    test(
      'should render file content with syntax highlighting and persist edits across file switches',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const sameItem = page.locator('[role="treeitem"][data-value$="same.md"]')
        const notesItem = page.locator('[role="treeitem"][data-value$="notes.txt"]')

        // Act
        await sameItem.click()

        // Assert
        await expect(page.locator('.cm-content')).toContainText('# Markdown')
        await expect(page.locator('.cm-content .cm-line span[class]').first()).toBeVisible()
        await expect(page.locator('.document-heading .format-label')).toHaveCount(0)

        // Act
        await page.locator('.cm-content').click()
        await page.keyboard.type('Persisted across file switch.')
        await notesItem.click()
        await expect(page.locator('.cm-content')).toContainText('Plain text note.')
        await sameItem.click()

        // Assert
        await expect(page.locator('.cm-content')).toContainText('Persisted across file switch.')
        expect(await readFile(join(workspace.root, 'same.md'), 'utf8')).toContain(
          'Persisted across file switch.',
        )
      },
    )
  })

  test.describe('When creating, renaming, and deleting files', () => {
    test(
      'should auto-increment conflicting untitled filenames, reject duplicate rename targets, and delete files on disk',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const firstUntitled = join(workspace.root, 'Untitled.md')
        const secondUntitled = join(workspace.root, 'Untitled 2.md')
        const renamed = join(workspace.root, 'Renamed.md')
        const nameInput = page.getByRole('textbox', { name: 'Name', exact: true })

        // Act
        await createFile(page)
        await createFile(page)

        // Assert
        await expect(entry(page, 'Untitled.md')).toBeVisible()
        await expect(entry(page, 'Untitled 2.md')).toBeVisible()
        expect(await readFile(firstUntitled, 'utf8')).toBe('# Untitled\n\n')
        expect(await readFile(secondUntitled, 'utf8')).toBe('# Untitled 2\n\n')

        // Act
        await contextAction(page, 'Untitled 2.md', 'Rename')
        await nameInput.fill('Untitled.md')
        await nameInput.press('Enter')

        // Assert
        await expect(page.getByRole('alert')).toContainText('already exists')
        await expect(nameInput).toHaveValue('Untitled.md')
        await expect(nameInput).toHaveAttribute('aria-invalid', 'true')
        expect(await exists(firstUntitled)).toBe(true)
        expect(await exists(secondUntitled)).toBe(true)

        // Act
        await nameInput.fill('Renamed.md')
        await nameInput.press('Enter')

        // Assert
        await expect(nameInput).toHaveCount(0)
        await expect(page.locator('.document-path')).toHaveAttribute('title', 'Renamed.md')
        expect(await exists(secondUntitled)).toBe(false)
        expect(await exists(renamed)).toBe(true)

        // Act
        await deleteEntry(page, 'Renamed.md')

        // Assert
        await expect(entry(page, 'Renamed.md')).toHaveCount(0)
        expect(await exists(renamed)).toBe(false)
        await expect(page.getByText('No file selected')).toBeVisible()
      },
    )
  })

  test.describe('When creating, renaming, and deleting folders and nested entries', () => {
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
  })
})
