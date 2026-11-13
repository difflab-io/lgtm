import { closeCompletion, completionStatus } from '@codemirror/autocomplete'
import { indentWithTab, redo, undo } from '@codemirror/commands'
import { indentUnit, matchBrackets, syntaxTree } from '@codemirror/language'
import {
  CharCategory,
  EditorSelection,
  type EditorState,
  Facet,
  StateEffect,
  StateField,
  type Extension,
  type SelectionRange,
  type Text,
} from '@codemirror/state'
import { EditorView, ViewPlugin, keymap, type ViewUpdate } from '@codemirror/view'
import {
  getDocumentWhichKeyController,
  registerModalEditor,
  type KeybindingPreset,
  type KeymapMode,
  type ModalEditorTarget,
  type ModalMotion,
  type ModalOperator,
  type ModalSelectionTransform,
  type ModalTextObjectSpec,
} from '@cloudvoyant/helical-ui'
import { clearEditorCursorTooltip, hasEditorIntellisenseActive } from './lspExtension'

export {
  clearEditorCursorTooltip,
  codeEditorLspExtension,
  dismissEditorIntellisense,
  editorCursorTooltipField,
  gotoFirstEditorDiagnostic,
  gotoLastEditorDiagnostic,
  gotoNextEditorDiagnostic,
  gotoPrevEditorDiagnostic,
  hasEditorIntellisenseActive,
  lspOptionsFacet,
  showEditorHoverTooltip,
  showEditorSignatureHelpTooltip,
  toCodeMirrorDiagnostics,
  triggerEditorCompletion,
  type CodeEditorLspOptions,
} from './lspExtension'

export {
  codeEditorHighlightStyle,
  codeEditorTheme,
  codeEditorThemeExtension,
  resolveLanguageExtension,
  toLineWrappingExtension,
} from './themeExtension'

/**
 * Configuration options for [`modalEditorExtension`].
 */
export interface ModalEditorExtensionOptions {
  /** Whether clicking inside the editor switches `normal` mode to `insert` mode. */
  clickToInsert?: boolean
  /** Whether the editor is currently read-only. */
  readOnly?: boolean
}

/**
 * Snapshot of the active modal mode and preset stored in [`modalStateField`].
 */
export interface ModalEditorStateValue {
  /** Active modal mode (`normal`, `insert`, `select`, or `jump`). */
  mode: KeymapMode
  /** Active keybinding preset (`helix`, `vim`, or `kakoune`). */
  preset: KeybindingPreset
}

/**
 * State effect dispatched when the active modal mode or preset changes.
 */
export const setModalStateEffect = StateEffect.define<ModalEditorStateValue>()

/**
 * CodeMirror 6 state field tracking the active modal editing mode and preset.
 */
export const modalStateField = StateField.define<ModalEditorStateValue>({
  create() {
    return { mode: 'normal', preset: 'helix' }
  },
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setModalStateEffect)) {
        return effect.value
      }
    }
    return value
  },
})

/**
 * CodeMirror 6 facet configuring pointer and read-only behavior for [`modalEditorExtension`].
 */
export const modalOptionsFacet = Facet.define<
  ModalEditorExtensionOptions,
  Required<ModalEditorExtensionOptions>
>({
  combine(values) {
    const merged = Object.assign({ clickToInsert: true, readOnly: false }, ...values)
    return {
      clickToInsert: Boolean(merged.clickToInsert),
      readOnly: Boolean(merged.readOnly),
    }
  },
})

/**
 * State effect dispatched when modal yank, delete, change, or DOM copy updates the modal clipboard register.
 */
export const setModalClipboardEffect = StateEffect.define<string>()

/**
 * CodeMirror 6 state field holding the active modal yank/paste clipboard register.
 */
export const modalClipboardField = StateField.define<string>({
  create() {
    return ''
  },
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setModalClipboardEffect)) {
        return effect.value
      }
    }
    return value
  },
})

const modalClipboardDomHandlers = EditorView.domEventHandlers({
  copy(_event, view) {
    const { doc, selection } = view.state
    const chunks = selection.ranges
      .filter((r) => !r.empty)
      .map((r) => doc.sliceString(r.from, r.to))
    if (chunks.length > 0) {
      view.dispatch({ effects: setModalClipboardEffect.of(chunks.join('\n')) })
    }
    return false
  },
  cut(_event, view) {
    const { doc, selection } = view.state
    const chunks = selection.ranges
      .filter((r) => !r.empty)
      .map((r) => doc.sliceString(r.from, r.to))
    if (chunks.length > 0) {
      view.dispatch({ effects: setModalClipboardEffect.of(chunks.join('\n')) })
    }
    return false
  },
})

