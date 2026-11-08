import { test, expect, entry, exists, createGitRepo, createExternalWorktree } from './fixtures'
import { readFile, realpath, writeFile } from 'node:fs/promises'
import { basename, join } from 'node:path'

// Helpers ---------------------------------------------------------------------
async function openFolderViaPicker(
  page: import('@playwright/test').Page,
  workspace: import('./fixtures').Workspace,
  folderPath: string,
) {
  const canonical = await realpath(folderPath)
  workspace.pickedFolder = canonical
  if (await page.getByRole('button', { name: 'Add project', exact: true }).isVisible()) {
    await page.getByRole('button', { name: 'Add project', exact: true }).click()
  } else {
    await page.getByRole('combobox', { name: 'Workspace selector', exact: true }).click()
    await page.getByRole('option', { name: 'Open folder…', exact: true }).click()
  }
  await expect.poll(() => workspace.root).toBe(canonical)
}

// Tests -----------------------------------------------------------------------
test.describe('Given a session with open projects', { tag: ['@desktop', '@projects'] }, () => {
  test.use({ initialWorkspace: 'existing' })

  test.describe('When toggling Explorer and Projects in the footer bar', () => {
    test(
      'should swap the left sidebar between WorkspaceExplorer and ProjectsPane and persist sidebarWidget across reload',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const explorerBtn = page.getByRole('button', { name: 'Explorer view', exact: true })
        const projectsBtn = page.getByRole('button', { name: 'Projects view', exact: true })
        await expect(explorerBtn).toHaveAttribute('aria-pressed', 'true')
        await expect(page.getByRole('tree', { name: 'Workspace files' })).toBeVisible()

        // Act
        await projectsBtn.click()
        await expect
          .poll(async () => {
            const sessionRaw = await readFile(
              join(workspace.lgtmHomeDir, 'sessions/default.json'),
              'utf8',
            ).catch(() => '')
            return sessionRaw.includes('"sidebarWidget": "projects"')
          })
          .toBe(true)
        await page.reload()

        // Assert
        await expect(projectsBtn).toHaveAttribute('aria-pressed', 'true')
        await expect(page.getByRole('tree', { name: 'Projects' })).toBeVisible()
        await expect(page.getByRole('button', { name: 'Add project', exact: true })).toBeVisible()
      },
    )
  })
})

test.describe('Given a project with linked worktrees', { tag: ['@desktop', '@projects'] }, () => {
  test.use({ initialWorkspace: 'existing' })

  test.describe('When collapsing and expanding the project branch in TreeView', () => {
    test(
      'should collapse and expand child worktree items using ArkTreeView.Branch while keeping an active linked worktree visible',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const rawDir = await workspace.createExternalDir()
        const repoDir = await createGitRepo(join(rawDir, 'alpha-repo'), {
          'README.md': '# Alpha\n',
        })
        const extBase = await workspace.createExternalDir()
        const wtOne = await createExternalWorktree(
          repoDir,
          join(extBase, 'wt-one'),
          'feature/wt-one',
        )
        const wtTwo = await createExternalWorktree(
          repoDir,
          join(extBase, 'wt-two'),
          'feature/wt-two',
        )

        await page.getByRole('button', { name: 'Projects view', exact: true }).click()
        await openFolderViaPicker(page, workspace, repoDir)
        await openFolderViaPicker(page, workspace, wtOne)
        await openFolderViaPicker(page, workspace, wtTwo)

        const projectItem = entry(page, repoDir)
        const wtOneItem = entry(page, wtOne)
        const wtTwoItem = entry(page, wtTwo)

        // Act
        await projectItem
          .getByRole('button', { name: `Collapse ${basename(repoDir)}`, exact: true })
          .click()
        const wtOneHiddenWhenCollapsed = (await wtOneItem.count()) === 0
        const wtTwoVisibleWhenActive = await wtTwoItem.isVisible()
        await projectItem.locator('[data-part="branch-control"]').first().click()
        const wtTwoHiddenAfterPrimarySelected = (await wtTwoItem.count()) === 0
        await projectItem
          .getByRole('button', { name: `Expand ${basename(repoDir)}`, exact: true })
          .click()

        // Assert
        expect(wtOneHiddenWhenCollapsed).toBe(true)
        expect(wtTwoVisibleWhenActive).toBe(true)
        expect(wtTwoHiddenAfterPrimarySelected).toBe(true)
        await expect(wtOneItem).toBeVisible()
        await expect(wtTwoItem).toBeVisible()
      },
    )

    test('should open the right-click ContextMenu on project and worktree nodes for New Worktree, Remove Worktree, and Remove Project', async ({
      page,
      workspace,
    }) => {
      // Arrange
      const rawDir = await workspace.createExternalDir()
      const repoDir = await createGitRepo(join(rawDir, 'ctx-repo'), {
        'README.md': '# Ctx\n',
      })
      const extBase = await workspace.createExternalDir()
      const wtExt = await createExternalWorktree(repoDir, join(extBase, 'wt-ext'), 'feature/wt-ext')

      await page.getByRole('button', { name: 'Projects view', exact: true }).click()
      await openFolderViaPicker(page, workspace, repoDir)
      await openFolderViaPicker(page, workspace, wtExt)

      const projectItem = entry(page, repoDir)
      const wtItem = entry(page, wtExt)

      // Act
      await wtItem.click({ button: 'right' })
      await page.getByRole('menuitem', { name: 'Remove Worktree', exact: true }).click()
      const removeDialog = page.getByRole('dialog', { name: 'Remove Worktree' })
      await removeDialog.getByRole('button', { name: 'Remove worktree', exact: true }).click()
      await removeDialog.waitFor({ state: 'hidden' })
      await projectItem.click({ button: 'right' })
      await page.getByRole('menuitem', { name: 'Remove Project', exact: true }).click()

      // Assert
      await expect(wtItem).toHaveCount(0)
      await expect(projectItem).toHaveCount(0)
      expect(await exists(wtExt)).toBe(true)
      expect(await exists(repoDir)).toBe(true)
    })
  })
})

