import { describe, expect, test } from 'bun:test'
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
    expect(menuItemClass).toContain('min-h-6.5')
    expect(statusDotClass).toContain('bg-emerald-500')
    expect(themeToggleClass).toContain('helical-theme-toggle')
  })
})
