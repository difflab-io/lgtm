import { HighlightStyle, syntaxHighlighting } from '@codemirror/language'
import { type Extension } from '@codemirror/state'
import { EditorView } from '@codemirror/view'
import { tags } from '@lezer/highlight'
import { getCachedLanguageExtension, loadLanguageExtension } from './languages'
import type { CodeEditorLineWrapping } from './types'

/**
 * Lezer syntax highlight style mapped to LGTM semantic CSS tokens (`--cm-syntax-*`).
 */
export const codeEditorHighlightStyle = HighlightStyle.define([
  {
    tag: [
      tags.keyword,
      tags.controlKeyword,
      tags.moduleKeyword,
      tags.definitionKeyword,
      tags.modifier,
    ],
    color: 'var(--cm-syntax-keyword, #8b5cf6)',
  },
  {
    tag: [tags.operator, tags.operatorKeyword, tags.punctuation],
    color: 'var(--cm-syntax-operator, inherit)',
  },
  {
    tag: [tags.string, tags.special(tags.string), tags.regexp, tags.character],
    color: 'var(--cm-syntax-string, #2ebd6b)',
  },
  {
    tag: [
      tags.number,
      tags.integer,
      tags.float,
      tags.bool,
      tags.null,
      tags.atom,
      tags.constant(tags.variableName),
    ],
    color: 'var(--cm-syntax-number, #f59e0b)',
  },
  {
    tag: [
      tags.function(tags.variableName),
      tags.function(tags.propertyName),
      tags.definition(tags.function(tags.variableName)),
      tags.definition(tags.typeName),
    ],
    color: 'var(--cm-syntax-function, #52a8ff)',
  },
  {
    tag: [tags.typeName, tags.className, tags.namespace],
    color: 'var(--cm-syntax-type, inherit)',
  },
  {
    tag: [tags.propertyName, tags.definition(tags.propertyName)],
    color: 'var(--cm-syntax-property, inherit)',
  },
  {
    tag: [tags.definition(tags.variableName), tags.variableName],
    color: 'var(--cm-syntax-variable, inherit)',
  },
  {
    tag: [tags.self, tags.tagName, tags.labelName, tags.standard(tags.variableName)],
    color: 'var(--cm-syntax-builtin, #f12b82)',
  },
  {
    tag: [tags.macroName, tags.attributeName, tags.annotation, tags.escape],
    color: 'var(--cm-syntax-macro, #00aa95)',
  },
  {
    tag: [tags.comment, tags.lineComment, tags.blockComment, tags.docComment],
    color: 'var(--cm-syntax-comment, #a1a1a1)',
    fontStyle: 'italic',
  },
  {
    tag: [
      tags.heading1,
      tags.heading2,
      tags.heading3,
      tags.heading4,
      tags.heading5,
      tags.heading6,
      tags.heading,
      tags.list,
    ],
    color: 'var(--cm-syntax-heading, #52a8ff)',
    fontWeight: '600',
  },
  {
    tag: tags.strong,
    color: 'var(--cm-syntax-strong, #ffae00)',
    fontWeight: '600',
  },
  {
    tag: tags.emphasis,
    color: 'var(--cm-syntax-italic, #f12b82)',
    fontStyle: 'italic',
  },
  {
    tag: tags.monospace,
    color: 'var(--cm-syntax-code, #2ebd6b)',
    backgroundColor: 'var(--cm-syntax-code-bg, #1a1a1a)',
    borderRadius: '3px',
  },
  { tag: tags.quote, color: 'var(--cm-syntax-quote, #a1a1a1)', fontStyle: 'italic' },
  {
    tag: [tags.link, tags.url],
    color: 'var(--cm-syntax-link, #52a8ff)',
    textDecoration: 'underline',
  },
  {
    tag: tags.strikethrough,
    textDecoration: 'line-through',
    color: 'var(--cm-syntax-meta, #a1a1a1)',
  },
  {
    tag: [tags.processingInstruction, tags.contentSeparator, tags.meta],
    color: 'var(--cm-syntax-meta, #a1a1a1)',
  },
  { tag: tags.inserted, color: 'var(--cm-syntax-inserted, #2ebd6b)' },
  { tag: tags.deleted, color: 'var(--cm-syntax-deleted, #f32e40)' },
  { tag: tags.changed, color: 'var(--cm-syntax-changed, #ffae00)' },
])

/**
 * CodeMirror 6 surface, gutter, selection, modal block-cursor, and tooltip theme extension.
 */
