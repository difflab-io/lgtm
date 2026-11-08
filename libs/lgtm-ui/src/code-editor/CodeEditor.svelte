<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { markdown } from '@codemirror/lang-markdown'
  import {
    HighlightStyle,
    LanguageDescription,
    matchBrackets,
    syntaxHighlighting,
  } from '@codemirror/language'
  import { languages } from '@codemirror/language-data'
  import { lintGutter, setDiagnostics, type Diagnostic } from '@codemirror/lint'
  import { Compartment, EditorState, RangeSetBuilder, type Extension } from '@codemirror/state'
  import {
    Decoration,
    type DecorationSet,
    EditorView,
    hoverTooltip,
    keymap,
    tooltips,
    ViewPlugin,
    type ViewUpdate,
  } from '@codemirror/view'
  import { tags } from '@lezer/highlight'
  import { basicSetup } from 'codemirror'
  import { cn } from '@cloudvoyant/helical-ui'
  import { renderMarkdownToDom } from './markdown'
  import { codeEditorHoverTooltipVariants, codeEditorRootVariants } from './styles'
  import type { CodeEditorDiagnostic, CodeEditorLineWrapping, CodeEditorProps } from './types'

  const codeEditorHighlightStyle = HighlightStyle.define([
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

  const codeEditorTooltipTheme = EditorView.theme({
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
    '.cm-gutters, &.cm-editor .cm-gutters': {
      backgroundColor: 'var(--color-background, #101010)',
      color: 'var(--cm-gutter-fg, var(--color-muted-foreground, #676767))',
      borderColor: 'var(--color-border, #2e2e2e)',
    },
    '.cm-activeLine, &.cm-editor .cm-activeLine': {
      backgroundColor: 'var(--cm-active-line-bg, var(--color-muted, #1a1a1a))',
    },
    '.cm-activeLineGutter, &.cm-editor .cm-activeLineGutter': {
      backgroundColor: 'var(--color-background, #101010)',
      color: 'var(--cm-gutter-active-fg, var(--color-foreground, #ededed))',
      fontWeight: '600',
    },
    '&.cm-focused .cm-matchingBracket, .cm-matchingBracket': {
      backgroundColor: 'color-mix(in oklab, var(--color-foreground, #ededed) 12%, transparent)',
      color: 'var(--color-foreground, #ededed)',
      textDecoration: 'none',
      fontWeight: '700',
      borderRadius: '2px',
    },
    '&.cm-focused .cm-nonmatchingBracket, .cm-nonmatchingBracket': {
      backgroundColor: 'color-mix(in oklab, var(--color-danger, #f32e40) 20%, transparent)',
      color: 'var(--color-danger, #f32e40)',
      textDecoration: 'none',
      borderRadius: '2px',
    },
    '.cm-line.cm-bracket-guide-open, .cm-line.cm-bracket-guide-mid, .cm-line.cm-bracket-guide-close':
      {
        position: 'relative',
      },
    '.cm-line.cm-bracket-guide-open::before, .cm-line.cm-bracket-guide-mid::before, .cm-line.cm-bracket-guide-close::before':
      {
        content: '""',
        position: 'absolute',
        left: 'calc(6px + var(--cm-guide-col, 0ch) + 0.45ch)',
        width: '0',
        borderLeft:
          '1px solid color-mix(in oklab, var(--color-foreground, #ededed) 28%, transparent)',
        pointerEvents: 'none',
      },
    '.cm-line.cm-bracket-guide-open::before': {
      top: '78%',
      bottom: '0',
    },
    '.cm-line.cm-bracket-guide-mid::before': {
      top: '0',
      bottom: '0',
    },
    '.cm-line.cm-bracket-guide-close::before': {
      top: '0',
      bottom: '78%',
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

  // Props ---------------------------------------------------------------------
  let {
    value,
    filename,
    readonly = false,
    lineWrapping = 'auto',
    size = 'sm',
    label = 'Code editor',
    class: className = '',
    diagnostics = [],
    selectionOffset = null,
    onchange,
    onhover,
    ongotodefinition,
  }: CodeEditorProps = $props()

  // State ---------------------------------------------------------------------
  let host: HTMLDivElement | undefined = $state()
  let view: EditorView | undefined = $state()
  const readOnlyCompartment = new Compartment()
  const lineWrappingCompartment = new Compartment()
  const languageCompartment = new Compartment()

  // Derived -------------------------------------------------------------------
  const rootClasses = $derived(cn(codeEditorRootVariants({ size }), className))

  function toCodeMirrorDiagnostics(docLength: number, items: CodeEditorDiagnostic[]): Diagnostic[] {
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

  function isMarkdownTarget(targetName: string): boolean {
    const lower = targetName.toLowerCase()
    return (
      lower.endsWith('.md') ||
      lower.endsWith('.markdown') ||
      lower.endsWith('.mdx') ||
      lower.endsWith('.mkd')
    )
  }

  function toLineWrappingExtension(mode: CodeEditorLineWrapping): Extension {
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

  function leadingIndentColumns(lineText: string): number {
    let cols = 0
    for (let i = 0; i < lineText.length; i++) {
      const ch = lineText[i]
      if (ch === ' ') cols += 1
      else if (ch === '\t') cols += 2
      else break
    }
    return cols
  }

  function findActiveBracketPair(state: EditorState): { open: number; close: number } | null {
    const head = state.selection.main.head
    for (const dir of [-1, 1] as const) {
      const direct = matchBrackets(state, head, dir)
      if (direct?.matched && direct.start && direct.end) {
        return {
          open: Math.min(direct.start.from, direct.end.from),
          close: Math.max(direct.start.from, direct.end.from),
        }
      }
    }

    const scanLimit = 8000
    const minPos = Math.max(0, head - scanLimit)
    const slice = state.doc.sliceString(minPos, head)
    const stack: string[] = []
    for (let i = slice.length - 1; i >= 0; i--) {
      const ch = slice[i]
      if (ch === '}' || ch === ']' || ch === ')') {
        stack.push(ch)
      } else if (ch === '{' || ch === '[' || ch === '(') {
        const expected = ch === '{' ? '}' : ch === '[' ? ']' : ')'
        if (stack.length > 0 && stack[stack.length - 1] === expected) {
          stack.pop()
        } else {
          const openPos = minPos + i
          const matched = matchBrackets(state, openPos, 1)
          if (matched?.matched && matched.end && matched.end.from >= head) {
            return { open: openPos, close: matched.end.from }
          }
        }
      }
    }
    return null
  }

  function buildBracketGuideDecorations(state: EditorState): DecorationSet {
    const pair = findActiveBracketPair(state)
    if (!pair) return Decoration.none

    const openLine = state.doc.lineAt(pair.open)
    const closeLine = state.doc.lineAt(pair.close)
    const builder = new RangeSetBuilder<Decoration>()
    const mark = Decoration.mark({ class: 'cm-matchingBracket' })

    if (openLine.number === closeLine.number) {
      builder.add(pair.open, pair.open + 1, mark)
      builder.add(pair.close, pair.close + 1, mark)
      return builder.finish()
    }

    const guideCol = Math.min(
      leadingIndentColumns(openLine.text),
      leadingIndentColumns(closeLine.text),
    )
    const style = `--cm-guide-col: ${guideCol}ch`

    builder.add(
      openLine.from,
      openLine.from,
      Decoration.line({ attributes: { class: 'cm-bracket-guide-open', style } }),
    )
    builder.add(pair.open, pair.open + 1, mark)

    for (let lineNum = openLine.number + 1; lineNum < closeLine.number; lineNum++) {
      const line = state.doc.line(lineNum)
      builder.add(
        line.from,
        line.from,
        Decoration.line({ attributes: { class: 'cm-bracket-guide-mid', style } }),
      )
    }

    builder.add(
      closeLine.from,
      closeLine.from,
      Decoration.line({ attributes: { class: 'cm-bracket-guide-close', style } }),
    )
    builder.add(pair.close, pair.close + 1, mark)

    return builder.finish()
  }

  const bracketGuideExtension = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet
      constructor(currentView: EditorView) {
        this.decorations = buildBracketGuideDecorations(currentView.state)
      }
      update(update: ViewUpdate) {
        if (update.docChanged || update.selectionSet) {
          this.decorations = buildBracketGuideDecorations(update.state)
        }
      }
    },
    {
      decorations: (plugin) => plugin.decorations,
    },
  )

  // Lifecycle -----------------------------------------------------------------
  onMount(() => {
    if (!host) return
    const initialDoc = untrack(() => value)
    const initialReadOnly = untrack(() => readonly)
    const initialLineWrapping = untrack(() => lineWrapping)
    const state = EditorState.create({
      doc: initialDoc,
      extensions: [
        basicSetup,
        bracketGuideExtension,
        syntaxHighlighting(codeEditorHighlightStyle),
        codeEditorTooltipTheme,
        tooltips({
          tooltipSpace(currentView) {
            const rect = currentView.scrollDOM.getBoundingClientRect()
            const pad = 8
            return {
              top: rect.top + pad,
              left: rect.left + pad,
              bottom: Math.max(rect.top + pad, rect.bottom - pad),
              right: Math.max(rect.left + pad, rect.right - pad),
            }
          },
        }),
        lintGutter(),
        languageCompartment.of([]),
        lineWrappingCompartment.of(toLineWrappingExtension(initialLineWrapping)),
        readOnlyCompartment.of([
          EditorState.readOnly.of(initialReadOnly),
          EditorView.editable.of(!initialReadOnly),
        ]),
        EditorView.contentAttributes.of({ 'aria-label': label }),
        hoverTooltip(async (_view, pos) => {
          if (!onhover) return null
          const ch = _view.state.doc.sliceString(pos, pos + 1)
          if (!ch || /[\s{}[\](),;:]/.test(ch)) return null
          const hover = await onhover(pos)
          if (!hover || !hover.contents.trim()) return null
          const docLen = _view.state.doc.length
          const rawFrom = hover.from !== undefined ? Math.max(0, Math.min(docLen, hover.from)) : pos
          const rawTo =
            hover.to !== undefined ? Math.max(rawFrom, Math.min(docLen, hover.to)) : undefined
          const sameLine =
            rawTo === undefined ||
            _view.state.doc.lineAt(rawFrom).number === _view.state.doc.lineAt(rawTo).number
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
        }),
        keymap.of([
          {
            key: 'F12',
            run(currentView) {
              if (!ongotodefinition) return false
              const pos = currentView.state.selection.main.head
              void ongotodefinition(pos)
              return true
            },
          },
        ]),
        EditorView.domEventHandlers({
          mousedown(event, currentView) {
            if (!ongotodefinition) return false
            if (!(event.metaKey || event.ctrlKey) || event.button !== 0) return false
            const pos = currentView.posAtCoords({ x: event.clientX, y: event.clientY })
            if (pos === null) return false
            event.preventDefault()
            void ongotodefinition(pos)
            return true
          },
        }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onchange?.(update.state.doc.toString())
          }
        }),
      ],
    })
    view = new EditorView({ state, parent: host })
    return () => {
      view?.destroy()
      view = undefined
    }
  })

  $effect(() => {
    const nextValue = value
    if (view && view.state.doc.toString() !== nextValue) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: nextValue },
      })
    }
  })

  $effect(() => {
    const nextReadOnly = readonly
    if (view) {
      view.dispatch({
        effects: readOnlyCompartment.reconfigure([
          EditorState.readOnly.of(nextReadOnly),
          EditorView.editable.of(!nextReadOnly),
        ]),
      })
    }
  })

  $effect(() => {
    const nextLineWrapping = lineWrapping
    if (view) {
      view.dispatch({
        effects: lineWrappingCompartment.reconfigure(toLineWrappingExtension(nextLineWrapping)),
      })
    }
  })

  $effect(() => {
    const currentView = view
    const nextDiagnostics = diagnostics
    if (!currentView) return
    const mapped = toCodeMirrorDiagnostics(currentView.state.doc.length, nextDiagnostics)
    currentView.dispatch(setDiagnostics(currentView.state, mapped))
  })

  $effect(() => {
    const currentView = view
    const targetOffset = selectionOffset
    if (!currentView || targetOffset === null || targetOffset === undefined) return
    const clamped = Math.max(0, Math.min(currentView.state.doc.length, targetOffset))
    currentView.dispatch({
      selection: { anchor: clamped },
      scrollIntoView: true,
    })
  })

  $effect(() => {
    const currentView = view
    const targetName = filename ?? label
    if (!currentView) return
    if (targetName && isMarkdownTarget(targetName)) {
      currentView.dispatch({
        effects: languageCompartment.reconfigure(markdown({ codeLanguages: languages })),
      })
      return
    }
    const description = targetName ? LanguageDescription.matchFilename(languages, targetName) : null
    if (!description) {
      currentView.dispatch({ effects: languageCompartment.reconfigure([]) })
      return
    }
    let cancelled = false
    void description.load().then((support) => {
      if (!cancelled && view === currentView) {
        currentView.dispatch({ effects: languageCompartment.reconfigure(support) })
      }
    })
    return () => {
      cancelled = true
    }
  })

  // Methods -------------------------------------------------------------------
  export function focus() {
    view?.focus()
  }
</script>

<div bind:this={host} class={rootClasses} data-slot="code-editor"></div>
