import { describe, expect, test } from 'bun:test'
import { buildDiffFileTree } from './diffService.svelte'

describe('Given a list of changed files / When building the hierarchical diff tree', () => {
  test('should compact single-child folder chains and expand all folder branches by default', () => {
    // Arrange
    const files = [
      {
        path: 'docs/architecture/diffs.md',
        oldPath: null,
        status: 'added' as const,
        additions: 3,
        deletions: 0,
      },
      {
        path: 'src/lib.rs',
        oldPath: null,
        status: 'modified' as const,
        additions: 1,
        deletions: 1,
      },
      {
        path: 'src/services/diff.rs',
        oldPath: null,
        status: 'added' as const,
        additions: 4,
        deletions: 0,
      },
    ]

    // Act
    const tree = buildDiffFileTree(files)

    // Assert
    expect(tree.folderIds).toEqual(['docs/architecture', 'src', 'src/services'])
    expect(tree.orderedPaths).toEqual([
      'docs/architecture/diffs.md',
      'src/services/diff.rs',
      'src/lib.rs',
    ])
    expect(tree.nodes[0]).toEqual({
      value: 'docs/architecture',
      label: 'docs/architecture',
      branch: true,
      children: [
        {
          value: 'docs/architecture/diffs.md',
          label: 'diffs.md',
        },
      ],
    })
  })
})