/**
 * Returns the CodeMirror 6 extension managing the modal yank/paste clipboard register and DOM `copy`/`cut` synchronization.
 */
export function modalClipboardExtension(): Extension {
  return [modalClipboardField, modalClipboardDomHandlers]
}

function writeClipboardRegister(view: EditorView, text: string): void {
  view.dispatch({ effects: setModalClipboardEffect.of(text) })
  if (text && typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
    void navigator.clipboard.writeText(text).catch(() => {})
  }
}

function charKind(
  ch: string,
  categorize?: (char: string) => CharCategory,
): 'word' | 'space' | 'punct' {
  if (!ch || ch === '\n' || ch === '\r') return 'space'
  if (categorize) {
    const cat = categorize(ch)
    if (cat === CharCategory.Space) return 'space'
    if (cat === CharCategory.Word) return 'word'
    return 'punct'
  }
  if (ch === ' ' || ch === '\t') return 'space'
  if (/[A-Za-z0-9_]/.test(ch)) return 'word'
  return 'punct'
}

function charKindAt(
  doc: Text,
  pos: number,
  categorize?: (char: string) => CharCategory,
): 'word' | 'space' | 'punct' {
  if (pos < 0 || pos >= doc.length) return 'space'
  const line = doc.lineAt(pos)
  if (pos === line.to) return 'space'
  return charKind(line.text[pos - line.from] ?? '', categorize)
}

function stepWordForward(
  doc: Text,
  pos: number,
  categorize?: (char: string) => CharCategory,
): { from: number; to: number } {
  const len = doc.length
  let i = Math.max(0, Math.min(len, pos))
  if (i >= len) return { from: len, to: len }

  const startKind = charKindAt(doc, i, categorize)
  if (startKind !== 'space') {
    const line = doc.lineAt(i)
    while (i < line.to && charKind(line.text[i - line.from] ?? '', categorize) === startKind) {
      i += 1
    }
  }
  while (i < len && charKindAt(doc, i, categorize) === 'space') {
    const line = doc.lineAt(i)
    while (i < line.to && charKind(line.text[i - line.from] ?? '', categorize) === 'space') {
      i += 1
    }
    if (i === line.to && i < len) {
      i += 1
    }
  }
  const wordStart = i
  if (i >= len) {
    return { from: pos, to: len }
  }
  const targetLine = doc.lineAt(i)
  const targetKind = charKind(targetLine.text[i - targetLine.from] ?? '', categorize)
  while (
    i < targetLine.to &&
    charKind(targetLine.text[i - targetLine.from] ?? '', categorize) === targetKind
  ) {
    i += 1
  }
  return { from: wordStart, to: i }
}

function stepWordEnd(
  doc: Text,
  pos: number,
  categorize?: (char: string) => CharCategory,
): { from: number; to: number } {
  const len = doc.length
  let i = Math.max(0, Math.min(len, pos))
  if (i >= len) return { from: len, to: len }

  const curKind = charKindAt(doc, i, categorize)
  if (curKind !== 'space' && charKindAt(doc, i + 1, categorize) !== curKind) {
    i += 1
  }
  while (i < len && charKindAt(doc, i, categorize) === 'space') {
    const line = doc.lineAt(i)
    while (i < line.to && charKind(line.text[i - line.from] ?? '', categorize) === 'space') {
      i += 1
    }
    if (i === line.to && i < len) {
      i += 1
    }
  }
  const wordStart = i
  if (i >= len) return { from: pos, to: len }
  const targetLine = doc.lineAt(i)
  const kind = charKind(targetLine.text[i - targetLine.from] ?? '', categorize)
  while (
    i < targetLine.to &&
    charKind(targetLine.text[i - targetLine.from] ?? '', categorize) === kind
  ) {
    i += 1
  }
  return { from: wordStart, to: i }
}

