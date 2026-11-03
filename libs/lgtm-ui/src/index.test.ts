import { describe, expect, test } from 'bun:test'
import { codeEditorRootVariants } from './code-editor/styles'
import { contextMenuItemVariants } from './context-menu/styles'
import { treeViewMessageVariants, treeViewRowVariants } from './tree-view/styles'

describe('When importing lgtm-ui Helical style variants', () => {
  test('should build orthogonal CVA classes for TreeView, ContextMenu, and CodeEditor', () => {
    // Arrange
    const errorVariant = { variant: 'error' as const }
    const destructiveItem = { size: 'sm' as const, destructive: true }
    const smallEditor = { size: 'sm' as const }

    // Act
    const rowClass = treeViewRowVariants()
    const messageClass = treeViewMessageVariants(errorVariant)
    const itemClass = contextMenuItemVariants(destructiveItem)
    const editorClass = codeEditorRootVariants(smallEditor)

    // Assert
    expect(rowClass).toContain('helical-tree-row')
    expect(messageClass).toContain('text-destructive')
    expect(itemClass).toContain('text-destructive')
    expect(editorClass).toContain('helical-code-editor')
  })
})
