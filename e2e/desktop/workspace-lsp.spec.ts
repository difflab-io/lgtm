import { test, expect, entry, createFile, renameEntry, exists } from './fixtures'
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

const SUPPORTED_LSP_CASES = [
  {
    languageId: 'rust',
    fileName: 'sample.rs',
    source: 'pub fn check() {\n    BROKEN();\n}\n',
  },
  {
    languageId: 'typescript',
    fileName: 'sample.ts',
    source: 'export function check(): void {\n  BROKEN();\n}\n',
  },
  {
    languageId: 'javascript',
    fileName: 'sample.js',
    source: 'export function check() {\n  BROKEN();\n}\n',
  },
  {
    languageId: 'python',
    fileName: 'sample.py',
    source: 'def check():\n    BROKEN()\n',
  },
  {
    languageId: 'go',
    fileName: 'sample.go',
    source: 'package main\nfunc check() {\n\tBROKEN()\n}\n',
  },
  {
    languageId: 'markdown',
    fileName: 'sample.md',
    source: '# Check\n\nBROKEN\n',
  },
] as const

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
        const menu = page.getByRole('menu', { name: 'LSP servers and diagnostics' })
        await expect(menu).toBeVisible()
        await expect(menu).toContainText('python')
        await expect(menu).toContainText('typescript')
        await menu.getByRole('menuitem', { name: 'Open diagnostics buffer', exact: true }).click()

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

        // Act: hover over symbol to verify formatted Markdown tooltip and bounded placement
        await page
          .locator('.cm-line')
          .first()
          .hover({ position: { x: 40, y: 8 } })
        const hoverTooltip = page.locator('[data-slot="code-editor-hover"]')
        await expect(hoverTooltip).toBeVisible()
        await expect(hoverTooltip.locator('pre code[data-language="rust"]')).toHaveText(
          'pub fn helper() -> i32',
        )
        await expect(hoverTooltip.locator('strong')).toHaveText('workspace')
        await expect(hoverTooltip.locator('p code')).toHaveText('helper')
        const scrollerBox = await page.locator('.cm-scroller').boundingBox()
        const tooltipBox = await page.locator('.cm-tooltip-hover').boundingBox()
        expect(scrollerBox).not.toBeNull()
        expect(tooltipBox).not.toBeNull()
        expect(tooltipBox!.y).toBeGreaterThanOrEqual(scrollerBox!.y)
        expect(tooltipBox!.y + tooltipBox!.height).toBeLessThanOrEqual(
          scrollerBox!.y + scrollerBox!.height,
        )

        // Act
        await lspButton.click()
        const menu = page.getByRole('menu', { name: 'LSP servers and diagnostics' })
        await expect(menu).toBeVisible()
        const rustTriggerItem = menu.locator('[data-item-value="server:rust"]')
        await expect(rustTriggerItem.locator('[data-slot="lsp-server-errors"]')).toHaveText('1')
        await rustTriggerItem.hover()
        await expect(rustTriggerItem).toHaveAttribute('data-highlighted', '')
        const rustSubmenu = page.getByRole('menu', { name: 'rust actions' })
        await expect(rustSubmenu).toBeVisible()
        await rustSubmenu
          .getByRole('menuitem', { name: 'View rust diagnostics', exact: true })
          .click()

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
      },
    )

    for (const { languageId, fileName, source } of SUPPORTED_LSP_CASES) {
      test(`should activate ${languageId} language server and report diagnostics for ${fileName}`, async ({
        page,
        workspace,
      }) => {
        // Arrange
        await createFile(page)
        await renameEntry(page, 'Untitled.md', fileName)
        await entry(page, 'same.md').click()
        await writeFile(join(workspace.root, fileName), source, 'utf8')

        // Act
        await entry(page, fileName).click()

        // Assert
        const lspButton = page.getByRole('button', { name: 'LSP status', exact: true })
        await expect(lspButton).toContainText(`LSP: ${languageId} (ready, 1 error)`)
        await expect(page.locator('.cm-lintRange-error')).toBeVisible()
      })
    }
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

  test.describe('When toggling the color theme from the titlebar', () => {
    test.use({ colorScheme: 'dark' })

    test(
      'should launch in dark mode when the OS is set to dark and cycle through system, light, and dark states',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const systemButton = page.getByRole('button', { name: 'Theme: system', exact: true })
        const darkSurface = 'rgb(16, 16, 16)'

        // Assert: launches directly in system dark mode without tauri.conf.json overriding OS theme
        expect(workspace.configuredWindowTheme).toBeNull()
        await expect(systemButton).toBeVisible()
        await expect(systemButton).toHaveAttribute('data-theme', 'system')
        await expect(page.locator('html')).toHaveClass(/theme-lgtm/)
        await expect(page.locator('html')).toHaveClass(/dark/)
        await expect(page.locator('.titlebar')).toHaveCSS('background-color', darkSurface)
        await expect(page.locator('.workspace-sidebar')).toHaveCSS('background-color', darkSurface)
        await expect.poll(() => workspace.setThemeCalls).toEqual([null])

        await entry(page, 'same.md').click()
        await expect(page.locator('.cm-gutters')).toHaveCSS('background-color', darkSurface)

        // Act: cycle system -> light
        await systemButton.click()

        // Assert: light mode active and synced to Tauri window
        const lightButton = page.getByRole('button', { name: 'Theme: light', exact: true })
        await expect(lightButton).toBeVisible()
        await expect(lightButton).toHaveAttribute('data-theme', 'light')
        await expect(page.locator('html')).not.toHaveClass(/dark/)
        await expect(page.locator('.cm-gutters')).not.toHaveCSS('background-color', darkSurface)
        await expect.poll(() => workspace.setThemeCalls).toEqual([null, 'light'])

        // Act: cycle light -> dark
        await lightButton.click()

        // Assert: dark mode active and synced to Tauri window
        const darkButton = page.getByRole('button', { name: 'Theme: dark', exact: true })
        await expect(darkButton).toBeVisible()
        await expect(darkButton).toHaveAttribute('data-theme', 'dark')
        await expect(page.locator('html')).toHaveClass(/dark/)
        await expect(page.locator('.cm-gutters')).toHaveCSS('background-color', darkSurface)
        await expect.poll(() => workspace.setThemeCalls).toEqual([null, 'light', 'dark'])

        // Act: cycle dark -> system (with OS set to light)
        await page.emulateMedia({ colorScheme: 'light' })
        await darkButton.click()

        // Assert: back in system mode following OS light preference
        await expect(systemButton).toBeVisible()
        await expect(systemButton).toHaveAttribute('data-theme', 'system')
        await expect(page.locator('html')).not.toHaveClass(/dark/)
        await expect.poll(() => workspace.setThemeCalls).toEqual([null, 'light', 'dark', null])
        await expect
          .poll(async () => {
            const raw = await readFile(join(workspace.lgtmHomeDir, 'config.json'), 'utf8').catch(
              () => '',
            )
            return raw.includes('"theme": "system"')
          })
          .toBe(true)
      },
    )

    test(
      'should route theme updates to project config when appearance.theme is already overridden in project config without clobbering sibling sections',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const globalConfigPath = join(workspace.lgtmHomeDir, 'config.json')
        const projectConfigDir = join(workspace.root, '.lgtm')
        const projectConfigPath = join(projectConfigDir, 'config.json')
        await mkdir(projectConfigDir, { recursive: true })
        await writeFile(
          globalConfigPath,
          JSON.stringify(
            {
              $schema: './schemas/config.schema.json',
              appearance: { theme: 'dark' },
              editor: { lineWrapping: false },
            },
            null,
            2,
          ) + '\n',
          'utf8',
        )
        await writeFile(
          projectConfigPath,
          JSON.stringify(
            {
              $schema: './schemas/config.schema.json',
              appearance: { theme: 'light' },
            },
            null,
            2,
          ) + '\n',
          'utf8',
        )

        // Act
        await page.reload()
        const lightButton = page.getByRole('button', { name: 'Theme: light', exact: true })
        await expect(lightButton).toBeVisible()
        await entry(page, 'same.md').click()
        await expect(page.locator('.cm-content')).not.toHaveClass(/cm-lineWrapping/)
        await lightButton.click()

        // Assert
        const darkButton = page.getByRole('button', { name: 'Theme: dark', exact: true })
        await expect(darkButton).toBeVisible()
        await expect
          .poll(async () => {
            const projectRaw = await readFile(projectConfigPath, 'utf8')
            return JSON.parse(projectRaw).appearance?.theme
          })
          .toBe('dark')
        const globalSaved = JSON.parse(await readFile(globalConfigPath, 'utf8'))
        expect(globalSaved.editor?.lineWrapping).toBe(false)
      },
    )
  })

  test.describe('When pressing Cmd+, to edit nested config.json or config.yaml in CodeEditor', () => {
    test(
      'should open global config with Cmd+, validate nested schema diagnostics and hover documentation, and live-apply JSON and YAML edits',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const lspButton = page.getByRole('button', { name: 'LSP status', exact: true })
        const tablist = page.getByRole('tablist', { name: 'Open editors' })

        // Act: open Settings (config.json) via Control+,
        await page.keyboard.press('Control+,')

        // Assert
        await expect(
          tablist.getByRole('tab', { name: 'Settings (config.json)', exact: true }),
        ).toHaveAttribute('aria-selected', 'true')
        expect(await exists(join(workspace.lgtmHomeDir, 'schemas/config.schema.json'))).toBe(true)
        await expect(lspButton).toContainText('LSP: json (ready)')
        await expect(page.locator('.cm-content')).toContainText('"appearance"')
        await expect(page.locator('.cm-content')).toHaveClass(/cm-lineWrapping/)

        // Act: hover over "theme" property in config.json to inspect schema hover documentation
        const themeLine = page.locator('.cm-line', { hasText: '"theme"' }).first()
        await themeLine.hover({ position: { x: 40, y: 8 } })
        const hoverTooltip = page.locator('[data-slot="code-editor-hover"]')
        await expect(hoverTooltip).toBeVisible()
        await expect(hoverTooltip).toContainText('appearance.theme')

        // Act: replace editor content with invalid nested theme value
        const editorContent = page.locator('.cm-content')
        await editorContent.click()
        await page.keyboard.press('ControlOrMeta+a')
        await page.keyboard.insertText('{\n  "appearance": {\n    "theme": "neon"\n  }\n}\n')

        // Assert: schema diagnostic error squiggly appears
        await expect(lspButton).toContainText('LSP: json (ready, 1 error)')
        await expect(page.locator('.cm-lintRange-error')).toBeVisible()

        // Act: replace editor content with valid nested JSON disabling lineWrapping and setting dark theme
        await editorContent.click()
        await page.keyboard.press('ControlOrMeta+a')
        await page.keyboard.insertText(
          '{\n  "$schema": "./schemas/config.schema.json",\n  "appearance": {\n    "theme": "dark"\n  },\n  "editor": {\n    "lineWrapping": false\n  }\n}\n',
        )

        // Assert: diagnostics clear and live UI updates theme and lineWrapping
        await expect(lspButton).toContainText('LSP: json (ready)')
        await expect(page.locator('.cm-lintRange-error')).toHaveCount(0)
        await expect(page.getByRole('button', { name: 'Theme: dark', exact: true })).toBeVisible()
        await expect(page.locator('html')).toHaveClass(/dark/)
        await expect(page.locator('.cm-content')).not.toHaveClass(/cm-lineWrapping/)

        // Act: switch to YAML global config and reopen with Control+,
        await page
          .getByRole('button', { name: 'Close Settings (config.json)', exact: true })
          .click()
        await rm(join(workspace.lgtmHomeDir, 'config.json'), { force: true })
        await writeFile(
          join(workspace.lgtmHomeDir, 'config.yaml'),
          '# yaml-language-server: $schema=./schemas/config.schema.json\nappearance:\n  theme: light\neditor:\n  lineWrapping: true\n',
          'utf8',
        )
        await page.keyboard.press('Control+,')

        // Assert: YAML config opens and live-applies edits
        await expect(
          tablist.getByRole('tab', { name: 'Settings (config.yaml)', exact: true }),
        ).toHaveAttribute('aria-selected', 'true')
        await expect(lspButton).toContainText('LSP: yaml (ready)')
        await editorContent.click()
        await page.keyboard.press('ControlOrMeta+a')
        await page.keyboard.insertText(
          '# yaml-language-server: $schema=./schemas/config.schema.json\nappearance:\n  theme: dark\neditor:\n  lineWrapping: true\n',
        )
        await expect(page.getByRole('button', { name: 'Theme: dark', exact: true })).toBeVisible()
      },
    )
  })
})