function stepWordBackward(
  doc: Text,
  pos: number,
  categorize?: (char: string) => CharCategory,
): { from: number; to: number } {
  const len = doc.length
  let i = Math.max(0, Math.min(len, pos))
  if (i <= 0) return { from: 0, to: 0 }

  i -= 1
  while (i > 0 && charKindAt(doc, i, categorize) === 'space') {
    const line = doc.lineAt(i)
    if (i === line.to && i > line.from) {
      i -= 1
    }
    while (i > line.from && charKind(line.text[i - line.from] ?? '', categorize) === 'space') {
      i -= 1
    }
    if (i === line.from && charKind(line.text[0] ?? '', categorize) === 'space' && i > 0) {
      i -= 1
    }
  }
  const wordEnd = i + 1
  const targetLine = doc.lineAt(i)
  const kind = charKind(targetLine.text[i - targetLine.from] ?? '', categorize)
  while (
    i > targetLine.from &&
    charKind(targetLine.text[i - 1 - targetLine.from] ?? '', categorize) === kind
  ) {
    i -= 1
  }
  return { from: i, to: wordEnd }
}

function stepParagraph(doc: Text, pos: number, dir: 1 | -1): number {
  const line = doc.lineAt(pos)
  let lineNum = line.number
  const isBlank = (n: number) => doc.line(n).text.trim().length === 0

  if (dir > 0) {
    while (lineNum <= doc.lines && !isBlank(lineNum)) {
      lineNum += 1
    }
    while (lineNum <= doc.lines && isBlank(lineNum)) {
      lineNum += 1
    }
    if (lineNum > doc.lines) return doc.length
    return doc.line(lineNum).from
  }

  if (pos > line.from && !isBlank(lineNum)) {
    while (lineNum > 1 && !isBlank(lineNum - 1)) {
      lineNum -= 1
    }
    return doc.line(lineNum).from
  }
  lineNum -= 1
  while (lineNum >= 1 && isBlank(lineNum)) {
    lineNum -= 1
  }
  while (lineNum > 1 && !isBlank(lineNum - 1)) {
    lineNum -= 1
  }
  if (lineNum < 1) return 0
  return doc.line(lineNum).from
}

function findWordObject(
  doc: Text,
  pos: number,
  around: boolean,
  categorize?: (char: string) => CharCategory,
): { from: number; to: number } | null {
  const line = doc.lineAt(pos)
  const text = line.text
  if (text.length === 0) return null
  const offset = Math.min(Math.max(0, pos - line.from), Math.max(0, text.length - 1))
  const kind = charKind(text[offset] ?? '', categorize)
  let start = offset
  let end = offset + 1
  while (start > 0 && charKind(text[start - 1] ?? '', categorize) === kind) {
    start -= 1
  }
  while (end < text.length && charKind(text[end] ?? '', categorize) === kind) {
    end += 1
  }
  if (around) {
    const prevEnd = end
    while (end < text.length && (text[end] === ' ' || text[end] === '\t')) {
      end += 1
    }
    if (end === prevEnd) {
      while (start > 0 && (text[start - 1] === ' ' || text[start - 1] === '\t')) {
        start -= 1
      }
    }
  }
  return { from: line.from + start, to: line.from + end }
}

function findParagraphObject(
  doc: Text,
  pos: number,
  around: boolean,
): { from: number; to: number } | null {
  const current = doc.lineAt(pos)
  const isBlank = (n: number) => doc.line(n).text.trim().length === 0
  if (isBlank(current.number)) return null
  let startLine = current.number
  let endLine = current.number
  while (startLine > 1 && !isBlank(startLine - 1)) {
    startLine -= 1
  }
  while (endLine < doc.lines && !isBlank(endLine + 1)) {
    endLine += 1
  }
  if (around) {
    while (endLine < doc.lines && isBlank(endLine + 1)) {
      endLine += 1
    }
  }
  const from = doc.line(startLine).from
  const toLine = doc.line(endLine)
  const to = around && endLine < doc.lines ? toLine.to + 1 : toLine.to
  return { from, to }
}

const STRUCTURAL_NODE_PATTERN =
  /FencedCode|CodeBlock|Block|StatementBlock|ObjectExpression|ArrayExpression|Function|Method|Class|Interface|Enum|Struct|Impl|Trait|Module/

