import { describe, expect, test } from 'bun:test'
import { buildDiffFileTree } from './diffService.svelte'
import { createKeybindingDispatcher } from '../keybindings'

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

describe('Given the centralized keybinding dispatcher / When pressing diff and review shortcuts', () => {
  test('should invoke matching handlers for Alt shortcuts and bracket chords', () => {
    // Arrange
    const triggered: string[] = []
    const dispatch = createKeybindingDispatcher({
      'diff.nextFile': () => triggered.push('nextFile'),
      'diff.prevFile': () => triggered.push('prevFile'),
      'diff.toggleViewMode': () => triggered.push('toggleViewMode'),
      'diff.toggleFullFile': () => triggered.push('toggleFullFile'),
      'review.toggleMode': () => triggered.push('toggleReviewMode'),
    })

    const makeKeyEvent = (key: string, code = key, altKey = false): KeyboardEvent =>
      ({
        key,
        code,
        altKey,
        metaKey: false,
        ctrlKey: false,
        target: null,
        preventDefault: () => {},
      }) as unknown as KeyboardEvent

    // Act
    dispatch(makeKeyEvent('m', 'KeyM', true))
    dispatch(makeKeyEvent('f', 'KeyF', true))
    dispatch(makeKeyEvent('r', 'KeyR', true))
    dispatch(makeKeyEvent(']'))
    dispatch(makeKeyEvent('f'))
    dispatch(makeKeyEvent('['))
    dispatch(makeKeyEvent('f'))

    // Assert
    expect(triggered).toEqual([
      'toggleViewMode',
      'toggleFullFile',
      'toggleReviewMode',
      'nextFile',
      'prevFile',
    ])
  })
})
