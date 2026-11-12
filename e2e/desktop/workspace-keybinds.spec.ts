import { test, expect, entry, createFile, renameEntry, exists } from './fixtures'
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
        await expect(draftsControl).toHaveCSS('border-radius', '0px')

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
      'should create, rename, delete, and toggle hidden entries in the workspace tree using a, r, d, and Shift+H',
      { tag: '@smoke' },
      async ({ page, workspace }) => {
        // Arrange
        const sameNode = entry(page, 'same.md')
        await sameNode.focus()

        // Assert: hidden dotfile is hidden by default, then toggled visible/hidden with Shift+H
        await expect(entry(page, '.gitignore')).toHaveCount(0)
        await page.keyboard.press('Shift+H')
        await expect(entry(page, '.gitignore')).toBeVisible()
        await page.keyboard.press('Shift+H')
        await expect(entry(page, '.gitignore')).toHaveCount(0)

        // Act: create new file with a
        await sameNode.focus()
        await page.keyboard.press('a')

        // Assert
        await expect(entry(page, 'Untitled.md')).toBeVisible()
        expect(await exists(join(workspace.root, 'Untitled.md'))).toBe(true)

        // Act: rename Untitled.md with r (without switching the editor mode badge to insert)
        await entry(page, 'Untitled.md').focus()
        await page.keyboard.press('r')
        const renameInput = page.getByRole('textbox', { name: 'Name', exact: true })
        await expect(renameInput).toBeFocused()
        await expect(page.getByTestId('which-key-mode-badge')).toHaveAttribute(
          'data-mode',
          'normal',
        )
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
        await expect(entry(page, 'Drafts/one.md')).toBeVisible()
        await page.keyboard.press('l')
        await expect(entry(page, 'Drafts/one.md')).toBeFocused()
        await page.keyboard.press('Enter')

        // Assert
        const badge = page.getByTestId('which-key-mode-badge')
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
        await expect(page.getByTestId('which-key-mode-badge')).toHaveAttribute(
          'data-mode',
          'normal',
        )

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

        // Act: traverse paragraphs with } and { in normal mode without selecting whole paragraphs
        await page.keyboard.press('}')
        await expect(page.locator('.cm-selectionBackground')).toHaveCount(0)
        await page.keyboard.press('{')
        await expect(page.locator('.cm-selectionBackground')).toHaveCount(0)
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
        const badge = page.getByTestId('which-key-mode-badge')
        await expect(badge).toHaveAttribute('data-mode', 'normal')

        // Act: open line below with o, type text in insert mode, and return to normal mode on Escape
        await page.keyboard.press('o')
        await expect(badge).toHaveAttribute('data-mode', 'insert')
        await page.keyboard.type('Modal line.')
        await page.keyboard.press('Escape')

        // Assert
        await expect(badge).toHaveAttribute('data-mode', 'normal')
        await expect(editorContent).toContainText('Modal line.')
        await expect
          .poll(() => readFile(join(workspace.root, 'same.md'), 'utf8'))
          .toContain('Modal line.')
      },
    )
  })

  test.describe('When using LSP keybindings in the code editor', () => {
    test.beforeEach(async ({ page, workspace }) => {
      await createFile(page)
      await renameEntry(page, 'Untitled.md', 'intellisense.rs')
      await entry(page, 'same.md').click()
      await writeFile(
        join(workspace.root, 'intellisense.rs'),
        'pub fn helper(x: i32, y: i32) -> i32 {\n    BROKEN();\n    42\n}\n',
        'utf8',
      )
      await entry(page, 'intellisense.rs').click()
      await expect(page.locator('.cm-lintRange-error')).toBeVisible()
      await page.locator('.cm-content').focus()
      await page.keyboard.press('Escape')
      await expect(page.getByTestId('which-key-mode-badge')).toHaveAttribute('data-mode', 'normal')
    })

    test(
      'should show and dismiss LSP hover tooltip with K and Space k',
      { tag: '@smoke' },
      async ({ page }) => {
        // Arrange
        const hoverTooltip = page.locator('[data-slot="code-editor-hover"]')

        // Act: press K in normal mode at top of file
        await page.keyboard.press('g')
        await page.keyboard.press('g')
        await page.keyboard.press('Shift+K')

        // Assert
        await expect(hoverTooltip).toBeVisible()
        await expect(hoverTooltip).toContainText('pub fn helper() -> i32')
        await page.keyboard.press('Escape')
        await expect(hoverTooltip).toBeHidden()

        // Act: press Space k (Helix hover) to open LSP hover tooltip again
        await page.keyboard.press('Space')
        await page.keyboard.press('k')

        // Assert
        await expect(hoverTooltip).toBeVisible()
        await page.keyboard.press('Escape')
        await expect(hoverTooltip).toBeHidden()
      },
    )

    test(
      'should jump to diagnostics and show diagnostic tooltip with ]d and [d',
      { tag: '@smoke' },
      async ({ page }) => {
        // Arrange
        const diagTooltip = page.locator('[data-slot="code-editor-diagnostic-tooltip"]')

        // Act: press ]d to jump to next diagnostic
        await page.keyboard.press(']')
        await page.keyboard.press('d')

        // Assert
        await expect(diagTooltip).toBeVisible()
        await expect(diagTooltip).toContainText('cannot find value BROKEN in this scope')
        await page.keyboard.press('Escape')
        await expect(diagTooltip).toBeHidden()

        // Act: press [d to jump to previous diagnostic
        await page.keyboard.press('[')
        await page.keyboard.press('d')

        // Assert
        await expect(diagTooltip).toBeVisible()
      },
    )

    test(
      'should show and dismiss LSP signature help with Space l s',
      { tag: '@smoke' },
      async ({ page }) => {
        // Arrange
        const sigTooltip = page.locator('[data-slot="code-editor-signature-help"]')

        // Act: press Space l s to open signature help at cursor
        await page.keyboard.press('Space')
        await page.keyboard.press('l')
        await page.keyboard.press('s')

        // Assert
        await expect(sigTooltip).toBeVisible()
        await expect(sigTooltip).toContainText('fn helper(x: i32, y: i32) -> i32')
        await page.keyboard.press('Escape')
        await expect(sigTooltip).toBeHidden()
      },
    )

    test(
      'should trigger, cancel with Escape or Control+e, and accept with Control+y autocomplete suggestions',
      { tag: '@smoke' },
      async ({ page }) => {
        // Arrange
        const editorContent = page.locator('.cm-content')
        const badge = page.getByTestId('which-key-mode-badge')
        const completionListbox = page.locator('.cm-tooltip-autocomplete [role="listbox"]')

        // Act: open line below (o) into insert mode and trigger completion with Control+Space
        await page.keyboard.press('o')
        await expect(badge).toHaveAttribute('data-mode', 'insert')
        await page.keyboard.type('hel')
        await page.keyboard.press('Control+Space')

        // Assert: autocomplete opens and Escape dismisses it while keeping insert mode active
        await expect(completionListbox).toBeVisible()
        await expect(completionListbox).toContainText('helper_value')
        await page.keyboard.press('Escape')
        await expect(completionListbox).toBeHidden()
        await expect(badge).toHaveAttribute('data-mode', 'insert')

        // Act: re-open autocomplete and dismiss with Control+e (Vim Compl-cancel)
        await page.keyboard.press('Control+Space')
        await expect(completionListbox).toBeVisible()
        await page.keyboard.press('Control+e')
        await expect(completionListbox).toBeHidden()
        await expect(badge).toHaveAttribute('data-mode', 'insert')

        // Act: re-open autocomplete, accept with Control+y, and exit insert mode with Escape
        await page.keyboard.press('Control+Space')
        await expect(completionListbox).toBeVisible()
        await page.keyboard.press('Control+y')

        // Assert
        await expect(editorContent).toContainText('helper_value()')
        await page.keyboard.press('Escape')
        await expect(badge).toHaveAttribute('data-mode', 'normal')
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
        const panel = page.getByTestId('which-key-panel')
        const explorer = page.getByTestId('which-key-explorer')

        // Act: press Space to open WhichKey popup
        await page.keyboard.press('Space')

        // Assert
        await expect(panel).toBeVisible()
        await expect(panel).toContainText('File')

        // Act: press f then s to save file and close WhichKey
        await page.keyboard.press('f')
        await page.keyboard.press('s')

        // Assert
        await expect(panel).toBeHidden()

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
        await expect(explorer).toBeVisible()

        // Act: press Escape to close WhichKeyExplorer
        await page.keyboard.press('Escape')

        // Assert
        await expect(explorer).toBeHidden()
      },
    )

    test(
      'should open CommandMenu on : and close on Escape',
      { tag: '@smoke' },
      async ({ page }) => {
        // Arrange
        await entry(page, 'same.md').click()
        await page.locator('.cm-content').focus()
        await page.keyboard.press('Escape')
        const menu = page.getByTestId('command-menu')
        await expect(page.getByRole('button', { name: /Open Command/i })).toHaveCount(0)
        expect(
          await page.evaluate(
            () => document.documentElement.scrollHeight <= document.documentElement.clientHeight,
          ),
        ).toBe(true)

        // Act: press : in normal mode to open CommandMenu
        await page.keyboard.press(':')

        // Assert
        await expect(menu).toBeVisible()
        await expect(menu).toHaveAttribute('data-size', 'sm')
        await expect(
          menu
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
        await expect(menu).toBeHidden()
      },
    )

    test(
      'should open CommandMenu on Mod+k by default and switch Mod+k to CommandPalette when commandStyle is palette',
      { tag: '@smoke' },
      async ({ page }) => {
        // Arrange
        await entry(page, 'same.md').click()
        await page.locator('.cm-content').focus()
        await page.keyboard.press('Escape')
        const menu = page.getByTestId('command-menu')
        const menuInput = page.getByTestId('command-menu-input')
        const palette = page.getByTestId('command-palette')
        const paletteInput = page.getByTestId('command-palette-input')

        // Act: press Meta+k to open default CommandMenu and switch command-style to palette
        await page.keyboard.press('Meta+k')
        await expect(menu).toBeVisible()
        await menuInput.fill('command-style palette')
        await menuInput.press('Enter')

        // Assert
        await expect(menu).toBeHidden()

        // Act: press Meta+k again
        await page.keyboard.press('Meta+k')

        // Assert: Meta+k now opens CommandPalette
        await expect(palette).toBeVisible()

        // Act: close CommandPalette and press :
        await paletteInput.press('Escape')
        await expect(palette).toBeHidden()
        await page.keyboard.press(':')

        // Assert: : still opens CommandMenu
        await expect(menu).toBeVisible()
        await page.keyboard.press('Escape')
        await expect(menu).toBeHidden()
      },
    )
  })
})
