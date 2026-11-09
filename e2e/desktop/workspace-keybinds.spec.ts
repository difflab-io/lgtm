import {
  test,
  expect,
  entry,
  createFile,
  renameEntry,
  exists,
  modeBadge,
  whichKeyPanel,
  whichKeyExplorer,
  commandMenu,
  commandMenuInput,
  commandPalette,
  commandPaletteInput,
} from './fixtures'
import { readFile, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

// Tests -----------------------------------------------------------------------
test.describe('Given an existing workspace', { tag: ['@desktop', '@keybinds'] }, () => {
  test.use({ initialWorkspace: 'existing' })

  test.describe('When navigating panes and the file tree with modal keybindings', () => {
    test(
      'should switch focus between workspace and editor panes with Ctrl-w l and Ctrl-w h',
      { tag: '@smoke' },
      async ({ page }) => {
        // Arrange
        await entry(page, 'same.md').click()
        await expect(page.locator('.cm-content')).toContainText('# Markdown')
        const sidebarPane = page.locator('[data-pane-id="workspace"]')
        const editorPane = page.locator('[data-pane-id="editor"]')

        // Act: focus workspace pane with Ctrl-w h
        await page.keyboard.press('Escape')
        await page.keyboard.press('Control+w')
        await page.keyboard.press('h')

        // Assert
        await expect(sidebarPane).toHaveAttribute('data-pane-active', 'true')
        await expect(editorPane).toHaveAttribute('data-pane-active', 'false')

        // Act: focus editor pane with Ctrl-w l
        await page.keyboard.press('Control+w')
        await page.keyboard.press('l')

        // Assert
        await expect(editorPane).toHaveAttribute('data-pane-active', 'true')
        await expect(sidebarPane).toHaveAttribute('data-pane-active', 'false')
        await expect(page.locator('.cm-content')).toBeFocused()
      },
    )

    test(
      'should navigate and open files in the workspace tree using j, k, l, h, and Enter',
      { tag: '@smoke' },
      async ({ page }) => {
        // Arrange
        const drafts = entry(page, 'Drafts')
        const draftsControl = drafts.getByRole('button', { name: 'Drafts', exact: true })
        await draftsControl.focus()

        // Act: expand Drafts with l, move into Drafts/one.md with l, and open with Enter
        await page.keyboard.press('l')
        await expect(drafts).toHaveAttribute('aria-expanded', 'true')
        await page.keyboard.press('l')
        await expect(entry(page, 'Drafts/one.md')).toBeFocused()
        await page.keyboard.press('j')
        await expect(entry(page, 'Drafts/two.md')).toBeFocused()
        await page.keyboard.press('k')
        await expect(entry(page, 'Drafts/one.md')).toBeFocused()
        await page.keyboard.press('Enter')

        // Assert
        await expect(page.locator('.cm-content')).toContainText('# One')

        // Act: return to tree and collapse Drafts with h
        await entry(page, 'Drafts/one.md').focus()
        await page.keyboard.press('h')
        await expect(draftsControl).toBeFocused()
        await page.keyboard.press('h')

        // Assert
        await expect(drafts).toHaveAttribute('aria-expanded', 'false')
      },
    )

    test(
      'should create, rename, and delete entries in the workspace tree using a, r, and d',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const sameNode = entry(page, 'same.md')
        await sameNode.focus()

        // Act: create new file with a
        await page.keyboard.press('a')

        // Assert
        await expect(entry(page, 'Untitled.md')).toBeVisible()
        expect(await exists(join(workspace.root, 'Untitled.md'))).toBe(true)

        // Act: rename Untitled.md with r
        await entry(page, 'Untitled.md').focus()
        await page.keyboard.press('r')
        const renameInput = page.getByRole('textbox', { name: 'Name', exact: true })
        await expect(renameInput).toBeFocused()
        await renameInput.fill('KeybindCreated.md')
        await renameInput.press('Enter')

        // Assert
        await expect(entry(page, 'KeybindCreated.md')).toBeVisible()
        expect(await exists(join(workspace.root, 'KeybindCreated.md'))).toBe(true)

        // Act: delete KeybindCreated.md with d
        await entry(page, 'KeybindCreated.md').focus()
        await page.keyboard.press('d')

        // Assert
        await expect(entry(page, 'KeybindCreated.md')).toHaveCount(0)
        expect(await exists(join(workspace.root, 'KeybindCreated.md'))).toBe(false)
      },
    )
  })
})

