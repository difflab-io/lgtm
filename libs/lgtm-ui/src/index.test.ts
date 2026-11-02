import { describe, expect, test } from 'bun:test'
import { codeEditorRootVariants } from './code-editor/styles'
import { contextMenuItemVariants } from './context-menu/styles'
import { treeViewMessageVariants, treeViewRowVariants } from './tree-view/styles'

describe('When importing lgtm-ui Helical style variants', () => {
  test('should build orthogonal CVA classes for TreeView, ContextMenu, and CodeEditor', () => {
    expect(treeViewRowVariants()).toContain('helical-tree-row')
    expect(treeViewMessageVariants({ variant: 'error' })).toContain('text-destructive')
    expect(contextMenuItemVariants({ size: 'sm', destructive: true })).toContain('text-destructive')
    expect(codeEditorRootVariants({ size: 'sm' })).toContain('helical-code-editor')
  })
})
