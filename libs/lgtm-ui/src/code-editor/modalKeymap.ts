import { redo, undo } from '@codemirror/commands'
import { EditorSelection, type SelectionRange, type Text } from '@codemirror/state'
import { EditorView } from '@codemirror/view'
import type {
  KeybindingPreset,
  KeymapMode,
  ModalEditorTarget,
  ModalMotion,
  ModalOperator,
  ModalSelectionTransform,
  ModalTextObjectSpec,
} from '@cloudvoyant/helical-ui'

let lastYankedText = ''

function isWordChar(ch: string): boolean {
  return /[A-Za-z0-9_]/.test(ch)
}

function isWhitespace(ch: string): boolean {
  return ch === ' ' || ch === '\t' || ch === '\n' || ch === '\r'
}

function charKind(ch: string): 'word' | 'space' | 'punct' {
  if (!ch || isWhitespace(ch)) return 'space'
  if (isWordChar(ch)) return 'word'
  return 'punct'
}

function stepWordForward(doc: Text, pos: number): { from: number; to: number } {
  const text = doc.toString()
  const len = text.length
  let i = Math.max(0, Math.min(len, pos))
  if (i >= len) return { from: len, to: len }

  const startKind = charKind(text[i] ?? '')
  if (startKind !== 'space') {
    while (i < len && charKind(text[i] ?? '') === startKind) {
      i += 1
    }
  }
  while (i < len && charKind(text[i] ?? '') === 'space') {
    i += 1
  }
  const wordStart = i
  if (i >= len) {
    return { from: pos, to: len }
  }
  const targetKind = charKind(text[i] ?? '')
  while (i < len && charKind(text[i] ?? '') === targetKind) {
    i += 1
  }
  return { from: wordStart, to: i }
}

function stepWordEnd(doc: Text, pos: number): { from: number; to: number } {
  const text = doc.toString()
  const len = text.length
  let i = Math.max(0, Math.min(len, pos))
  if (i >= len) return { from: len, to: len }

  if (i < len && charKind(text[i] ?? '') !== 'space') {
    const nextKind = charKind(text[i + 1] ?? '')
    if (nextKind !== charKind(text[i] ?? '')) {
      i += 1
    }
  }
  while (i < len && charKind(text[i] ?? '') === 'space') {
    i += 1
  }
  const wordStart = i
  if (i >= len) return { from: pos, to: len }
  const kind = charKind(text[i] ?? '')
  while (i < len && charKind(text[i] ?? '') === kind) {
    i += 1
  }
  return { from: wordStart, to: i }
}

function stepWordBackward(doc: Text, pos: number): { from: number; to: number } {
  const text = doc.toString()
  const len = text.length
  let i = Math.max(0, Math.min(len, pos))
  if (i <= 0) return { from: 0, to: 0 }

  i -= 1
  while (i > 0 && charKind(text[i] ?? '') === 'space') {
    i -= 1
  }
  const wordEnd = i + 1
  const kind = charKind(text[i] ?? '')
  while (i > 0 && charKind(text[i - 1] ?? '') === kind) {
    i -= 1
  }
  return { from: i, to: wordEnd }
}

function stepParagraph(doc: Text, pos: number, dir: 1 | -1): number {
  const line = doc.lineAt(pos)
  let lineNum = line.number
  const isBlank = (n: number) => doc.line(n).text.trim().length === 0

  while (lineNum >= 1 && lineNum <= doc.lines && isBlank(lineNum)) {
    lineNum += dir
  }
  while (lineNum >= 1 && lineNum <= doc.lines && !isBlank(lineNum)) {
    lineNum += dir
  }
  const clamped = Math.max(1, Math.min(doc.lines, lineNum))
  return dir > 0 ? doc.line(clamped).to : doc.line(clamped).from
}

