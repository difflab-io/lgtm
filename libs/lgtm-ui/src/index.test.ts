import { describe, expect, test } from 'bun:test'
import { EditorSelection, EditorState } from '@codemirror/state'
import {
  codeEditorLspExtension,
  codeEditorThemeExtension,
  lspOptionsFacet,
  modalClipboardField,
  modalEditorExtension,
  modalOptionsFacet,
  modalStateField,
  resolveEditorSymbolOffset,
  setModalClipboardEffect,
  setModalStateEffect,
} from './code-editor/modalKeymap'
import { codeEditorHoverTooltipVariants, codeEditorRootVariants } from './code-editor/styles'
import { contextMenuItemVariants } from './context-menu/styles'
import { menuIndicatorVariants, menuItemVariants } from './menu/styles'
import { tabsItemContainerVariants } from './tabs/styles'
import { themeToggleVariants } from './theme-toggle/styles'
import { treeViewMessageVariants, treeViewRowVariants } from './tree-view/styles'

describe('When importing lgtm-ui Helical style variants', () => {
  test('should build orthogonal CVA classes for TreeView, ContextMenu, CodeEditor, Tabs, Menu, and ThemeToggle', () => {
    // Arrange
    const errorVariant = { variant: 'error' as const }
    const destructiveItem = { size: 'sm' as const, destructive: true }
    const smallEditor = { size: 'sm' as const }

    // Act
    const rowClass = treeViewRowVariants()
    const messageClass = treeViewMessageVariants(errorVariant)
    const itemClass = contextMenuItemVariants(destructiveItem)
    const editorClass = codeEditorRootVariants(smallEditor)
    const hoverClass = codeEditorHoverTooltipVariants()
    const tabItemClass = tabsItemContainerVariants({ size: 'sm' })
    const menuItemClass = menuItemVariants({ size: 'sm' })
    const statusDotClass = menuIndicatorVariants({ tone: 'active-page' })
    const themeToggleClass = themeToggleVariants({ size: 'sm', variant: 'ghost' })

    // Assert
    expect(rowClass).toContain('helical-tree-row')
    expect(messageClass).toContain('text-destructive')
    expect(itemClass).toContain('text-destructive')
    expect(editorClass).toContain('helical-code-editor')
    expect(hoverClass).toContain('helical-code-editor-hover')
    expect(tabItemClass).toContain('border-r')
    expect(menuItemClass).toContain('min-h-5.5')
    expect(menuItemClass).toContain('rounded-none')
    expect(statusDotClass).toContain('bg-emerald-500')
    expect(themeToggleClass).toContain('helical-theme-toggle')
  })
})

describe('When composing CodeEditor CodeMirror 6 extensions', () => {
  test('should initialize facets and modal state field and update modal state via effects', () => {
    // Arrange
    const onhover = async () => null
    const state = EditorState.create({
      doc: 'fn main() {}\n',
      extensions: [
        codeEditorThemeExtension(),
        modalEditorExtension({ clickToInsert: false, readOnly: true }),
        codeEditorLspExtension({ onhover }),
      ],
    })

    // Act
    const updated = state.update({
      effects: [
        setModalStateEffect.of({ mode: 'select', preset: 'vim' }),
        setModalClipboardEffect.of('fn main() {}'),
      ],
    }).state

    // Assert
    expect(state.field(modalStateField)).toEqual({ mode: 'normal', preset: 'helix' })
    expect(state.field(modalClipboardField)).toBe('')
    expect(state.facet(modalOptionsFacet)).toEqual({ clickToInsert: false, readOnly: true })
    expect(state.facet(lspOptionsFacet).onhover).toBe(onhover)
    expect(updated.field(modalStateField)).toEqual({ mode: 'select', preset: 'vim' })
    expect(updated.field(modalClipboardField)).toBe('fn main() {}')
  })

  test('should resolve the symbol offset inside a forward word selection in selection-first presets', () => {
    // Arrange
    const state = EditorState.create({
      doc: 'let total = helper(1, 2);\n',
      selection: EditorSelection.single(12, 18),
    })

    // Act
    const offset = resolveEditorSymbolOffset(state)

    // Assert
    expect(offset).toBe(12)
  })
})