test.describe('Given a Git project', { tag: ['@desktop', '@projects'] }, () => {
  test.use({ initialWorkspace: 'existing' })

  test.describe('When creating and removing managed worktrees from ProjectsPane', () => {
    test(
      'should preview ~/.lgtm/worktrees/<project>/<branch-slug>, create the worktree, and switch to it',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const rawDir = await workspace.createExternalDir()
        const repoDir = await createGitRepo(join(rawDir, 'managed-repo'), {
          'README.md': '# Managed Repo\n',
        })
        await page.getByRole('button', { name: 'Projects view', exact: true }).click()
        await openFolderViaPicker(page, workspace, repoDir)
        const projectItem = entry(page, repoDir)
        const dialog = page.getByRole('dialog', { name: 'New Worktree' })

        // Act
        await projectItem.click({ button: 'right' })
        await page.getByRole('menuitem', { name: 'New Worktree', exact: true }).click()
        const branchInput = dialog.getByRole('textbox', { name: 'Branch name', exact: true })
        await branchInput.fill('worktree/Feature/New-UI')
        const previewText = await dialog.getByTestId('worktree-preview-path').innerText()
        await dialog.getByRole('button', { name: 'Create worktree', exact: true }).click()

        // Assert
        expect(previewText).toBe('~/.lgtm/worktrees/managed-repo/feature-new-ui')
        await expect(dialog).toHaveCount(0)
        const rawManagedPath = join(
          workspace.lgtmHomeDir,
          'worktrees',
          'managed-repo',
          'feature-new-ui',
        )
        expect(await exists(rawManagedPath)).toBe(true)
        const expectedManagedPath = await realpath(rawManagedPath)
        await expect(entry(page, expectedManagedPath)).toHaveAttribute('aria-selected', 'true')
      },
    )

    test('should display an inline error in the dialog when the branch is invalid or already checked out', async ({
      page,
      workspace,
    }) => {
      // Arrange
      const rawDir = await workspace.createExternalDir()
      const repoDir = await createGitRepo(join(rawDir, 'err-repo'), {
        'README.md': '# Error Repo\n',
      })
      await page.getByRole('button', { name: 'Projects view', exact: true }).click()
      await openFolderViaPicker(page, workspace, repoDir)

      // Act
      await page.getByRole('button', { name: 'New worktree', exact: true }).click()
      const dialog = page.getByRole('dialog', { name: 'New Worktree' })
      const branchInput = dialog.getByRole('textbox', { name: 'Branch name', exact: true })
      await branchInput.fill('main')
      await dialog.getByRole('button', { name: 'Create worktree', exact: true }).click()

      // Assert
      await expect(dialog).toBeVisible()
      await expect(dialog.getByRole('alert')).toContainText('already checked out')
    })

    test('should delete managed worktree storage on removal with force confirmation when dirty while only detaching external worktrees', async ({
      page,
      workspace,
    }) => {
      // Arrange
      const rawDir = await workspace.createExternalDir()
      const repoDir = await createGitRepo(join(rawDir, 'clean-repo'), {
        'README.md': '# Clean Repo\n',
      })
      await page.getByRole('button', { name: 'Projects view', exact: true }).click()
      await openFolderViaPicker(page, workspace, repoDir)

      await page.getByRole('button', { name: 'New worktree', exact: true }).click()
      const dialog = page.getByRole('dialog', { name: 'New Worktree' })
      await dialog
        .getByRole('textbox', { name: 'Branch name', exact: true })
        .fill('feature/dirty-wt')
      await dialog.getByRole('button', { name: 'Create worktree', exact: true }).click()
      await expect(dialog).toHaveCount(0)

      const rawManagedPath = join(
        workspace.lgtmHomeDir,
        'worktrees',
        'clean-repo',
        'feature-dirty-wt',
      )
      const managedPath = await realpath(rawManagedPath)
      await writeFile(join(managedPath, 'untracked.txt'), 'dirty worktree content\n', 'utf8')
      const wtItem = entry(page, managedPath)
      const removeDialog = page.getByRole('dialog', { name: 'Remove Worktree' })

      // Act
      await wtItem.click({ button: 'right' })
      await page.getByRole('menuitem', { name: 'Remove Worktree', exact: true }).click()
      await removeDialog.getByRole('button', { name: 'Remove worktree', exact: true }).click()
      const dirtyAlertText = await removeDialog.getByRole('alert').innerText()
      await removeDialog.getByRole('button', { name: 'Delete anyway', exact: true }).click()

      // Assert
      expect(dirtyAlertText).toBeTruthy()
      await expect(removeDialog).toHaveCount(0)
      await expect(wtItem).toHaveCount(0)
      expect(await exists(managedPath)).toBe(false)
    })
  })
})

