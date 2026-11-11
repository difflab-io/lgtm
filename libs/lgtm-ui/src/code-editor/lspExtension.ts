import {
  acceptCompletion,
  autocompletion,
  closeCompletion,
  completeAnyWord,
  completionStatus,
  moveCompletionSelection,
  startCompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from '@codemirror/autocomplete'
import { forEachDiagnostic, lintGutter, type Diagnostic } from '@codemirror/lint'
import { Facet, Prec, StateEffect, StateField, type Extension } from '@codemirror/state'
import {
  EditorView,
  hoverTooltip,
  keymap,
  showTooltip,
  tooltips,
  type Tooltip,
} from '@codemirror/view'
import { renderMarkdownToDom } from './markdown'
import { codeEditorHoverTooltipVariants } from './styles'
import type {
  CodeEditorCompletionItem,
  CodeEditorDiagnostic,
  CodeEditorHoverResult,
  CodeEditorSignatureHelp,
} from './types'

/**
 * Callback handlers supplied to [`codeEditorLspExtension`] via [`lspOptionsFacet`].
 */
export interface CodeEditorLspOptions {
  /** Requests hover documentation at `offset`. */
  onhover?: (offset: number) => Promise<CodeEditorHoverResult | null>
  /** Navigates to the definition of the symbol at `offset`. */
  ongotodefinition?: (offset: number) => void | Promise<void>
  /** Requests completion candidates at `offset`. */
  oncompletion?: (offset: number) => Promise<CodeEditorCompletionItem[]>
  /** Requests callable signature help at `offset`. */
  onsignaturehelp?: (offset: number) => Promise<CodeEditorSignatureHelp | null>
}

/**
 * CodeMirror 6 facet storing active LSP callbacks for completion, hover, signature help, and goto-definition.
 */
export const lspOptionsFacet = Facet.define<CodeEditorLspOptions, CodeEditorLspOptions>({
  combine(values) {
    return Object.assign({}, ...values)
  },
})

const setEditorCursorTooltipEffect = StateEffect.define<Tooltip | null>()

/**
 * CodeMirror state field hosting keyboard-triggered hover, signature-help, and diagnostic tooltips.
 */
export const editorCursorTooltipField = StateField.define<Tooltip | null>({
  create() {
    return null
  },
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setEditorCursorTooltipEffect)) {
        return effect.value
      }
    }
    if (tr.docChanged || tr.selection) {
      return null
    }
    return value
  },
  provide: (field) => showTooltip.from(field),
})

/**
 * Converts LGTM [`CodeEditorDiagnostic`] records into CodeMirror `@codemirror/lint` [`Diagnostic`] entries.
 */
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

/**
 * Returns whether an autocomplete menu or keyboard-triggered tooltip is currently active in `view`.
 */
export function hasEditorIntellisenseActive(view: EditorView | null | undefined): boolean {
  if (!view) return false
  return (
    completionStatus(view.state) === 'active' ||
    Boolean(view.state.field(editorCursorTooltipField, false))
  )
}

/**
 * Clears any active keyboard-triggered cursor tooltip in `view`.
 */
export function clearEditorCursorTooltip(view: EditorView | null | undefined): boolean {
  if (!view) return false
  const active = view.state.field(editorCursorTooltipField, false)
  if (!active) return false
  view.dispatch({ effects: setEditorCursorTooltipEffect.of(null) })
  return true
}

/**
 * Dismisses any active autocomplete menu or keyboard-triggered cursor tooltip in `view`.
 */
export function dismissEditorIntellisense(view: EditorView | null | undefined): boolean {
  if (!view) return false
  if (completionStatus(view.state) !== null) {
    closeCompletion(view)
    return true
  }
  return clearEditorCursorTooltip(view)
}

/**
 * Explicitly starts CodeMirror autocompletion at the current cursor position in `view`.
 */
export function triggerEditorCompletion(view: EditorView | null | undefined): boolean {
  if (!view || view.state.readOnly) return false
  return startCompletion(view)
}