function findWordObject(
  doc: Text,
  pos: number,
  around: boolean,
): { from: number; to: number } | null {
  const line = doc.lineAt(pos)
  const text = line.text
  if (text.length === 0) return null
  const offset = Math.min(Math.max(0, pos - line.from), Math.max(0, text.length - 1))
  const kind = charKind(text[offset] ?? '')
  let start = offset
  let end = offset + 1
  while (start > 0 && charKind(text[start - 1] ?? '') === kind) {
    start -= 1
  }
  while (end < text.length && charKind(text[end] ?? '') === kind) {
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

function findCodeblockOrBracketObject(
  doc: Text,
  pos: number,
  around: boolean,
): { from: number; to: number } | null {
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

  const text = doc.toString()
  const pairs: Array<[string, string]> = [
    ['{', '}'],
    ['(', ')'],
    ['[', ']'],
  ]
  for (const [openCh, closeCh] of pairs) {
    let depth = 0
    let openIdx = -1
    for (let i = pos; i >= 0; i -= 1) {
      if (text[i] === closeCh && i !== pos) depth += 1
      else if (text[i] === openCh) {
        if (depth === 0) {
          openIdx = i
          break
        }
        depth -= 1
      }
    }
    if (openIdx !== -1) {
      depth = 0
      for (let i = openIdx + 1; i < text.length; i += 1) {
        if (text[i] === openCh) depth += 1
        else if (text[i] === closeCh) {
          if (depth === 0) {
            return around ? { from: openIdx, to: i + 1 } : { from: openIdx + 1, to: i }
          }
          depth -= 1
        }
      }
    }
  }
  return findParagraphObject(doc, pos, around)
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
            let cur = head
            let span = { from: head, to: head }
            for (let s = 0; s < steps; s += 1) {
              span = stepWordForward(doc, cur)
              cur = span.to
            }
            if (extend) return EditorSelection.range(anchor, span.to)
            if (isSelectionFirst) return EditorSelection.range(span.from, span.to)
            return EditorSelection.cursor(span.from)
          }
          case 'wordEnd': {
            let cur = head
            let span = { from: head, to: head }
            for (let s = 0; s < steps; s += 1) {
              span = stepWordEnd(doc, cur)
              cur = span.to
            }
            if (extend) return EditorSelection.range(anchor, span.to)
            if (isSelectionFirst) return EditorSelection.range(span.from, span.to)
            return EditorSelection.cursor(Math.max(span.from, span.to - 1))
          }
          case 'wordBackward': {
            let cur = head
            let span = { from: head, to: head }
            for (let s = 0; s < steps; s += 1) {
              span = stepWordBackward(doc, cur)
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
            if (extend) return EditorSelection.range(anchor, cur)
            if (isSelectionFirst) return EditorSelection.range(head, cur)
            return EditorSelection.cursor(cur)
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
              !range.empty &&
              range.from === startLine.from &&
              (range.to === curEndLine.to || range.to === Math.min(doc.length, curEndLine.to + 1))
            const targetEndLineNum = alreadyFullLine
              ? Math.min(doc.lines, curEndLine.number + steps)
              : Math.min(doc.lines, startLine.number + steps - 1)
            const endLine = doc.line(targetEndLineNum)
            const to = Math.min(doc.length, endLine.to + (endLine.number < doc.lines ? 1 : 0))
            return EditorSelection.range(startLine.from, to)
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
      const around = spec.startsWith('a')
      const kind = spec[1]
      let changed = false

      const nextRanges = selection.ranges.map((range) => {
        let match: { from: number; to: number } | null = null
        if (kind === 'w') {
          match = findWordObject(doc, range.head, around)
        } else if (kind === 'p') {
          match = findParagraphObject(doc, range.head, around)
        } else if (kind === 'c' || kind === 'n') {
          match = findCodeblockOrBracketObject(doc, range.head, around)
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
        lastYankedText = yankedText
        if (yankedText && typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
          void navigator.clipboard.writeText(yankedText).catch(() => {})
        }
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
        if (yankedChunks.length > 0) {
          lastYankedText = yankedChunks.join('\n')
        }
        if (changes.length > 0) {
          view.dispatch({ changes, scrollIntoView: true })
        }
        return op === 'change' ? { enterInsert: true } : true
      }

      if (op === 'pasteAfter' || op === 'pasteBefore') {
        if (!lastYankedText) return false
        const repeated = lastYankedText.repeat(steps)
        const isLinewise = lastYankedText.endsWith('\n')
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

    setModalState(modalState: { mode: KeymapMode; preset: KeybindingPreset }): void {
      const view = getView()
      if (!view) return
      view.dom.dataset.modalMode = modalState.mode
      view.dom.dataset.modalPreset = modalState.preset
    },
  }
}