test.describe('Given an open file in the code editor', { tag: ['@desktop', '@keybinds'] }, () => {
  test.use({ initialWorkspace: 'existing' })

  test.describe('When editing with Helix, Vim, and Kakoune modal keybindings', () => {
    test(
      'should start in normal mode with a block cursor and show helix · normal in the status bar',
      { tag: '@smoke' },
      async ({ page }) => {
        // Arrange
        await entry(page, 'Drafts').getByRole('button', { name: 'Drafts', exact: true }).focus()

        // Act
        await page.keyboard.press('l')
        await page.keyboard.press('l')
        await page.keyboard.press('Enter')

        // Assert
        const badge = modeBadge(page)
        await expect(badge).toBeVisible()
        await expect(badge).toHaveAttribute('data-mode', 'normal')
        await expect(badge).toHaveAttribute('data-preset', 'helix')
        await expect(badge).toContainText('helix · normal')
        await expect(page.locator('.cm-editor')).toHaveAttribute('data-modal-mode', 'normal')
      },
    )

    test(
      'should move by words and lines, select text objects (miw), and execute d, y, p, and u operators',
      { tag: '@smoke' },
      async ({ page }) => {
        // Arrange
        await entry(page, 'notes.txt').click()
        const editorContent = page.locator('.cm-content')
        await expect(editorContent).toContainText('Plain text note.')
        await editorContent.focus()
        await page.keyboard.press('Escape')
        await expect(modeBadge(page)).toHaveAttribute('data-mode', 'normal')

        // Act: go to start of document (gg), select inner word (miw), and delete (d)
        await page.keyboard.press('g')
        await page.keyboard.press('g')
        await page.keyboard.press('m')
        await page.keyboard.press('i')
        await page.keyboard.press('w')
        await page.keyboard.press('d')

        // Assert: "Plain" was deleted
        await expect(editorContent).toContainText(' text note.')

        // Act: undo (u) to restore "Plain", then paste (P) before
        await page.keyboard.press('u')
        await expect(editorContent).toContainText('Plain text note.')
        await page.keyboard.press('P')

        // Assert
        await expect(editorContent).toContainText('PlainPlain text note.')
      },
    )

    test(
      'should add multiple cursors with C and collapse to the primary cursor with comma',
      { tag: '@smoke' },
      async ({ page }) => {
        // Arrange
        await entry(page, 'Drafts').getByRole('button', { name: 'Drafts', exact: true }).click()
        await entry(page, 'Drafts/one.md').click()
        const editorContent = page.locator('.cm-content')
        await expect(editorContent).toContainText('# One')
        await editorContent.focus()
        await page.keyboard.press('Escape')

        // Act: go to top (gg), add secondary cursors below (C twice), then keep primary cursor (,)
        await page.keyboard.press('g')
        await page.keyboard.press('g')
        await page.keyboard.press('Shift+C')
        await page.keyboard.press('Shift+C')

        // Assert: secondary cursor decorations exist before collapsing
        await expect(page.locator('.cm-cursor-secondary').first()).toBeVisible()

        // Act: collapse to primary cursor with ,
        await page.keyboard.press(',')

        // Assert
        await expect(page.locator('.cm-cursor-secondary')).toHaveCount(0)
      },
    )

    test(
      'should enter insert mode on i, a, and o and return to normal mode on Escape',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        await entry(page, 'same.md').click()
        const editorContent = page.locator('.cm-content')
        await expect(editorContent).toContainText('# Markdown')
        await editorContent.focus()
        await page.keyboard.press('Escape')
        await expect(modeBadge(page)).toHaveAttribute('data-mode', 'normal')

        // Act: open line below with o, type text in insert mode, and return to normal mode on Escape
        await page.keyboard.press('o')
        await expect(modeBadge(page)).toHaveAttribute('data-mode', 'insert')
        await page.keyboard.type('Modal line.')
        await page.keyboard.press('Escape')

        // Assert
        await expect(modeBadge(page)).toHaveAttribute('data-mode', 'normal')
        await expect(editorContent).toContainText('Modal line.')
        await expect
          .poll(() => readFile(join(workspace.root, 'same.md'), 'utf8'))
          .toContain('Modal line.')
      },
    )

    test(
      'should trigger LSP hover with K, navigate diagnostics with ]d and [d, show signature help, and complete with Ctrl+Space',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        await createFile(page)
        await renameEntry(page, 'Untitled.md', 'intellisense.rs')
        await entry(page, 'same.md').click()
        await writeFile(
          join(workspace.root, 'intellisense.rs'),
          'pub fn helper(x: i32, y: i32) -> i32 {\n    BROKEN();\n    42\n}\n',
          'utf8',
        )
        await entry(page, 'intellisense.rs').click()
        const editorContent = page.locator('.cm-content')
        await expect(page.locator('.cm-lintRange-error')).toBeVisible()
        await editorContent.focus()
        await page.keyboard.press('Escape')
        await expect(modeBadge(page)).toHaveAttribute('data-mode', 'normal')

        // Act: press K in normal mode to open LSP hover tooltip at cursor, then also verify Helix Space k
        await page.keyboard.press('g')
        await page.keyboard.press('g')
        await page.keyboard.press('Shift+K')

        // Assert: hover tooltip is visible and Escape dismisses it in normal mode
        const hoverTooltip = page.locator('[data-slot="code-editor-hover"]')
        await expect(hoverTooltip).toBeVisible()
        await expect(hoverTooltip).toContainText('pub fn helper() -> i32')
        await page.keyboard.press('Escape')
        await expect(hoverTooltip).toBeHidden()

        // Act: press Space k (Helix hover) to open LSP hover tooltip again
        await page.keyboard.press('Space')
        await page.keyboard.press('k')
        await expect(hoverTooltip).toBeVisible()
        await page.keyboard.press('Escape')
        await expect(hoverTooltip).toBeHidden()

        // Act: press ]d to jump to next diagnostic and show diagnostic tooltip
        await page.keyboard.press(']')
        await page.keyboard.press('d')

        // Assert
        const diagTooltip = page.locator('[data-slot="code-editor-diagnostic-tooltip"]')
        await expect(diagTooltip).toBeVisible()
        await expect(diagTooltip).toContainText('cannot find value BROKEN in this scope')
        await page.keyboard.press('Escape')
        await expect(diagTooltip).toBeHidden()

        // Act: press Space l s to open signature help at cursor
        await page.keyboard.press('Space')
        await page.keyboard.press('l')
        await page.keyboard.press('s')

        // Assert
        const sigTooltip = page.locator('[data-slot="code-editor-signature-help"]')
        await expect(sigTooltip).toBeVisible()
        await expect(sigTooltip).toContainText('fn helper(x: i32, y: i32) -> i32')
        await page.keyboard.press('Escape')
        await expect(sigTooltip).toBeHidden()

        // Act: open line below (o) into insert mode, trigger completion with Control+Space, dismiss with Control+e while staying in insert mode, then accept with Control+y and return to normal mode on Escape
        await page.keyboard.press('o')
        await expect(modeBadge(page)).toHaveAttribute('data-mode', 'insert')
        await page.keyboard.type('hel')
        await page.keyboard.press('Control+Space')
        const completionListbox = page.locator('.cm-tooltip-autocomplete [role="listbox"]')
        await expect(completionListbox).toBeVisible()
        await expect(completionListbox).toContainText('helper_value')

        // Pressing Escape or Control+e dismisses autocomplete while keeping insert mode active
        await page.keyboard.press('Escape')
        await expect(completionListbox).toBeHidden()
        await expect(modeBadge(page)).toHaveAttribute('data-mode', 'insert')

        // Re-open autocomplete, dismiss with Control+e (Vim Compl-cancel), then re-open and accept with Control+y
        await page.keyboard.press('Control+Space')
        await expect(completionListbox).toBeVisible()
        await page.keyboard.press('Control+e')
        await expect(completionListbox).toBeHidden()
        await expect(modeBadge(page)).toHaveAttribute('data-mode', 'insert')

        await page.keyboard.press('Control+Space')
        await expect(completionListbox).toBeVisible()
        await page.keyboard.press('Control+y')
        await expect(editorContent).toContainText('helper_value()')
        await page.keyboard.press('Escape')
        await expect(modeBadge(page)).toHaveAttribute('data-mode', 'normal')
      },
    )
  })
})