function buildCursorTooltip(
  view: EditorView,
  createDom: () => HTMLElement,
  options?: { pos?: number; end?: number },
): Tooltip {
  const docLen = view.state.doc.length
  const rawPos = options?.pos ?? view.state.selection.main.head
  const pos = Math.max(0, Math.min(docLen, rawPos))
  const rawEnd =
    options?.end !== undefined ? Math.max(pos, Math.min(docLen, options.end)) : undefined
  const sameLine =
    rawEnd === undefined ||
    view.state.doc.lineAt(pos).number === view.state.doc.lineAt(rawEnd).number
  const end = sameLine ? rawEnd : undefined
  return {
    pos,
    ...(end !== undefined ? { end } : {}),
    above: true,
    create() {
      return { dom: createDom() }
    },
  }
}

/**
 * Displays a keyboard-triggered hover documentation tooltip at the cursor in `view`.
 */
export function showEditorHoverTooltip(
  view: EditorView | null | undefined,
  hover: CodeEditorHoverResult,
): boolean {
  if (!view || !hover.contents.trim()) return false
  const tooltip = buildCursorTooltip(
    view,
    () => {
      const dom = document.createElement('div')
      dom.className = codeEditorHoverTooltipVariants()
      dom.setAttribute('data-slot', 'code-editor-hover')
      dom.appendChild(renderMarkdownToDom(hover.contents))
      return dom
    },
    { pos: hover.from ?? view.state.selection.main.head, end: hover.to },
  )
  view.dispatch({ effects: setEditorCursorTooltipEffect.of(tooltip) })
  return true
}

/**
 * Displays a keyboard-triggered signature help tooltip at the cursor in `view`.
 */
export function showEditorSignatureHelpTooltip(
  view: EditorView | null | undefined,
  help: CodeEditorSignatureHelp,
): boolean {
  if (!view || help.signatures.length === 0) return false
  const sig = help.signatures[help.activeSignature] ?? help.signatures[0]
  if (!sig) return false
  const markdownParts = [`\`\`\`\n${sig.label}\n\`\`\``]
  if (sig.documentation?.trim()) {
    markdownParts.push(sig.documentation.trim())
  }
  const markdown = markdownParts.join('\n\n---\n\n')
  const tooltip = buildCursorTooltip(view, () => {
    const dom = document.createElement('div')
    dom.className = codeEditorHoverTooltipVariants()
    dom.setAttribute('data-slot', 'code-editor-signature-help')
    dom.appendChild(renderMarkdownToDom(markdown))
    return dom
  })
  view.dispatch({ effects: setEditorCursorTooltipEffect.of(tooltip) })
  return true
}

interface IndexedDiagnostic {
  diagnostic: Diagnostic
  from: number
  to: number
}

function collectEditorDiagnostics(view: EditorView): IndexedDiagnostic[] {
  const items: IndexedDiagnostic[] = []
  forEachDiagnostic(view.state, (diagnostic, from, to) => {
    items.push({ diagnostic, from, to })
  })
  items.sort((a, b) => a.from - b.from || a.to - b.to)
  return items
}

function dispatchDiagnosticJump(view: EditorView, target: IndexedDiagnostic): boolean {
  const tooltip = buildCursorTooltip(
    view,
    () => {
      const dom = document.createElement('div')
      dom.className = codeEditorHoverTooltipVariants()
      dom.setAttribute('data-slot', 'code-editor-diagnostic-tooltip')
      const messageEl = document.createElement('div')
      messageEl.setAttribute('data-slot', 'code-editor-diagnostic-message')
      messageEl.appendChild(renderMarkdownToDom(target.diagnostic.message))
      dom.appendChild(messageEl)
      return dom
    },
    { pos: target.from, end: target.to },
  )
  view.dispatch({
    selection: { anchor: target.from, head: target.to },
    scrollIntoView: true,
    effects: setEditorCursorTooltipEffect.of(tooltip),
  })
  return true
}

/**
 * Jumps to the next inline diagnostic in `view` (`count` steps forward, wrapping around).
 */
export function gotoNextEditorDiagnostic(view: EditorView | null | undefined, count = 1): boolean {
  if (!view) return false
  const items = collectEditorDiagnostics(view)
  if (items.length === 0) return false
  const sel = view.state.selection.main
  let firstIdx = items.findIndex(
    (item) => item.from > sel.from || (item.from === sel.from && item.to > sel.to),
  )
  if (firstIdx === -1) {
    firstIdx = 0
  }
  const steps = Math.max(1, count) - 1
  const target = items[(firstIdx + steps) % items.length]
  if (!target) return false
  return dispatchDiagnosticJump(view, target)
}