function findCodeblockOrBracketObject(
  state: EditorState,
  pos: number,
  around: boolean,
): { from: number; to: number } | null {
  const { doc } = state
  const tree = syntaxTree(state)
  let node = tree.resolveInner(pos, 0)
  while (node.parent) {
    if (STRUCTURAL_NODE_PATTERN.test(node.name) && node.to > node.from) {
      if (node.name === 'FencedCode' || node.name === 'CodeBlock') {
        const startLine = doc.lineAt(node.from).number
        const endLine = doc.lineAt(node.to).number
        if (around || endLine <= startLine + 1) {
          return { from: node.from, to: node.to }
        }
        return {
          from: doc.line(startLine + 1).from,
          to: doc.line(endLine - 1).to,
        }
      }
      if (around) {
        return { from: node.from, to: node.to }
      }
      const firstChar = doc.sliceString(node.from, node.from + 1)
      const lastChar = doc.sliceString(Math.max(node.from, node.to - 1), node.to)
      if (
        (firstChar === '{' && lastChar === '}') ||
        (firstChar === '(' && lastChar === ')') ||
        (firstChar === '[' && lastChar === ']')
      ) {
        return { from: node.from + 1, to: Math.max(node.from + 1, node.to - 1) }
      }
      return { from: node.from, to: node.to }
    }
    node = node.parent
  }

  const currentLine = doc.lineAt(pos).number
  let openFence: number | null = null
  let closeFence: number | null = null
  for (let n = currentLine; n >= 1; n -= 1) {
    if (doc.line(n).text.trimStart().startsWith('```')) {
      openFence = n
      break
    }
  }
  if (openFence !== null) {
    for (let n = Math.max(openFence + 1, currentLine); n <= doc.lines; n += 1) {
      if (doc.line(n).text.trimStart().startsWith('```')) {
        closeFence = n
        break
      }
    }
  }
  if (openFence !== null && closeFence !== null && openFence < closeFence) {
    if (around) {
      return { from: doc.line(openFence).from, to: doc.line(closeFence).to }
    }
    const innerStart = Math.min(closeFence, openFence + 1)
    const innerEnd = Math.max(innerStart, closeFence - 1)
    return { from: doc.line(innerStart).from, to: doc.line(innerEnd).to }
  }

  const directMatch = matchBrackets(state, pos, 1) ?? matchBrackets(state, pos, -1)
  if (directMatch?.matched && directMatch.end) {
    const from = Math.min(directMatch.start.from, directMatch.end.from)
    const to = Math.max(directMatch.start.to, directMatch.end.to)
    return around ? { from, to } : { from: from + 1, to: Math.max(from + 1, to - 1) }
  }

  return findParagraphObject(doc, pos, around)
}

const SECTION_NODE_PATTERN =
  /ATXHeading|SetextHeading|FencedCode|Function|Method|Class|Interface|TypeAlias|Enum|Struct|Impl|Trait|Module|ModItem|ExportDeclaration|LexicalDeclaration|VariableDeclaration|RuleSet/

