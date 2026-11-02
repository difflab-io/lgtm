import { describe, expect, test } from 'bun:test'
import { WorkspaceNodeViewModel } from './workspaceNodeViewModel'

describe('When mapping WorkspaceNodes to TreeNodes', () => {
  test('should map directory and file nodes with expected branch and description fields', () => {
    // Arrange
    const folderModel = new WorkspaceNodeViewModel({
      path: 'src',
      name: 'src',
      kind: 'directory',
    })
    const fileModel = new WorkspaceNodeViewModel({
      path: 'src/lib.rs',
      name: 'lib.rs',
      kind: 'file',
    })

    // Act
    const folder = folderModel.toTreeNode()
    const file = fileModel.toTreeNode()

    // Assert
    expect(folder).toEqual({
      value: 'src',
      label: 'src',
      description: undefined,
      branch: true,
    })
    expect(file).toEqual({
      value: 'src/lib.rs',
      label: 'lib.rs',
      description: 'lib.rs',
      branch: false,
    })
  })
})