/**
 * Jumps to the previous inline diagnostic in `view` (`count` steps backward, wrapping around).
 */
export function gotoPrevEditorDiagnostic(view: EditorView | null | undefined, count = 1): boolean {
  if (!view) return false
  const items = collectEditorDiagnostics(view)
  if (items.length === 0) return false
  const sel = view.state.selection.main
  let firstIdx = -1
  for (let i = items.length - 1; i >= 0; i -= 1) {
    const item = items[i]!
    if (item.from < sel.from || (item.from === sel.from && item.to < sel.to)) {
      firstIdx = i
      break
    }
  }
  if (firstIdx === -1) {
    firstIdx = items.length - 1
  }
  const steps = Math.max(1, count) - 1
  const targetIdx = (((firstIdx - steps) % items.length) + items.length) % items.length
  const target = items[targetIdx]
  if (!target) return false
  return dispatchDiagnosticJump(view, target)
}

/**
 * Jumps to the first inline diagnostic in `view`.
 */
export function gotoFirstEditorDiagnostic(view: EditorView | null | undefined): boolean {
  if (!view) return false
  const items = collectEditorDiagnostics(view)
  const target = items[0]
  if (!target) return false
  return dispatchDiagnosticJump(view, target)
}

/**
 * Jumps to the last inline diagnostic in `view`.
 */
export function gotoLastEditorDiagnostic(view: EditorView | null | undefined): boolean {
  if (!view) return false
  const items = collectEditorDiagnostics(view)
  const target = items[items.length - 1]
  if (!target) return false
  return dispatchDiagnosticJump(view, target)
}

