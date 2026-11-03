import { test, expect, entry, createFile, renameEntry, exists } from './fixtures'
import { writeFile } from 'node:fs/promises'
import { join } from 'node:path'

// Tests -----------------------------------------------------------------------
test.describe('Given a new workspace', { tag: ['@desktop', '@lsp'] }, () => {
  test.use({ initialWorkspace: 'default-seeded' })

  test.describe('When opening the default workspace containing loose source files', () => {
    test(
      'should automatically warm up workspace language servers and open the diagnostics buffer',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const lspButton = page.getByRole('button', { name: 'LSP status', exact: true })

        // Assert
        expect(await exists(join(workspace.root, '.lgtm/mise.toml'))).toBe(true)
        await expect(lspButton).toContainText('LSP: python (ready, 1 error)')

        // Act
        await lspButton.click()
        const popover = page.getByRole('dialog', { name: 'LSP servers and diagnostics' })
        await expect(popover).toBeVisible()
        await expect(popover).toContainText('python')
        await expect(popover).toContainText('typescript')
        await popover.getByRole('button', { name: 'Open diagnostics buffer', exact: true }).click()

        // Assert
        const tablist = page.getByRole('tablist', { name: 'Open editors' })
        await expect(
          tablist.getByRole('tab', { name: 'LSP Diagnostics', exact: true }),
        ).toHaveAttribute('aria-selected', 'true')
        await expect(page.locator('.cm-content')).toContainText('# LSP Diagnostics')
        await expect(page.locator('.cm-content')).toContainText('python [ready]')
        await expect(page.locator('.cm-content')).toContainText('typescript [ready]')
        await expect(page.locator('.cm-content')).toContainText(
          'hello.py:2:5 [ERROR] [E0425] (rustc) cannot find value `BROKEN` in this scope',
        )
      },
    )
  })
})

test.describe('Given an existing workspace', { tag: ['@desktop', '@lsp'] }, () => {
  test.use({ initialWorkspace: 'existing' })

  test.describe('When opening multiple files in tabs', () => {
    test(
      'should switch between open file tabs and close tabs cleanly',
      { tag: '@smoke' },
      async ({ page }) => {
        // Arrange
        const tablist = page.getByRole('tablist', { name: 'Open editors' })

        // Act
        await entry(page, 'same.md').click()
        await entry(page, 'notes.txt').click()

        // Assert
        await expect(tablist.getByRole('tab')).toHaveCount(2)
        await expect(tablist.getByRole('tab', { name: 'notes.txt', exact: true })).toHaveAttribute(
          'aria-selected',
          'true',
        )
        await expect(page.locator('.cm-content')).toContainText('Plain text note.')

        // Act
        await tablist.getByRole('tab', { name: 'same.md', exact: true }).click()

        // Assert
        await expect(tablist.getByRole('tab', { name: 'same.md', exact: true })).toHaveAttribute(
          'aria-selected',
          'true',
        )
        await expect(page.locator('.cm-content')).toContainText('# Markdown')

        // Act
        await page.getByRole('button', { name: 'Close same.md', exact: true }).click()

        // Assert
        await expect(tablist.getByRole('tab')).toHaveCount(1)
        await expect(page.locator('.cm-content')).toContainText('Plain text note.')
      },
    )
  })

  test.describe('When inspecting LSP status and diagnostics', () => {
    test(
      'should display active language server status, inline lint markers, and open full diagnostics in a read-only buffer tab',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        await createFile(page)
        await renameEntry(page, 'Untitled.md', 'lib.rs')
        await entry(page, 'same.md').click()
        await writeFile(
          join(workspace.root, 'lib.rs'),
          'pub fn helper() -> i32 {\n    42\n}\n\npub fn main() {\n    BROKEN();\n}\n',
          'utf8',
        )

        // Act
        await entry(page, 'lib.rs').click()

        // Assert
        const lspButton = page.getByRole('button', { name: 'LSP status', exact: true })
        await expect(lspButton).toContainText('LSP: rust (ready, 1 error)')
        await expect(page.locator('.cm-lintRange-error')).toBeVisible()

        // Act
        await lspButton.click()
        const popover = page.getByRole('dialog', { name: 'LSP servers and diagnostics' })
        await expect(popover).toBeVisible()
        await expect(popover).toContainText('cannot find value `BROKEN` in this scope')
        await popover.getByRole('button', { name: 'View rust diagnostics', exact: true }).click()

        // Assert
        const tablist = page.getByRole('tablist', { name: 'Open editors' })
        await expect(
          tablist.getByRole('tab', { name: 'LSP Diagnostics (rust)', exact: true }),
        ).toHaveAttribute('aria-selected', 'true')
        await expect(page.locator('.cm-content')).toContainText('# LSP Diagnostics (rust)')
        await expect(page.locator('.cm-content')).toContainText(
          'lib.rs:6:5 [ERROR] [E0425] (rustc) cannot find value `BROKEN` in this scope',
        )
        await expect(page.locator('.cm-content')).toHaveAttribute('contenteditable', 'false')

        // Act
        await lspButton.click()
        await page
          .getByRole('dialog', { name: 'LSP servers and diagnostics' })
          .getByRole('button', { name: /lib\.rs:6:5/ })
          .click()

        // Assert
        await expect(tablist.getByRole('tab', { name: 'lib.rs', exact: true })).toHaveAttribute(
          'aria-selected',
          'true',
        )
      },
    )
  })

  test.describe('When navigating to definitions across files via LSP', () => {
    test(
      'should jump to symbol definition on F12',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        await createFile(page)
        await renameEntry(page, 'Untitled.md', 'helper.rs')
        await writeFile(
          join(workspace.root, 'helper.rs'),
          'pub fn helper() -> i32 {\n    42\n}\n',
          'utf8',
        )
        await createFile(page)
        await renameEntry(page, 'Untitled.md', 'caller.rs')
        await entry(page, 'same.md').click()
        await writeFile(
          join(workspace.root, 'caller.rs'),
          'use crate::helper;\nfn run() -> i32 {\n    helper()\n}\n',
          'utf8',
        )
        await entry(page, 'helper.rs').click()
        await entry(page, 'caller.rs').click()

        // Act
        await page.locator('.cm-content').click()
        await page.keyboard.press('F12')

        // Assert
        const tablist = page.getByRole('tablist', { name: 'Open editors' })
        await expect(tablist.getByRole('tab', { name: 'helper.rs', exact: true })).toHaveAttribute(
          'aria-selected',
          'true',
        )
        await expect(page.locator('.cm-content')).toContainText('pub fn helper() -> i32')
      },
    )
  })
})
