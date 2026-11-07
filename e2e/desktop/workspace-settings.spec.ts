import { test, expect, entry, exists } from './fixtures'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

const SCHEMA_URL = 'https://cdn.jsdelivr.net/gh/difflab-io/lgtm@v0.1.0/schemas/config.schema.json'

// Tests -----------------------------------------------------------------------
test.describe('Given no global settings', { tag: ['@desktop', '@settings'] }, () => {
  test.use({ initialWorkspace: 'existing' })

  test.describe('When user edits global settings', () => {
    test(
      'should auto-create global config.json if missing and open it in the Settings tab',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const globalConfigPath = join(workspace.lgtmHomeDir, 'config.json')
        const tablist = page.getByRole('tablist', { name: 'Open editors' })
        const lspButton = page.getByRole('button', { name: 'LSP status', exact: true })
        expect(await exists(globalConfigPath)).toBe(false)

        // Act
        await page.keyboard.press('Meta+,')

        // Assert
        await expect(tablist.getByRole('tab', { name: 'Settings', exact: true })).toHaveAttribute(
          'aria-selected',
          'true',
        )
        expect(await exists(globalConfigPath)).toBe(true)
        await expect(lspButton).toContainText('LSP: json (ready)')
        await expect(page.locator('.cm-content')).toContainText(SCHEMA_URL)
        await expect(page.locator('.cm-content')).toContainText('"appearance"')
      },
    )
  })
})

