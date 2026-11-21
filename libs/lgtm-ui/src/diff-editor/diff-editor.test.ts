import { describe, expect, it } from 'bun:test'
import { Chunk, getChunks, unifiedMergeView } from '@codemirror/merge'
import { EditorState, Text } from '@codemirror/state'
import { goToNextHunk, goToPrevHunk } from '../code-editor/modalKeymap'
import {
  isMarkdownTarget,
  toCodeMirrorDiagnostics,
  toLineWrappingExtension,
} from '../code-editor/shared'
import { diffEditorRootVariants } from './styles'

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
    const unifiedClass = diffEditorRootVariants({ size: 'md', mode: 'unified' })
    const chunks = Chunk.build(baseText, headText)

    // Assert
    expect(splitClass).toContain('helical-diff-editor')
    expect(splitClass).toContain('helical-diff-editor-split')
    expect(unifiedClass).toContain('helical-diff-editor-unified')
    expect(chunks.length).toBe(2)
  })

  it('should clamp diagnostics, detect Markdown filenames, and build line-wrapping extensions via shared editor helpers', () => {
    // Arrange
    const diagnostics = [
      {
        from: -5,
        to: 999,
        severity: 'error' as const,
        message: 'Unexpected token',
        source: 'rust-analyzer',
        code: 'E0001',
      },
    ]

    // Act
    const mapped = toCodeMirrorDiagnostics(24, diagnostics)
    const wrapOff = toLineWrappingExtension('off')
    const wrapAuto = toLineWrappingExtension('auto')
    const wrapColumn = toLineWrappingExtension(80)

    // Assert
    expect(mapped[0]?.from).toBe(0)
    expect(mapped[0]?.to).toBe(24)
    expect(mapped[0]?.source).toBe('rust-analyzer E0001')
    expect(isMarkdownTarget('docs/architecture/diffs.md')).toBe(true)
    expect(isMarkdownTarget('src/services/diff.rs')).toBe(false)
    expect(Array.isArray(wrapOff) && wrapOff.length).toBe(0)
    expect(Array.isArray(wrapAuto) && wrapAuto.length).toBe(1)
    expect(Array.isArray(wrapColumn) && wrapColumn.length).toBe(2)
  })
})

describe('Given a DiffEditor in unified mode / When editing head content with readonly false', () => {
  it('should update changed chunks dynamically and guard modal hunk navigation when no view is mounted', () => {
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
