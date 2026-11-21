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
import { readFile, readdir, realpath, stat, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

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
      'should remember opened workspaces in the Projects pane and reopen them',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const defaultRoot = await realpath(workspace.root)
        const externalDir = await realpath(await workspace.createExternalDir())
        await writeFile(join(externalDir, 'Cargo.toml'), '[package]\nname = "demo"\n')
        workspace.pickedFolder = externalDir

        // Act
        await createFile(page)

        // Assert
        await expect(entry(page, 'Untitled.md')).toBeVisible()
        expect(await readFile(join(defaultRoot, 'Untitled.md'), 'utf8')).toBe('# Untitled\n\n')

        // Act
        await page.getByRole('button', { name: 'Projects view', exact: true }).click()
        await page.getByRole('button', { name: 'Add project', exact: true }).click()

        // Assert
        await expect(entry(page, 'Cargo.toml')).toBeVisible()
        await expect(entry(page, externalDir)).toHaveAttribute('aria-selected', 'true')

        // Act
        await entry(page, defaultRoot).click()

        // Assert
        await expect(entry(page, 'Untitled.md')).toBeVisible()

        // Act
        await entry(page, externalDir).click()

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

  test.describe('When browsing the file tree', () => {
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

  test.describe('When viewing a file', () => {
    test(
      'should render file content with syntax highlighting',
      { tag: '@smoke' },
      async ({ page }) => {
        // Arrange
        const sameItem = page.locator('[role="treeitem"][data-value$="same.md"]')

        // Act
        await sameItem.click()

        // Assert
        await expect(page.locator('.cm-content')).toContainText('# Markdown')
        await expect(page.locator('.cm-content.cm-lineWrapping')).toBeVisible()
        await expect(page.locator('.cm-content .cm-line span[class]').first()).toBeVisible()
        await expect(page.locator('.document-heading')).toHaveCount(0)
      },
    )
  })

  test.describe('When editing a file', () => {
    test(
      'should persist edits across file switches and on disk',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const sameItem = page.locator('[role="treeitem"][data-value$="same.md"]')
        const notesItem = page.locator('[role="treeitem"][data-value$="notes.txt"]')
        await sameItem.click()
        await expect(page.locator('.cm-content')).toContainText('# Markdown')

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

  test.describe('When creating a file', () => {
    test(
      'should create a new untitled file at the root or inside a folder via context menu',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const drafts = page.getByRole('treeitem', { name: 'Drafts', exact: true })

        // Act
        await createFile(page)

        // Assert
        await expect(entry(page, 'Untitled.md')).toBeVisible()
        await expect(page.locator('.cm-content')).toContainText('# Untitled')
        expect(await readFile(join(workspace.root, 'Untitled.md'), 'utf8')).toBe('# Untitled\n\n')
        await expect(page.getByRole('textbox', { name: 'Name', exact: true })).toHaveCount(0)

        // Act
        await drafts.getByRole('button', { name: 'Drafts', exact: true }).click({ button: 'right' })
        await expect
          .poll(async () => (await page.getByRole('menu').boundingBox())?.x ?? 0)
          .toBeGreaterThan(0)
        await page.getByRole('menuitem', { name: 'New file', exact: true }).click()

        // Assert
        await expect(page.locator('.document-path')).toHaveAttribute('title', 'Drafts/Untitled.md')
        expect(await readFile(join(workspace.root, 'Drafts/Untitled.md'), 'utf8')).toBe(
          '# Untitled\n\n',
        )
      },
    )

    test('should auto-increment the filename when Untitled.md already exists', async ({
      page,
      workspace,
    }) => {
      // Arrange
      const firstUntitled = join(workspace.root, 'Untitled.md')
      const secondUntitled = join(workspace.root, 'Untitled 2.md')

      // Act
      await createFile(page)
      await createFile(page)

      // Assert
      await expect(entry(page, 'Untitled.md')).toBeVisible()
      await expect(entry(page, 'Untitled 2.md')).toBeVisible()
      expect(await readFile(firstUntitled, 'utf8')).toBe('# Untitled\n\n')
      expect(await readFile(secondUntitled, 'utf8')).toBe('# Untitled 2\n\n')
    })
  })

  test.describe('When creating a folder', () => {
    test(
      'should create a new folder inline and cancel creation on Escape',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const name = page.getByRole('textbox', { name: 'Name', exact: true })
        const researchDir = join(workspace.root, 'Research')

        // Act
        await createFolder(page, 'Research')

        // Assert
        expect((await stat(researchDir)).isDirectory()).toBe(true)
        await expect(entry(page, 'Research')).toHaveAttribute('aria-expanded', 'false')

        // Act
        await page.getByRole('button', { name: 'New folder', exact: true }).click()
        await name.fill('Cancelled')
        await name.press('Escape')

        // Assert
        expect(await readdir(workspace.root)).not.toContain('Cancelled')
      },
    )

    test('should reject duplicate folder names and retain input after errors', async ({
      page,
      workspace,
    }) => {
      // Arrange
      const name = page.getByRole('textbox', { name: 'Name', exact: true })
      await createFolder(page, 'Research')

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
    })
  })

  test.describe('When renaming a file or folder', () => {
    test(
      'should rename files and folders on disk and update the active document path',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const name = page.getByRole('textbox', { name: 'Name', exact: true })
        const originalFile = join(workspace.root, 'same.md')
        const renamedFile = join(workspace.root, 'Renamed.md')
        const originalFolder = join(workspace.root, 'Empty')
        const renamedFolder = join(workspace.root, 'Archive')
        await entry(page, 'same.md').click()
        await expect(page.locator('.cm-content')).toContainText('# Markdown')

        // Act
        await renameEntry(page, 'same.md', 'Renamed.md')

        // Assert
        await expect(page.locator('.document-path')).toHaveAttribute('title', 'Renamed.md')
        expect(await exists(originalFile)).toBe(false)
        expect(await exists(renamedFile)).toBe(true)

        // Act
        await entry(page, 'Empty').getByRole('button', { name: 'Empty', exact: true }).focus()
        await page.keyboard.press('F2')
        await name.fill('Archive')
        await name.press('Enter')

        // Assert
        await expect(entry(page, 'Archive')).toBeVisible()
        await expect.poll(() => exists(originalFolder)).toBe(false)
        expect((await stat(renamedFolder)).isDirectory()).toBe(true)
      },
    )

    test('should reject conflicting target names without overwriting existing entries', async ({
      page,
      workspace,
    }) => {
      // Arrange
      const nameInput = page.getByRole('textbox', { name: 'Name', exact: true })
      const samePath = join(workspace.root, 'same.md')
      const notesPath = join(workspace.root, 'notes.txt')

      // Act
      await contextAction(page, 'notes.txt', 'Rename')
      await nameInput.fill('same.md')
      await nameInput.press('Enter')

      // Assert
      await expect(page.getByRole('alert')).toContainText('already exists')
      await expect(nameInput).toHaveValue('same.md')
      await expect(nameInput).toHaveAttribute('aria-invalid', 'true')
      expect(await readFile(samePath, 'utf8')).toBe('# Markdown\n')
      expect(await readFile(notesPath, 'utf8')).toBe('Plain text note.\n')
    })
  })

  test.describe('When deleting a file or folder', () => {
    test(
      'should move files and folders to Trash and clear selection when deleting the open file',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const samePath = join(workspace.root, 'same.md')
        const draftsPath = join(workspace.root, 'Drafts')
        await entry(page, 'same.md').click()
        await expect(page.locator('.cm-content')).toContainText('# Markdown')

        // Act
        await deleteEntry(page, 'same.md')

        // Assert
        await expect(entry(page, 'same.md')).toHaveCount(0)
        expect(await exists(samePath)).toBe(false)
        await expect(page.getByText('No file selected')).toBeVisible()

        // Act
        await deleteEntry(page, 'Drafts')

        // Assert
        await expect(entry(page, 'Drafts')).toHaveCount(0)
        expect(await exists(draftsPath)).toBe(false)
      },
    )
  })
})