test.describe(
  'Given global settings and no local settings',
  { tag: ['@desktop', '@settings'] },
  () => {
    test.use({ initialWorkspace: 'existing' })

    test.describe('When user edits global settings', () => {
      test(
        'should open global settings in a tab and focus the existing tab when already open',
        { tag: '@smoke' },
        async ({ page, workspace }) => {
          // Arrange
          const globalConfigPath = join(workspace.lgtmHomeDir, 'config.json')
          await writeFile(
            globalConfigPath,
            JSON.stringify(
              {
                $schema: SCHEMA_URL,
                appearance: { theme: 'system' },
                editor: { lineWrapping: 'auto' },
              },
              null,
              2,
            ) + '\n',
            'utf8',
          )
          const tablist = page.getByRole('tablist', { name: 'Open editors' })

          // Act
          await page.keyboard.press('Meta+,')
          await entry(page, 'same.md').click()
          await expect(tablist.getByRole('tab', { name: 'same.md', exact: true })).toHaveAttribute(
            'aria-selected',
            'true',
          )
          await page.keyboard.press('Meta+,')
          await page.keyboard.press('Meta+,')

          // Assert
          await expect(tablist.getByRole('tab', { name: 'Settings', exact: true })).toHaveCount(1)
          await expect(tablist.getByRole('tab', { name: 'Settings', exact: true })).toHaveAttribute(
            'aria-selected',
            'true',
          )
        },
      )
    })

    test.describe('When user saves global settings', () => {
      test(
        'should refresh the editor and shell with updated theme and lineWrapping',
        { tag: '@smoke' },
        async ({ page, workspace }) => {
          // Arrange
          const globalConfigPath = join(workspace.lgtmHomeDir, 'config.json')
          await writeFile(
            globalConfigPath,
            JSON.stringify(
              {
                $schema: SCHEMA_URL,
                appearance: { theme: 'system' },
                editor: { lineWrapping: 'auto' },
              },
              null,
              2,
            ) + '\n',
            'utf8',
          )
          await page.keyboard.press('Meta+,')
          await expect(page.locator('.cm-content')).toHaveClass(/cm-lineWrapping/)

          // Act
          const editorContent = page.locator('.cm-content')
          await editorContent.click()
          await page.keyboard.press('ControlOrMeta+a')
          await page.keyboard.insertText(
            JSON.stringify(
              {
                $schema: SCHEMA_URL,
                appearance: { theme: 'dark' },
                editor: { lineWrapping: 'off' },
              },
              null,
              2,
            ) + '\n',
          )

          // Assert
          await expect(page.locator('html')).toHaveClass(/dark/)
          await expect(page.locator('.cm-content')).not.toHaveClass(/cm-lineWrapping/)
          await expect.poll(() => workspace.setThemeCalls).toContain('dark')
        },
      )
    })

    test.describe('When there are errors in global settings', () => {
      test(
        'should show inline error markers in the settings file and display the error in the LGTM IDE diagnostics buffer',
        { tag: '@smoke' },
        async ({ page, workspace }) => {
          // Arrange
          const globalConfigPath = join(workspace.lgtmHomeDir, 'config.json')
          await writeFile(
            globalConfigPath,
            JSON.stringify({ $schema: SCHEMA_URL, appearance: { theme: 'system' } }, null, 2) +
              '\n',
            'utf8',
          )
          const lspButton = page.getByRole('button', { name: 'LSP status', exact: true })
          const tablist = page.getByRole('tablist', { name: 'Open editors' })
          await page.keyboard.press('Meta+,')

          // Act
          const editorContent = page.locator('.cm-content')
          await editorContent.click()
          await page.keyboard.press('ControlOrMeta+a')
          await page.keyboard.insertText(
            '{\n  "appearance": {\n    "theme": "invalid-theme"\n  }\n}\n',
          )

          // Assert: inline error marker appears in the settings file and LSP status reports 1 error
          await expect(page.locator('.cm-lintRange-error').first()).toBeVisible()
          await expect(lspButton).toContainText('LSP: json (ready, 1 error)')

          // Act: open LGTM IDE diagnostics buffer
          await lspButton.click()
          const menu = page.getByRole('menu', { name: 'LSP servers and diagnostics' })
          await expect(menu).toBeVisible()
          await menu.getByRole('menuitem', { name: 'Open diagnostics buffer', exact: true }).click()

          // Assert: error message is shown in the diagnostics buffer tab
          await expect(
            tablist.getByRole('tab', { name: 'LSP Diagnostics', exact: true }),
          ).toHaveAttribute('aria-selected', 'true')
          await expect(page.locator('.cm-content')).toContainText('# LSP Diagnostics')
          await expect(page.locator('.cm-content')).toContainText('~/.lgtm/config.json:3:5 [ERROR]')
          await expect(page.locator('.cm-content')).toContainText('invalid-theme')
        },
      )
    })

    test.describe('When user edits local settings', () => {
      test(
        'should create global settings if missing and open an empty JSON temp buffer for local settings without creating a local file on disk',
        { tag: '@smoke' },
        async ({ page, workspace }) => {
          // Arrange
          const globalConfigPath = join(workspace.lgtmHomeDir, 'config.json')
          const localConfigPath = join(workspace.root, '.lgtm/config.json')
          const tablist = page.getByRole('tablist', { name: 'Open editors' })
          expect(await exists(globalConfigPath)).toBe(false)
          expect(await exists(localConfigPath)).toBe(false)

          // Act
          await page.keyboard.press('Meta+Shift+,')

          // Assert
          await expect(
            tablist.getByRole('tab', { name: 'Settings (project)', exact: true }),
          ).toHaveAttribute('aria-selected', 'true')
          expect(await exists(globalConfigPath)).toBe(true)
          expect(await exists(localConfigPath)).toBe(false)
          await expect(page.locator('.cm-content')).toHaveText('{}')
        },
      )
    })
  },
)