test.describe(
  'Given multiple workspaces with open tabs',
  { tag: ['@desktop', '@projects'] },
  () => {
    test.use({ initialWorkspace: 'existing' })

    test.describe('When switching workspaces and restarting the app', () => {
      test(
        'should switch between project primary checkout and worktrees immediately using cached state and retain only edited tabs and the last opened tab across switches and restarts',
        { tag: '@smoke' },
        async ({ page, workspace }) => {
          // Arrange
          const rawDir = await workspace.createExternalDir()
          const secondRepo = await createGitRepo(join(rawDir, 'second-repo'), {
            'alpha.md': '# Alpha\n',
            'beta.md': '# Beta\n',
          })
          const tablist = page.getByRole('tablist', { name: 'Open editors' })

          await entry(page, 'Drafts').click()
          await entry(page, 'Drafts/one.md').click()
          await page.locator('.cm-content').click()
          await page.keyboard.insertText('Edited line\n')
          await expect
            .poll(async () => readFile(join(workspace.root, 'Drafts/one.md'), 'utf8'))
            .toContain('Edited line')

          await entry(page, 'same.md').click()
          await entry(page, 'notes.txt').click()
          const firstRoot = workspace.root

          // Act
          await openFolderViaPicker(page, workspace, secondRepo)
          await entry(page, 'alpha.md').click()
          await page.getByRole('button', { name: 'Projects view', exact: true }).click()
          await entry(page, firstRoot).click()
          await page.getByRole('button', { name: 'Explorer view', exact: true }).click()
          await page.reload()

          // Assert
          await expect(tablist.getByRole('tab', { name: 'one.md', exact: true })).toBeVisible()
          await expect(
            tablist.getByRole('tab', { name: 'notes.txt', exact: true }),
          ).toHaveAttribute('aria-selected', 'true')
          await expect(tablist.getByRole('tab', { name: 'same.md', exact: true })).toHaveCount(0)
          await expect(entry(page, 'Drafts/one.md')).toBeVisible()
        },
      )
    })

    test.describe('When using Helical UI WhichKey leader chords, CommandMenu, and TreeView hotkeys', () => {
      test(
        'should switch to Projects via Space p p, open New Worktree via :worktree-new, and navigate project branches with modal TreeView hotkeys',
        { tag: '@smoke' },
        async ({ page, workspace }) => {
          // Arrange
          const rawDir = await workspace.createExternalDir()
          const repoDir = await createGitRepo(join(rawDir, 'hotkey-repo'), {
            'README.md': '# Hotkey Repo\n',
          })
          await openFolderViaPicker(page, workspace, repoDir)
          const dialog = page.getByRole('dialog', { name: 'New Worktree' })

          // Act
          await page.keyboard.press('Space')
          await page.keyboard.press('p')
          await page.keyboard.press('p')
          await page.keyboard.press(':')
          const cmdInput = page.getByTestId('command-menu-input')
          await cmdInput.fill('worktree-new')
          await cmdInput.press('Enter')
          const openedFromCommand = await dialog.isVisible()
          await page.keyboard.press('Escape')
          const projectControl = entry(page, repoDir)
          await projectControl.focus()
          await page.keyboard.press('a')
          const openedFromTreeHotkey = await dialog.isVisible()
          await page.keyboard.press('Escape')

          // Assert
          await expect(page.getByRole('tree', { name: 'Projects' })).toBeVisible()
          expect(openedFromCommand).toBe(true)
          expect(openedFromTreeHotkey).toBe(true)
          await expect(dialog).toHaveCount(0)
        },
      )
    })
  },
)