export const codeEditorTheme = EditorView.theme({
  '&': {
    backgroundColor: 'var(--color-background, #101010)',
    color: 'var(--color-foreground, #ededed)',
  },
  '&.cm-focused': {
    outline: 'none',
  },
  '.cm-content, &.cm-editor .cm-content': {
    caretColor: 'var(--color-ring, #52a8ff)',
  },
  '.cm-cursor, .cm-dropCursor, &.cm-editor .cm-cursor, &.cm-editor .cm-dropCursor': {
    borderLeftColor: 'var(--color-ring, #52a8ff)',
  },
  '&.cm-editor[data-modal-mode="normal"] .cm-cursor, &.cm-editor[data-modal-mode="select"] .cm-cursor':
    {
      borderLeftWidth: '1ch',
      borderLeftColor: 'color-mix(in srgb, var(--color-ring, #52a8ff) 55%, transparent)',
      marginLeft: '0',
    },
  '.cm-gutters, &.cm-editor .cm-gutters': {
    backgroundColor: 'var(--color-background, #101010)',
    color: 'var(--cm-gutter-fg, var(--color-muted-foreground, #676767))',
    borderColor: 'var(--color-border, #2e2e2e)',
  },
  '.cm-activeLine, &.cm-editor .cm-activeLine': {
    backgroundColor:
      'color-mix(in srgb, var(--cm-active-line-bg, var(--color-muted, #1a1a1a)) 45%, transparent)',
  },
  '.cm-activeLineGutter, &.cm-editor .cm-activeLineGutter': {
    backgroundColor: 'var(--color-background, #101010)',
    color: 'var(--cm-gutter-active-fg, var(--color-foreground, #ededed))',
    fontWeight: '600',
  },
  '&.cm-focused .cm-matchingBracket, .cm-matchingBracket': {
    backgroundColor: 'transparent',
    color: 'var(--cm-match-paren, #ffae00)',
    textDecoration: 'underline',
    fontWeight: '600',
  },
  '.cm-foldPlaceholder, &.cm-editor .cm-foldPlaceholder': {
    backgroundColor: 'var(--color-muted, #1a1a1a)',
    borderColor: 'var(--color-border, #2e2e2e)',
    color: 'var(--color-muted-foreground, #a1a1a1)',
  },
  '.cm-panels, &.cm-editor .cm-panels': {
    backgroundColor: 'var(--color-card, #1a1a1a)',
    color: 'var(--color-foreground, #ededed)',
    borderColor: 'var(--color-border, #2e2e2e)',
  },
  '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection':
    {
      backgroundColor: 'var(--cm-selection-bg, #264564) !important',
    },
  '.cm-tooltip': {
    border: '1px solid var(--color-border, #2e2e2e)',
    borderRadius: '0.375rem',
    backgroundColor: 'var(--color-popover, #1a1a1a)',
    color: 'var(--color-popover-foreground, #ededed)',
    boxShadow: '0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1)',
    overflowY: 'auto',
    overscrollBehavior: 'contain',
    maxWidth: 'min(32rem, calc(100vw - 2rem))',
  },
  '.cm-tooltip.cm-tooltip-hover': {
    border: '1px solid var(--color-border, #2e2e2e)',
    borderRadius: '0.375rem',
    backgroundColor: 'var(--color-popover, #1a1a1a)',
  },
  '.cm-tooltip .cm-tooltip-section': {
    border: 'none',
    backgroundColor: 'transparent',
  },
  '.cm-tooltip .cm-tooltip-section:not(:first-child)': {
    borderTop: '1px solid var(--color-border, #2e2e2e)',
  },
})

/**
 * Returns the combined CodeMirror 6 theme and Lezer syntax highlighting extension.
 */
export function codeEditorThemeExtension(): Extension {
  return [syntaxHighlighting(codeEditorHighlightStyle), codeEditorTheme]
}

/**
 * Builds the CodeMirror 6 line-wrapping extension for the given `CodeEditorLineWrapping` mode.
 */
export function toLineWrappingExtension(mode: CodeEditorLineWrapping): Extension {
  if (!mode || mode === 'off') {
    return []
  }
  if (typeof mode === 'number' && Number.isFinite(mode) && mode > 0) {
    const cols = Math.max(1, Math.floor(mode))
    return [
      EditorView.lineWrapping,
      EditorView.theme({
        '.cm-content, &.cm-editor .cm-content': {
          maxWidth: `${cols}ch`,
        },
      }),
    ]
  }
  return [EditorView.lineWrapping]
}

export { getCachedLanguageExtension, loadLanguageExtension }

/**
 * Resolves the CodeMirror 6 language extension for `targetName` (filename or buffer title).
 */
export async function resolveLanguageExtension(targetName: string | undefined): Promise<Extension> {
  return loadLanguageExtension(targetName)
}