test.describe('Given global and local settings', { tag: ['@desktop', '@settings'] }, () => {
  test.use({ initialWorkspace: 'existing', colorScheme: 'dark' })

  test.describe('When user edits and saves local settings', () => {
    test(
      'should open local settings in a tab, focus the existing tab on repeat, and refresh the editor with project overrides on save',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const globalConfigPath = join(workspace.lgtmHomeDir, 'config.json')
        const localConfigDir = join(workspace.root, '.lgtm')
        const localConfigPath = join(localConfigDir, 'config.json')
        await mkdir(localConfigDir, { recursive: true })
        await writeFile(
          globalConfigPath,
          JSON.stringify(
            {
              $schema: SCHEMA_URL,
              appearance: { theme: 'dark' },
              editor: { lineWrapping: 'off' },
            },
            null,
            2,
          ) + '\n',
          'utf8',
        )
        await writeFile(
          localConfigPath,
          JSON.stringify(
            {
              $schema: SCHEMA_URL,
              appearance: { theme: 'dark' },
            },
            null,
            2,
          ) + '\n',
          'utf8',
        )
        await page.reload()
        const tablist = page.getByRole('tablist', { name: 'Open editors' })

        // Act: open local settings, switch to another file, and press Cmd+Shift+, again to focus existing tab
        await page.keyboard.press('Meta+Shift+,')
        await expect(
          tablist.getByRole('tab', { name: 'Settings (project)', exact: true }),
        ).toHaveAttribute('aria-selected', 'true')
        await entry(page, 'same.md').click()
        await page.keyboard.press('Meta+Shift+,')
        await page.keyboard.press('Meta+Shift+,')

        // Assert: tab is focused without duplication
        await expect(
          tablist.getByRole('tab', { name: 'Settings (project)', exact: true }),
        ).toHaveCount(1)
        await expect(
          tablist.getByRole('tab', { name: 'Settings (project)', exact: true }),
        ).toHaveAttribute('aria-selected', 'true')

        // Act: save local settings overriding theme to light
        const editorContent = page.locator('.cm-content')
        await editorContent.click()
        await page.keyboard.press('ControlOrMeta+a')
        await page.keyboard.insertText(
          JSON.stringify(
            {
              $schema: SCHEMA_URL,
              appearance: { theme: 'light' },
            },
            null,
            2,
          ) + '\n',
        )

        // Assert: local light theme overrides global dark theme while global lineWrapping off is preserved
        await expect(page.locator('html')).not.toHaveClass(/dark/)
        await expect(page.locator('.cm-content')).not.toHaveClass(/cm-lineWrapping/)
        await expect.poll(() => workspace.setThemeCalls).toContain('light')
        const savedLocal = JSON.parse(await readFile(localConfigPath, 'utf8'))
        expect(savedLocal.appearance?.theme).toBe('light')
      },
    )
  })

  test.describe('When there are errors in local settings', () => {
    test(
      'should show inline error markers in the local settings file and display the error in the LGTM IDE diagnostics buffer',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const globalConfigPath = join(workspace.lgtmHomeDir, 'config.json')
        const localConfigDir = join(workspace.root, '.lgtm')
        const localConfigPath = join(localConfigDir, 'config.json')
        await mkdir(localConfigDir, { recursive: true })
        await writeFile(
          globalConfigPath,
          JSON.stringify({ $schema: SCHEMA_URL, appearance: { theme: 'dark' } }, null, 2) + '\n',
          'utf8',
        )
        await writeFile(
          localConfigPath,
          JSON.stringify({ $schema: SCHEMA_URL, appearance: { theme: 'light' } }, null, 2) + '\n',
          'utf8',
        )
        const lspButton = page.getByRole('button', { name: 'LSP status', exact: true })
        const tablist = page.getByRole('tablist', { name: 'Open editors' })
        await page.keyboard.press('Meta+Shift+,')

        // Act
        const editorContent = page.locator('.cm-content')
        await editorContent.click()
        await page.keyboard.press('ControlOrMeta+a')
        await page.keyboard.insertText(
          '{\n  "appearance": {\n    "unknownLocalField": true\n  }\n}\n',
        )

        // Assert: inline error marker appears in local settings and LSP status reports 1 error
        await expect(page.locator('.cm-lintRange-error').first()).toBeVisible()
        await expect(lspButton).toContainText('LSP: json (ready, 1 error)')

        // Act: open LGTM IDE diagnostics buffer
        await lspButton.click()
        const menu = page.getByRole('menu', { name: 'LSP servers and diagnostics' })
        await expect(menu).toBeVisible()
        await menu.getByRole('menuitem', { name: 'Open diagnostics buffer', exact: true }).click()

        // Assert: error message is shown in the diagnostics buffer tab
        await expect(
          tablist.getByRole('tab', { name: 'LSP Diagnostics', exact: true }),
        ).toHaveAttribute('aria-selected', 'true')
        await expect(page.locator('.cm-content')).toContainText('# LSP Diagnostics')
        await expect(page.locator('.cm-content')).toContainText('.lgtm/config.json:3:5 [ERROR]')
        await expect(page.locator('.cm-content')).toContainText('unknownLocalField')
      },
    )
  })
})