async function resolveLspCompletions(context: CompletionContext): Promise<CompletionResult | null> {
  const modalMode = context.view?.dom.dataset.modalMode
  if (!context.explicit && modalMode && modalMode !== 'insert') {
    return null
  }
  const word = context.matchBefore(/[\w$]+/)
  const triggerBefore = context.matchBefore(/[.:<"/]/)
  if (!context.explicit && !word && !triggerBefore) {
    return null
  }
  const { oncompletion } = context.state.facet(lspOptionsFacet)
  if (oncompletion) {
    const items = await oncompletion(context.pos)
    if (items && items.length > 0) {
      const defaultFrom = word ? word.from : context.pos
      const docLen = context.state.doc.length
      let resultFrom = defaultFrom
      const options: Completion[] = items.map((item) => {
        if (item.from !== undefined) {
          resultFrom = Math.max(0, Math.min(docLen, item.from))
        }
        return {
          label: item.label,
          ...(item.kind ? { type: item.kind } : {}),
          ...(item.detail ? { detail: item.detail } : {}),
          ...(item.insertText && item.insertText !== item.label ? { apply: item.insertText } : {}),
          ...(item.documentation
            ? {
                info: () => {
                  const dom = document.createElement('div')
                  dom.className = codeEditorHoverTooltipVariants()
                  dom.setAttribute('data-slot', 'code-editor-completion-info')
                  dom.appendChild(renderMarkdownToDom(item.documentation!))
                  return dom
                },
              }
            : {}),
        }
      })
      return {
        from: resultFrom,
        options,
      }
    }
  }
  if (context.explicit) {
    return completeAnyWord(context)
  }
  return null
}

const lspHoverExtension = hoverTooltip(async (view, pos) => {
  const { onhover } = view.state.facet(lspOptionsFacet)
  if (!onhover) return null
  const ch = view.state.doc.sliceString(pos, pos + 1)
  if (!ch || /[\s{}[\](),;:]/.test(ch)) return null
  const hover = await onhover(pos)
  if (!hover || !hover.contents.trim()) return null
  const docLen = view.state.doc.length
  const rawFrom = hover.from !== undefined ? Math.max(0, Math.min(docLen, hover.from)) : pos
  const rawTo = hover.to !== undefined ? Math.max(rawFrom, Math.min(docLen, hover.to)) : undefined
  const sameLine =
    rawTo === undefined ||
    view.state.doc.lineAt(rawFrom).number === view.state.doc.lineAt(rawTo).number
  const from = sameLine ? rawFrom : pos
  const to = sameLine ? rawTo : undefined
  return {
    pos: from,
    ...(to !== undefined ? { end: to } : {}),
    above: true,
    create() {
      const dom = document.createElement('div')
      dom.className = codeEditorHoverTooltipVariants()
      dom.setAttribute('data-slot', 'code-editor-hover')
      dom.appendChild(renderMarkdownToDom(hover.contents))
      return { dom }
    },
  }
})

const lspIntellisenseKeymap = Prec.highest(
  keymap.of([
    {
      key: 'Escape',
      run(view) {
        return dismissEditorIntellisense(view)
      },
    },
    {
      key: 'Ctrl-e',
      run(view) {
        return hasEditorIntellisenseActive(view) ? dismissEditorIntellisense(view) : false
      },
    },
    {
      key: 'Ctrl-o',
      run(view) {
        return hasEditorIntellisenseActive(view) ? dismissEditorIntellisense(view) : false
      },
    },
    {
      key: 'Ctrl-Space',
      run(view) {
        return triggerEditorCompletion(view)
      },
    },
    {
      key: 'Ctrl-x',
      run(view) {
        return triggerEditorCompletion(view)
      },
    },
    {
      key: 'Ctrl-Shift-Space',
      run(view) {
        const { onsignaturehelp } = view.state.facet(lspOptionsFacet)
        if (!onsignaturehelp) return false
        const pos = view.state.selection.main.head
        void onsignaturehelp(pos).then((help) => {
          if (help) showEditorSignatureHelpTooltip(view, help)
        })
        return true
      },
    },
    {
      key: 'Alt-s',
      run(view) {
        const { onsignaturehelp } = view.state.facet(lspOptionsFacet)
        if (!onsignaturehelp) return false
        const pos = view.state.selection.main.head
        void onsignaturehelp(pos).then((help) => {
          if (help) showEditorSignatureHelpTooltip(view, help)
        })
        return true
      },
    },
    {
      key: 'Ctrl-n',
      run(view) {
        if (completionStatus(view.state) === 'active') {
          return moveCompletionSelection(true)(view)
        }
        return triggerEditorCompletion(view)
      },
    },
    {
      key: 'Ctrl-p',
      run(view) {
        if (completionStatus(view.state) === 'active') {
          return moveCompletionSelection(false)(view)
        }
        return triggerEditorCompletion(view)
      },
    },
    {
      key: 'Ctrl-y',
      run(view) {
        return completionStatus(view.state) === 'active' ? acceptCompletion(view) : false
      },
    },
  ]),
)

const lspNavigationKeymap = keymap.of([
  {
    key: 'F12',
    run(view) {
      const { ongotodefinition } = view.state.facet(lspOptionsFacet)
      if (!ongotodefinition) return false
      void ongotodefinition(view.state.selection.main.head)
      return true
    },
  },
])

const lspMouseHandlers = EditorView.domEventHandlers({
  mousedown(event, view) {
    const { ongotodefinition } = view.state.facet(lspOptionsFacet)
    if (ongotodefinition && (event.metaKey || event.ctrlKey) && event.button === 0) {
      const pos = view.posAtCoords({ x: event.clientX, y: event.clientY })
      if (pos === null) return false
      event.preventDefault()
      void ongotodefinition(pos)
      return true
    }
    return false
  },
})

/**
 * Returns the CodeMirror 6 extension bundling LSP completions, hover tooltips, signature help, diagnostics gutter, and goto-definition bindings.
 */
export function codeEditorLspExtension(options: CodeEditorLspOptions = {}): Extension {
  return [
    lspOptionsFacet.of(options),
    editorCursorTooltipField,
    lintGutter(),
    tooltips({
      tooltipSpace(view) {
        const rect = view.scrollDOM.getBoundingClientRect()
        const pad = 8
        return {
          top: rect.top + pad,
          left: rect.left + pad,
          bottom: Math.max(rect.top + pad, rect.bottom - pad),
          right: Math.max(rect.left + pad, rect.right - pad),
        }
      },
    }),
    autocompletion({
      override: [resolveLspCompletions],
      interactionDelay: 0,
      activateOnTyping: true,
    }),
    lspHoverExtension,
    lspIntellisenseKeymap,
    lspNavigationKeymap,
    lspMouseHandlers,
  ]
}