test.describe('Given WhichKey and the command surfaces', { tag: ['@desktop', '@keybinds'] }, () => {
  test.use({ initialWorkspace: 'existing' })

  test.describe('When running commands and switching commandStyle', () => {
    test(
      'should open WhichKey on Space, execute Space f s and Space e, and open WhichKeyExplorer on ?',
      { tag: '@smoke' },
      async ({ page }) => {
        // Arrange
        await entry(page, 'same.md').click()
        await page.locator('.cm-content').focus()
        await page.keyboard.press('Escape')

        // Act: press Space to open WhichKey popup
        await page.keyboard.press('Space')

        // Assert
        await expect(whichKeyPanel(page)).toBeVisible()
        await expect(whichKeyPanel(page)).toContainText('File')

        // Act: press f then s to save file and close WhichKey
        await page.keyboard.press('f')
        await page.keyboard.press('s')

        // Assert
        await expect(whichKeyPanel(page)).toBeHidden()

        // Act: press Space then e to toggle sidebar closed
        await page.keyboard.press('Space')
        await page.keyboard.press('e')

        // Assert
        await expect
          .poll(async () => (await page.locator('.document-pane').boundingBox())?.x)
          .toBe(0)

        // Act: press Space then e to toggle sidebar open again
        await page.keyboard.press('Space')
        await page.keyboard.press('e')

        // Assert
        await expect
          .poll(async () => (await page.locator('.document-pane').boundingBox())?.x ?? 0)
          .toBeGreaterThan(0)

        // Act: press ? to open WhichKeyExplorer
        await page.keyboard.press('?')

        // Assert
        await expect(whichKeyExplorer(page)).toBeVisible()

        // Act: press Escape to close WhichKeyExplorer
        await page.keyboard.press('Escape')

        // Assert
        await expect(whichKeyExplorer(page)).toBeHidden()
      },
    )

    test(
      'should open CommandMenu by default on : and Mod+k and switch Mod+k to CommandPalette when commandStyle is palette',
      { tag: '@smoke' },
      async ({ page }) => {
        // Arrange
        await entry(page, 'same.md').click()
        await page.locator('.cm-content').focus()
        await page.keyboard.press('Escape')
        await expect(page.getByRole('button', { name: /Open Command/i })).toHaveCount(0)
        expect(
          await page.evaluate(
            () => document.documentElement.scrollHeight <= document.documentElement.clientHeight,
          ),
        ).toBe(true)

        // Act: press : in normal mode to open CommandMenu
        await page.keyboard.press(':')

        // Assert
        await expect(commandMenu(page)).toBeVisible()
        await expect(commandMenu(page)).toHaveAttribute('data-size', 'sm')
        await expect(
          commandMenu(page)
            .locator('[data-testid="command-menu-item"][data-value="write"]')
            .getByTestId('command-menu-item-aliases'),
        ).toHaveText('w')
        expect(
          await page.evaluate(
            () => document.documentElement.scrollHeight <= document.documentElement.clientHeight,
          ),
        ).toBe(true)

        // Act: close CommandMenu with Escape
        await page.keyboard.press('Escape')

        // Assert
        await expect(commandMenu(page)).toBeHidden()

        // Act: press Meta+k to open default CommandMenu and switch command-style to palette
        await page.keyboard.press('Meta+k')
        await expect(commandMenu(page)).toBeVisible()
        await commandMenuInput(page).fill('command-style palette')
        await commandMenuInput(page).press('Enter')

        // Assert
        await expect(commandMenu(page)).toBeHidden()

        // Act: press Meta+k again
        await page.keyboard.press('Meta+k')

        // Assert: Meta+k now opens CommandPalette
        await expect(commandPalette(page)).toBeVisible()

        // Act: close CommandPalette and press :
        await commandPaletteInput(page).press('Escape')
        await expect(commandPalette(page)).toBeHidden()
        await page.keyboard.press(':')

        // Assert: : still opens CommandMenu
        await expect(commandMenu(page)).toBeVisible()
        await page.keyboard.press('Escape')
        await expect(commandMenu(page)).toBeHidden()
      },
    )
  })
})
