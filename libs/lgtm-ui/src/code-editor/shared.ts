import { HighlightStyle } from '@codemirror/language'
import type { Diagnostic } from '@codemirror/lint'
import type { Extension } from '@codemirror/state'
import { EditorView } from '@codemirror/view'
import { tags } from '@lezer/highlight'
import { renderMarkdownToDom } from './markdown'
import type { CodeEditorDiagnostic, CodeEditorLineWrapping } from './types'

/** Shared Lezer syntax highlight style used across `CodeEditor` and `DiffEditor`. */
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

/** Returns whether `targetName` has a Markdown file extension. */
export function isMarkdownTarget(targetName: string): boolean {
  const lower = targetName.toLowerCase()
  return (
    lower.endsWith('.md') ||
    lower.endsWith('.markdown') ||
    lower.endsWith('.mdx') ||
    lower.endsWith('.mkd')
  )
}

/** Builds a CodeMirror line-wrapping extension for `mode`. */
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

/** Converts `CodeEditorDiagnostic[]` items into clamped CodeMirror `Diagnostic[]` entries. */
export function toCodeMirrorDiagnostics(
  docLength: number,
  items: CodeEditorDiagnostic[],
): Diagnostic[] {
  return items.map((item) => {
    const from = Math.max(0, Math.min(docLength, item.from))
    const to = Math.max(from, Math.min(docLength, item.to))
    const sourceLabel = [item.source, item.code].filter(Boolean).join(' ')
    return {
      from,
      to,
      severity: item.severity,
      message: item.message,
      renderMessage() {
        const wrapper = document.createElement('div')
        wrapper.setAttribute('data-slot', 'code-editor-diagnostic-message')
        wrapper.appendChild(renderMarkdownToDom(item.message))
        return wrapper
      },
      ...(sourceLabel ? { source: sourceLabel } : {}),
    }
  })
}
