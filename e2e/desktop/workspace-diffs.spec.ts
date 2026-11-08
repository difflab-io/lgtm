import { test, expect, entry, captureVisualSnapshot } from './fixtures'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'

// Tests -----------------------------------------------------------------------
test.describe('Given an existing workspace', { tag: ['@desktop', '@diffs'] }, () => {
  test.use({ initialWorkspace: 'diff-seeded' })

  test.describe('When viewing diffs in Local mode', () => {
    test(
      'should list Working Tree and local shadow commits in the bottom checklist and render single-file diffs',
      { tag: '@smoke' },
      async ({ page }, testInfo) => {
        // Arrange
        const toggleDiffs = page.getByRole('button', { name: 'Toggle diffs', exact: true })

        // Act
        await toggleDiffs.click()
        await expect(page.getByRole('region', { name: 'Commits checklist' })).toBeVisible()
        await entry(page, 'src/lib.rs').click()

        // Assert
        const checklist = page.getByRole('list', { name: 'Commit range checklist' })
        await expect(checklist.getByRole('checkbox')).toHaveCount(5)
        await expect(
          checklist.getByRole('checkbox', { name: /WORKTREE Working Tree/ }),
        ).toBeChecked()
        await expect(
          checklist.getByRole('checkbox', { name: /lgtm: refine beta calculation/ }),
        ).not.toBeChecked()
        await expect(
          checklist.getByRole('checkbox', { name: /lgtm: add diff service/ }),
        ).not.toBeChecked()

        await expect(entry(page, 'docs/architecture')).toHaveAttribute('aria-expanded', 'true')
        await expect(entry(page, 'src')).toHaveAttribute('aria-expanded', 'true')
        await expect(page.getByTestId('diff-file-stats-docs/architecture/diffs.md')).toContainText(
          'A +3 -0',
        )
        await expect(page.getByTestId('diff-file-stats-src/lib.rs')).toContainText('M +1 -1')
        await expect(page.getByTestId('diff-active-filepath')).toHaveText('src/lib.rs')
        await expect(page.locator('.cm-mergeViewEditor')).toHaveCount(2)
        await expect(page.locator('.cm-mergeViewEditor').nth(0)).toContainText('3')
        await expect(page.locator('.cm-mergeViewEditor').nth(1)).toContainText(
          '30 // modified in WT',
        )

        await captureVisualSnapshot(page, testInfo, '01-local-working-tree-split')

        // Act: collapse and re-expand the `src` folder branch in DiffExplorer
        await entry(page, 'src').getByRole('button', { name: 'src', exact: true }).click()
        await expect(entry(page, 'src')).toHaveAttribute('aria-expanded', 'false')
        await entry(page, 'src').getByRole('button', { name: 'src', exact: true }).click()
        await expect(entry(page, 'src')).toHaveAttribute('aria-expanded', 'true')
      },
    )

    test('should recompute the file list and diff content when multiple commits are checked in the checklist', async ({
      page,
    }, testInfo) => {
      // Arrange
      await page.getByRole('button', { name: 'Toggle diffs', exact: true }).click()
      const checklist = page.getByRole('list', { name: 'Commit range checklist' })
      const worktreeCheckbox = checklist.getByRole('checkbox', { name: /WORKTREE Working Tree/ })
      const s2Checkbox = checklist.getByRole('checkbox', { name: /lgtm: refine beta calculation/ })
      const s1Checkbox = checklist.getByRole('checkbox', { name: /lgtm: add diff service/ })

      // Act: select S1 + S2 and uncheck Working Tree
      await s2Checkbox.check()
      await s1Checkbox.check()
      await worktreeCheckbox.uncheck()

      // Assert
      await expect(page.getByTestId('diff-file-stats-src/lib.rs')).toContainText('M +1 -1')
      await expect(page.getByTestId('diff-file-stats-src/old.rs')).toContainText('D +0 -2')
      await expect(page.getByTestId('diff-file-stats-src/services/diff.rs')).toContainText(
        'A +5 -0',
      )
      await entry(page, 'src/services/diff.rs').click()
      await expect(page.getByTestId('diff-active-filepath')).toHaveText('src/services/diff.rs')
      await expect(page.locator('.cm-mergeViewEditor').nth(1)).toContainText(
        'pub struct DiffService;',
      )

      await captureVisualSnapshot(page, testInfo, '02-multi-commit-range-checked')
    })

    test('should toggle between split and unified modes and between hunk-only and full-file views', async ({
      page,
    }, testInfo) => {
      // Arrange
      await page.getByRole('button', { name: 'Toggle diffs', exact: true }).click()
      await entry(page, 'src/lib.rs').click()
      await expect(page.locator('.cm-mergeViewEditor')).toHaveCount(2)
      await expect(page.locator('.cm-collapsedLines').first()).toBeVisible()

      // Act: switch to Unified mode (hunks only)
      await page
        .getByRole('group', { name: 'Diff layout mode' })
        .getByRole('button', { name: 'Unified', exact: true })
        .click()

      // Assert
      await expect(page.locator('.cm-mergeViewEditor')).toHaveCount(0)
      await expect(page.locator('.cm-editor')).toHaveCount(1)
      await expect(page.locator('.cm-deletedChunk').first()).toBeVisible()

      await captureVisualSnapshot(page, testInfo, '03-unified-mode-hunks')

      // Act: switch back to Split mode and expand Full File
      await page
        .getByRole('group', { name: 'Diff layout mode' })
        .getByRole('button', { name: 'Split', exact: true })
        .click()
      await page.getByRole('button', { name: 'Full File', exact: true }).click()

      // Assert
      await expect(page.locator('.cm-mergeViewEditor')).toHaveCount(2)
      await expect(page.locator('.cm-collapsedLines')).toHaveCount(0)
      await expect(page.locator('.cm-mergeViewEditor').nth(1)).toContainText('pub fn alpha()')
      await expect(page.locator('.cm-mergeViewEditor').nth(1)).toContainText('pub fn gamma()')

      await captureVisualSnapshot(page, testInfo, '04-full-file-expanded')
    })

    test('should save live edits to the working tree and refresh diff counts when Working Tree is selected', async ({
      page,
      workspace,
    }, testInfo) => {
      // Arrange
      await page.getByRole('button', { name: 'Toggle diffs', exact: true }).click()
      await entry(page, 'src/lib.rs').click()
      await page.getByRole('button', { name: 'Full File', exact: true }).click()
      const rightContent = page.locator('.cm-mergeViewEditor').nth(1).locator('.cm-content')

      // Act
      await rightContent.click()
      await page.keyboard.press('End')
      await page.keyboard.press('Enter')
      await page.keyboard.type('pub fn delta() -> i32 { 40 }')
      await page.getByRole('button', { name: 'Save', exact: true }).click()

      // Assert
      await expect(page.getByTestId('diff-file-stats-src/lib.rs')).toContainText('M +2 -1')
      const diskText = await readFile(join(workspace.root, 'src/lib.rs'), 'utf8')
      expect(diskText).toContain('pub fn delta() -> i32 { 40 }')

      await captureVisualSnapshot(page, testInfo, '05-working-tree-edited-and-saved')
    })
  })

  test.describe('When switching to ForgeBacked mode and navigating via keyboard', () => {
    test(
      'should list standard Git commits in the checklist and navigate files and modes with keyboard shortcuts',
      { tag: '@smoke' },
      async ({ page }, testInfo) => {
        // Arrange
        await page.getByRole('button', { name: 'Toggle diffs', exact: true }).click()
        const checklist = page.getByRole('list', { name: 'Commit range checklist' })
        await expect(checklist.getByRole('checkbox')).toHaveCount(5)

        // Act: switch ReviewService to ForgeBacked mode with Space d r
        await page.keyboard.press(' ')
        await page.keyboard.press('d')
        await page.keyboard.press('r')

        // Assert: only the 2 standard Git branch commits (C2 and C1) remain in the checklist
        await expect(
          page
            .getByRole('group', { name: 'Review mode' })
            .getByRole('button', { name: 'Branch', exact: true }),
        ).toHaveAttribute('aria-pressed', 'true')
        await expect(checklist.getByRole('checkbox')).toHaveCount(2)
        await expect(
          checklist.getByRole('checkbox', { name: /feat: update alpha and docs/ }),
        ).toBeChecked()
        await expect(page.getByTestId('diff-active-filepath')).toHaveText(
          'docs/architecture/index.md',
        )

        // Act: navigate to next changed file with ]f and toggle view mode with Space d m
        await page.keyboard.press(']')
        await page.keyboard.press('f')
        await expect(page.getByTestId('diff-active-filepath')).toHaveText('src/lib.rs')

        await page.keyboard.press(' ')
        await page.keyboard.press('d')
        await page.keyboard.press('m')
        await expect(
          page
            .getByRole('group', { name: 'Diff layout mode' })
            .getByRole('button', { name: 'Unified', exact: true }),
        ).toHaveAttribute('aria-pressed', 'true')

        await captureVisualSnapshot(page, testInfo, '06-forge-backed-git-commits')
      },
    )
  })
})
