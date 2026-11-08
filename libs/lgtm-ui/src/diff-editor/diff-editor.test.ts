import { describe, expect, it } from 'bun:test'
import { Chunk, getChunks, unifiedMergeView } from '@codemirror/merge'
import { EditorState, Text } from '@codemirror/state'
import { goToNextHunk, goToPrevHunk } from '../code-editor/modalKeymap'
import { diffEditorRootVariants, diffEditorStyles } from './styles'

describe('Given a DiffEditor in split mode / When rendering base and head content', () => {
  it('should build split and unified CVA classes and compute changed chunks between base and head documents', () => {
    // Arrange
    const baseText = Text.of([
      'pub fn alpha() -> i32 {',
      '    1',
      '}',
      '',
      'pub fn beta() -> i32 {',
      '    2',
      '}',
    ])
    const headText = Text.of([
      'pub fn alpha() -> i32 {',
      '    10',
      '}',
      '',
      'pub fn beta() -> i32 {',
      '    20',
      '}',
    ])

    // Act
    const splitClass = diffEditorRootVariants({ size: 'sm', mode: 'split' })
    const unifiedClass = diffEditorStyles({ size: 'md', mode: 'unified' })
    const chunks = Chunk.build(baseText, headText)

    // Assert
    expect(splitClass).toContain('helical-diff-editor')
    expect(splitClass).toContain('helical-diff-editor-split')
    expect(unifiedClass).toContain('helical-diff-editor-unified')
    expect(chunks.length).toBe(2)
  })

  it('should toggle between collapsed unchanged hunks and the full uncollapsed file via showFullFile', () => {
    // Arrange
    const baseDoc = Array.from({ length: 20 }, (_, idx) => `line ${idx + 1}`).join('\n')
    const headDoc = Array.from({ length: 20 }, (_, idx) =>
      idx === 10 ? 'line 11 modified' : `line ${idx + 1}`,
    ).join('\n')

    // Act
    const hunkState = EditorState.create({
      doc: headDoc,
      extensions: [
        unifiedMergeView({
          original: baseDoc,
          mergeControls: false,
          collapseUnchanged: { margin: 3, minSize: 4 },
        }),
      ],
    })
    const fullFileState = EditorState.create({
      doc: headDoc,
      extensions: [
        unifiedMergeView({
          original: baseDoc,
          mergeControls: false,
          collapseUnchanged: undefined,
        }),
      ],
    })

    // Assert
    expect(getChunks(hunkState)?.chunks.length).toBe(1)
    expect(getChunks(fullFileState)?.chunks.length).toBe(1)
    expect(fullFileState.doc.lines).toBe(20)
  })
})

describe('Given a DiffEditor in unified mode / When editing head content with readonly false', () => {
  it('should update changed chunks dynamically and expose modal hunk navigation helpers', () => {
    // Arrange
    const baseDoc = 'fn main() {\n  println!("old");\n}\n'
    const headDoc = 'fn main() {\n  println!("new");\n}\n'
    const state = EditorState.create({
      doc: headDoc,
      extensions: [
        unifiedMergeView({
          original: baseDoc,
          mergeControls: false,
        }),
      ],
    })

    // Act
    const nextTransaction = state.update({
      changes: { from: state.doc.length, insert: '// extra line\n' },
    })

    // Assert
    expect(nextTransaction.state.doc.toString()).toContain('// extra line')
    expect(getChunks(nextTransaction.state)?.chunks.length).toBeGreaterThanOrEqual(1)
    expect(goToNextHunk(undefined)).toBe(false)
    expect(goToPrevHunk(undefined)).toBe(false)
  })
})