const SECTION_LINE_PATTERN =
  /^(?:#{1,6}\s+|(?:export\s+)?(?:default\s+)?(?:async\s+)?(?:pub(?:\([^)]*\))?\s+)?(?:fn|function|class|interface|type|enum|struct|impl|trait|mod|def|func)\b|\S.*\{\s*$)/

function collectSectionOffsets(state: EditorState): number[] {
  const { doc } = state
  const offsets = new Set<number>()
  const tree = syntaxTree(state)
  tree.iterate({
    enter(node) {
      if (SECTION_NODE_PATTERN.test(node.name) && node.from < node.to) {
        offsets.add(doc.lineAt(node.from).from)
      }
    },
  })
  for (let n = 1; n <= doc.lines; n += 1) {
    const line = doc.line(n)
    if (SECTION_LINE_PATTERN.test(line.text)) {
      offsets.add(line.from)
    }
  }
  return [...offsets].sort((a, b) => a - b)
}

/**
 * Jumps to the next (`dir = 1`) or previous (`dir = -1`) structural code section, function, or heading in `view`.
 */
export function gotoEditorSection(
  view: EditorView | null | undefined,
  dir: 1 | -1,
  count = 1,
): boolean {
  if (!view) return false
  const { state } = view
  const { doc, selection } = state
  const steps = Math.max(1, count || 1)
  const extend = view.dom.dataset.modalMode === 'select'
  const sections = collectSectionOffsets(state)

  let cur = selection.main.head
  for (let s = 0; s < steps; s += 1) {
    if (sections.length > 0) {
      const currentLineStart = doc.lineAt(cur).from
      const candidate =
        dir > 0
          ? sections.find((offset) => offset > currentLineStart)
          : [...sections].reverse().find((offset) => offset < currentLineStart)
      if (candidate !== undefined) {
        cur = candidate
        continue
      }
    }
    cur = stepParagraph(doc, cur, dir)
  }

  const nextRange = extend
    ? EditorSelection.range(selection.main.anchor, cur)
    : EditorSelection.cursor(cur)
  view.dispatch({
    selection: EditorSelection.create([nextRange], 0),
    scrollIntoView: true,
  })
  return true
}

/**
 * Resolves the document offset of the symbol under the primary cursor or selection in `state`.
 */
export function resolveEditorSymbolOffset(state: EditorState): number {
  const { doc, selection } = state
  const main = selection.main
  if (main.empty) return main.head
  const categorize = state.charCategorizer(main.head)
  if (main.head > main.anchor && charKindAt(doc, main.head, categorize) !== 'word') {
    for (let pos = main.from; pos < main.to; pos += 1) {
      if (charKindAt(doc, pos, categorize) === 'word') {
        return pos
      }
    }
    return main.from
  }
  return main.head
}

/**
 * Scrolls the active CodeMirror cursor line to the vertical center, top, or bottom of the viewport.
 */
export function scrollEditorAlign(
  view: EditorView | null | undefined,
  where: 'center' | 'top' | 'bottom',
): boolean {
  if (!view) return false
  const head = view.state.selection.main.head
  view.dispatch({
    effects: EditorView.scrollIntoView(head, {
      y: where === 'center' ? 'center' : where === 'top' ? 'start' : 'end',
    }),
  })
  return true
}

/**
 * Scrolls the CodeMirror viewport up or down by half a page (`count` times) and moves the cursor into view.
 */
export function scrollEditorHalfPage(
  view: EditorView | null | undefined,
  dir: 1 | -1,
  count = 1,
): boolean {
  if (!view) return false
  const scroller = view.scrollDOM
  const delta = Math.max(120, Math.round(scroller.clientHeight * 0.5)) * Math.max(1, count)
  scroller.scrollTop = Math.max(0, scroller.scrollTop + dir * delta)
  return true
}

/**
 * Creates a Helical UI `ModalEditorTarget` backed by a CodeMirror 6 `EditorView`.
 */
export function createCodeMirrorModalTarget(
  getView: () => EditorView | undefined,
): ModalEditorTarget {
  return {
    move(
      motion: ModalMotion,
      options: { preset: KeybindingPreset; mode: KeymapMode; count: number },
    ): boolean {
      const view = getView()
      if (!view) return false
      const { state } = view
      const { doc, selection } = state
      const categorize = state.charCategorizer(selection.main.head)
      const steps = Math.max(1, options.count || 1)
      const isSelectionFirst = options.preset === 'helix' || options.preset === 'kakoune'
      const extend = options.mode === 'select'

      const nextRanges: SelectionRange[] = selection.ranges.map((range) => {
        const head = range.head
        const anchor = range.anchor

        switch (motion) {
          case 'left': {
            const next = Math.max(0, head - steps)
            return extend ? EditorSelection.range(anchor, next) : EditorSelection.cursor(next)
          }
          case 'right': {
            const next = Math.min(doc.length, head + steps)
            return extend ? EditorSelection.range(anchor, next) : EditorSelection.cursor(next)
          }
          case 'down':
          case 'up': {
            const dir = motion === 'down' ? 1 : -1
            const line = doc.lineAt(head)
            const col = head - line.from
            const targetLineNum = Math.max(1, Math.min(doc.lines, line.number + dir * steps))
            const targetLine = doc.line(targetLineNum)
            const next = Math.min(targetLine.to, targetLine.from + col)
            return extend ? EditorSelection.range(anchor, next) : EditorSelection.cursor(next)
          }
          case 'wordForward': {
            let cur = !extend && !range.empty && head > anchor ? head - 1 : head
            let span = { from: head, to: head }
            for (let s = 0; s < steps; s += 1) {
              span = stepWordForward(doc, cur, categorize)
              cur = Math.max(span.from, span.to - 1)
            }
            if (extend) return EditorSelection.range(anchor, span.to)
            if (isSelectionFirst) return EditorSelection.range(span.from, span.to)
            return EditorSelection.cursor(span.from)
          }
          case 'wordEnd': {
            let cur = !extend && !range.empty && head > anchor ? head - 1 : head
            let span = { from: head, to: head }
            for (let s = 0; s < steps; s += 1) {
              span = stepWordEnd(doc, cur, categorize)
              cur = Math.max(span.from, span.to - 1)
            }
            if (extend) return EditorSelection.range(anchor, span.to)
            if (isSelectionFirst) return EditorSelection.range(span.from, span.to)
            return EditorSelection.cursor(Math.max(span.from, span.to - 1))
          }
          case 'wordBackward': {
            let cur = head
            let span = { from: head, to: head }
            for (let s = 0; s < steps; s += 1) {
              span = stepWordBackward(doc, cur, categorize)
              cur = span.from
            }
            if (extend) return EditorSelection.range(anchor, span.from)
            if (isSelectionFirst) return EditorSelection.range(span.to, span.from)
            return EditorSelection.cursor(span.from)
          }
          case 'paragraphForward':
          case 'paragraphBackward': {
            const dir = motion === 'paragraphForward' ? 1 : -1
            let cur = head
            for (let s = 0; s < steps; s += 1) {
              cur = stepParagraph(doc, cur, dir)
            }
            return extend ? EditorSelection.range(anchor, cur) : EditorSelection.cursor(cur)
          }
          case 'lineStart': {
            const line = doc.lineAt(head)
            return extend
              ? EditorSelection.range(anchor, line.from)
              : EditorSelection.cursor(line.from)
          }
          case 'lineEnd': {
            const line = doc.lineAt(head)
            return extend ? EditorSelection.range(anchor, line.to) : EditorSelection.cursor(line.to)
          }
          case 'lineSelect': {
            const startLine = doc.lineAt(range.from)
            const curEndLine = doc.lineAt(range.to)
            const alreadyFullLine =
              !range.empty && range.from === startLine.from && range.to === curEndLine.to
            const targetEndLineNum = alreadyFullLine
              ? Math.min(doc.lines, curEndLine.number + steps)
              : Math.min(doc.lines, startLine.number + steps - 1)
            const endLine = doc.line(targetEndLineNum)
            return EditorSelection.range(startLine.from, endLine.to)
          }
          case 'docStart': {
            return extend ? EditorSelection.range(anchor, 0) : EditorSelection.cursor(0)
          }
          case 'docEnd': {
            return extend
              ? EditorSelection.range(anchor, doc.length)
              : EditorSelection.cursor(doc.length)
          }
        }
      })

      view.dispatch({
        selection: EditorSelection.create(nextRanges, selection.mainIndex),
        scrollIntoView: true,
      })
      return true
    },

    selectTextObject(spec: ModalTextObjectSpec): boolean {
      const view = getView()
      if (!view) return false
      const { doc, selection } = view.state
      const categorize = view.state.charCategorizer(selection.main.head)
      const around = spec.startsWith('a')
      const kind = spec[1]
      let changed = false

      const nextRanges = selection.ranges.map((range) => {
        let match: { from: number; to: number } | null = null
        if (kind === 'w') {
          match = findWordObject(doc, range.head, around, categorize)
        } else if (kind === 'p') {
          match = findParagraphObject(doc, range.head, around)
        } else if (kind === 'c' || kind === 'n') {
          match = findCodeblockOrBracketObject(view.state, range.head, around)
        } else if (kind === 'l') {
          const line = doc.lineAt(range.head)
          if (around) {
            match = {
              from: line.from,
              to: Math.min(doc.length, line.to + (line.number < doc.lines ? 1 : 0)),
            }
          } else {
            const leading = line.text.length - line.text.trimStart().length
            const trailing = line.text.length - line.text.trimEnd().length
            match = {
              from: line.from + leading,
              to: Math.max(line.from + leading, line.to - trailing),
            }
          }
        }
        if (!match) return range
        changed = true
        return EditorSelection.range(match.from, match.to)
      })

      if (!changed) return false
      view.dispatch({
        selection: EditorSelection.create(nextRanges, selection.mainIndex),
        scrollIntoView: true,
      })
      return true
    },

    transformSelection(action: ModalSelectionTransform): boolean {
      const view = getView()
      if (!view) return false
      const { doc, selection } = view.state

      switch (action) {
        case 'collapse': {
          const ranges = selection.ranges.map((r) => EditorSelection.cursor(r.head))
          view.dispatch({ selection: EditorSelection.create(ranges, selection.mainIndex) })
          return true
        }
        case 'flip': {
          const ranges = selection.ranges.map((r) => EditorSelection.range(r.head, r.anchor))
          view.dispatch({ selection: EditorSelection.create(ranges, selection.mainIndex) })
          return true
        }
        case 'keepPrimary': {
          const main = selection.main
          view.dispatch({ selection: EditorSelection.single(main.anchor, main.head) })
          return true
        }
        case 'addCursorBelow':
        case 'addCursorAbove': {
          const dir = action === 'addCursorBelow' ? 1 : -1
          const refRange =
            dir > 0 ? selection.ranges[selection.ranges.length - 1]! : selection.ranges[0]!
          const line = doc.lineAt(refRange.head)
          const targetLineNum = line.number + dir
          if (targetLineNum < 1 || targetLineNum > doc.lines) return false
          const col = refRange.head - line.from
          const targetLine = doc.line(targetLineNum)
          const nextPos = Math.min(targetLine.to, targetLine.from + col)
          const nextRanges = [...selection.ranges, EditorSelection.cursor(nextPos)]
          view.dispatch({
            selection: EditorSelection.create(nextRanges, nextRanges.length - 1),
            scrollIntoView: true,
          })
          return true
        }
      }
    },

    execOperator(
      op: ModalOperator,
      options: { preset: KeybindingPreset; mode: KeymapMode; count: number },
    ): { enterInsert?: boolean; yankedText?: string } | boolean {
      const view = getView()
      if (!view) return false
      const { state } = view
      const { doc, selection } = state
      const steps = Math.max(1, options.count || 1)

      if (op === 'yank') {
        const chunks = selection.ranges.map((r) => {
          if (!r.empty) return doc.sliceString(r.from, r.to)
          const line = doc.lineAt(r.head)
          return `${line.text}\n`
        })
        const yankedText = chunks.join('\n')
        writeClipboardRegister(view, yankedText)
        return { yankedText }
      }

      if (state.readOnly) return false

      if (op === 'undo') {
        return undo(view)
      }
      if (op === 'redo') {
        return redo(view)
      }

      if (op === 'delete' || op === 'change') {
        const changes: Array<{ from: number; to: number; insert: string }> = []
        const yankedChunks: string[] = []
        for (const r of selection.ranges) {
          const from = r.from
          let to = r.to
          if (from === to) {
            to = Math.min(doc.length, from + steps)
          }
          if (from < to) {
            yankedChunks.push(doc.sliceString(from, to))
            changes.push({ from, to, insert: '' })
          }
        }
        const yankedText = yankedChunks.join('\n')
        if (changes.length > 0) {
          view.dispatch({
            changes,
            ...(yankedText ? { effects: setModalClipboardEffect.of(yankedText) } : {}),
            scrollIntoView: true,
          })
          if (yankedText && typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
            void navigator.clipboard.writeText(yankedText).catch(() => {})
          }
        }
        return op === 'change' ? { enterInsert: true } : true
      }

      if (op === 'pasteAfter' || op === 'pasteBefore') {
        const yankedText = state.field(modalClipboardField, false) ?? ''
        if (!yankedText) return false
        const repeated = yankedText.repeat(steps)
        const isLinewise = yankedText.endsWith('\n')
        const changes = selection.ranges.map((r) => {
          if (isLinewise) {
            const line = doc.lineAt(r.head)
            if (op === 'pasteAfter') {
              const trimmed = repeated.replace(/\n$/, '')
              return { from: line.to, to: line.to, insert: `\n${trimmed}` }
            }
            return { from: line.from, to: line.from, insert: repeated }
          }
          const insertPos =
            op === 'pasteAfter'
              ? r.empty
                ? Math.min(doc.lineAt(r.head).to, r.head + 1)
                : r.to
              : r.from
          return { from: insertPos, to: insertPos, insert: repeated }
        })
        view.dispatch({ changes, scrollIntoView: true })
        return true
      }

      if (op === 'insertStart') {
        const ranges = selection.ranges.map((r) => EditorSelection.cursor(r.from))
        view.dispatch({ selection: EditorSelection.create(ranges, selection.mainIndex) })
        return { enterInsert: true }
      }

      if (op === 'appendEnd') {
        const ranges = selection.ranges.map((r) => {
          if (!r.empty) return EditorSelection.cursor(r.to)
          const line = doc.lineAt(r.head)
          const next = options.preset === 'vim' ? Math.min(line.to, r.head + 1) : r.head
          return EditorSelection.cursor(next)
        })
        view.dispatch({ selection: EditorSelection.create(ranges, selection.mainIndex) })
        return { enterInsert: true }
      }

      if (op === 'openBelow' || op === 'openAbove') {
        const main = selection.main
        const line = doc.lineAt(main.head)
        const indentMatch = /^[\t ]*/.exec(line.text)
        const indent = indentMatch ? indentMatch[0] : ''
        if (op === 'openBelow') {
          const insert = `\n${indent}`
          const nextCursor = line.to + insert.length
          view.dispatch({
            changes: { from: line.to, to: line.to, insert },
            selection: EditorSelection.cursor(nextCursor),
            scrollIntoView: true,
          })
        } else {
          const insert = `${indent}\n`
          const nextCursor = line.from + indent.length
          view.dispatch({
            changes: { from: line.from, to: line.from, insert },
            selection: EditorSelection.cursor(nextCursor),
            scrollIntoView: true,
          })
        }
        return { enterInsert: true }
      }

      return false
    },

    setModalState(modalState: ModalEditorStateValue): void {
      const view = getView()
      if (!view) return
      const prevMode = view.dom.dataset.modalMode
      view.dom.dataset.modalMode = modalState.mode
      view.dom.dataset.modalPreset = modalState.preset
      if (modalState.mode !== 'insert') {
        if (completionStatus(view.state) !== null) {
          closeCompletion(view)
        }
        if (prevMode === 'insert') {
          clearEditorCursorTooltip(view)
        }
      }
    },

    hasActiveSuggestion(): boolean {
      return hasEditorIntellisenseActive(getView())
    },
  }
}

const modalEditorViewPlugin = ViewPlugin.fromClass(
  class {
    private readonly unregister: () => void

    constructor(private readonly view: EditorView) {
      const target = createCodeMirrorModalTarget(() => this.view)
      const registrationRoot = this.view.dom.parentElement ?? this.view.dom
      this.unregister = registerModalEditor(registrationRoot, target)
      const controller = getDocumentWhichKeyController(this.view.dom.ownerDocument)
      if (controller) {
        const wkState = controller.getState()
        target.setModalState({ mode: wkState.mode, preset: wkState.preset })
      } else {
        target.setModalState({ mode: 'normal', preset: 'helix' })
      }
    }

    update(update: ViewUpdate) {
      const next = update.state.field(modalStateField, false)
      if (
        next &&
        update.transactions.some((tr) => tr.effects.some((e) => e.is(setModalStateEffect)))
      ) {
        this.view.dom.dataset.modalMode = next.mode
        this.view.dom.dataset.modalPreset = next.preset
      }
    }

    destroy() {
      this.unregister()
    }
  },
)

const modalEditorDomHandlers = EditorView.domEventHandlers({
  mousedown(event, view) {
    if (event.button !== 0 || event.metaKey || event.ctrlKey) return false
    const { clickToInsert, readOnly } = view.state.facet(modalOptionsFacet)
    if (clickToInsert && !readOnly && !view.state.readOnly) {
      const controller = getDocumentWhichKeyController(view.dom.ownerDocument)
      if (controller && controller.getState().mode === 'normal') {
        controller.setMode('insert')
      }
    }
    return false
  },
  beforeinput(event, view) {
    const modalMode = view.dom.dataset.modalMode
    if (modalMode && modalMode !== 'insert') {
      event.preventDefault()
      return true
    }
    return false
  },
})

/**
 * Returns the CodeMirror 6 extension integrating Helical UI modal editing (`normal`, `select`, `insert`, `jump`), `Tab`/`Shift-Tab` indentation, and pointer/input guards.
 */
export function modalEditorExtension(options: ModalEditorExtensionOptions = {}): Extension {
  return [
    modalOptionsFacet.of(options),
    modalStateField,
    modalClipboardExtension(),
    modalEditorViewPlugin,
    modalEditorDomHandlers,
    indentUnit.of('  '),
    keymap.of([indentWithTab]),
  ]
}
