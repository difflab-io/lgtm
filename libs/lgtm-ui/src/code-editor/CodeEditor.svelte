<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { ensureSyntaxTree, HighlightStyle, syntaxHighlighting } from '@codemirror/language'
  import { lintGutter, setDiagnostics, type Diagnostic } from '@codemirror/lint'
  import { Compartment, EditorState, type Extension } from '@codemirror/state'
  import { EditorView, hoverTooltip, keymap, tooltips } from '@codemirror/view'
  import { tags } from '@lezer/highlight'
  import { basicSetup } from 'codemirror'
  import { cn } from '@cloudvoyant/helical-ui'
  import { getCachedLanguageExtension, loadLanguageExtension } from './languages'
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

  // Props ---------------------------------------------------------------------
  let {
    value,
    filename,
    readonly = false,
    lineWrapping = 'auto',
    clickToInsert = true,
    size = 'sm',
    label = 'Code editor',
    class: className = '',
    diagnostics = [],
    selectionOffset = null,
    onchange,
    onhover,
    ongotodefinition,
    oncompletion,
    onsignaturehelp,
    onViewReady,
  }: CodeEditorProps = $props()

  // State ---------------------------------------------------------------------
  let host: HTMLDivElement | undefined = $state()
  let view: EditorView | undefined = $state()
  const readOnlyCompartment = new Compartment()
  const lineWrappingCompartment = new Compartment()
  const languageCompartment = new Compartment()
  const modalCompartment = new Compartment()
  const lspCompartment = new Compartment()

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

  // Lifecycle -----------------------------------------------------------------
  let configuredTargetName: string | undefined
  onMount(() => {
    if (!host) return
    const initialDoc = untrack(() => value)
    const initialReadOnly = untrack(() => readonly)
    const initialLineWrapping = untrack(() => lineWrapping)
    const initialTarget = untrack(() => filename ?? label)
    const initialLanguage = getCachedLanguageExtension(initialTarget)
    if (initialLanguage !== null) {
      configuredTargetName = initialTarget
    }
    const state = EditorState.create({
      doc: initialDoc,
      ...(initialAnchor !== null ? { selection: { anchor: initialAnchor } } : {}),
      extensions: [
        basicSetup,
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
        languageCompartment.of(initialLanguage ?? []),
        lineWrappingCompartment.of(toLineWrappingExtension(initialLineWrapping)),
        readOnlyCompartment.of([
          EditorState.readOnly.of(initialReadOnly),
          EditorView.editable.of(!initialReadOnly),
        ]),
        modalEditorExtension(),
        modalCompartment.of(
          modalOptionsFacet.of({
            clickToInsert: initialClickToInsert,
            readOnly: initialReadOnly,
          }),
        ),
        codeEditorLspExtension(),
        lspCompartment.of(
          lspOptionsFacet.of({
            onhover,
            ongotodefinition,
            oncompletion,
            onsignaturehelp,
          }),
        ),
        EditorView.contentAttributes.of({
          'aria-label': label,
          'data-pane-focus-target': '',
          'data-modal-editor': 'true',
        }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onchange?.(update.state.doc.toString())
          }
        }),
      ],
    })
    ensureSyntaxTree(state, state.doc.length, 100)
    view = new EditorView({ state, parent: host })
    onViewReady?.(view)
    return () => {
      onViewReady?.(null)
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
      ensureSyntaxTree(view.state, view.state.doc.length, 100)
    }
  })

  $effect(() => {
    const nextReadOnly = readonly
    const nextClickToInsert = clickToInsert
    if (view) {
      view.dispatch({
        effects: [
          readOnlyCompartment.reconfigure([
            EditorState.readOnly.of(nextReadOnly),
            EditorView.editable.of(!nextReadOnly),
          ]),
          modalCompartment.reconfigure(
            modalOptionsFacet.of({
              clickToInsert: nextClickToInsert,
              readOnly: nextReadOnly,
            }),
          ),
        ],
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
    const nextHover = onhover
    const nextGotoDefinition = ongotodefinition
    const nextCompletion = oncompletion
    const nextSignatureHelp = onsignaturehelp
    if (view) {
      view.dispatch({
        effects: lspCompartment.reconfigure(
          lspOptionsFacet.of({
            onhover: nextHover,
            ongotodefinition: nextGotoDefinition,
            oncompletion: nextCompletion,
            onsignaturehelp: nextSignatureHelp,
          }),
        ),
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
    if (!currentView || targetName === configuredTargetName) return
    const syncExtension = getCachedLanguageExtension(targetName)
    if (syncExtension !== null) {
      configuredTargetName = targetName
      currentView.dispatch({
        effects: languageCompartment.reconfigure(syncExtension),
      })
      ensureSyntaxTree(currentView.state, currentView.state.doc.length, 100)
      return
    }
    let cancelled = false
    void loadLanguageExtension(targetName).then((extension) => {
      if (!cancelled && view === currentView) {
        configuredTargetName = targetName
        currentView.dispatch({ effects: languageCompartment.reconfigure(extension) })
        ensureSyntaxTree(currentView.state, currentView.state.doc.length, 100)
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

  export function getView(): EditorView | undefined {
    return view
  }
</script>

<div
  bind:this={host}
  data-modal-ignore-pointer={!clickToInsert || readonly ? 'true' : undefined}
  class={rootClasses}
  data-slot="code-editor"
></div>
