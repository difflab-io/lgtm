<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { markdown } from '@codemirror/lang-markdown'
  import { HighlightStyle, LanguageDescription, syntaxHighlighting } from '@codemirror/language'
  import { languages } from '@codemirror/language-data'
  import { lintGutter, setDiagnostics, type Diagnostic } from '@codemirror/lint'
  import { Compartment, EditorState } from '@codemirror/state'
  import { EditorView, hoverTooltip, keymap, tooltips } from '@codemirror/view'
  import { tags } from '@lezer/highlight'
  import { basicSetup } from 'codemirror'
  import { cn } from '@cloudvoyant/helical-ui'
  import { renderMarkdownToDom } from './markdown'
  import { codeEditorHoverTooltipVariants, codeEditorRootVariants } from './styles'
  import type { CodeEditorDiagnostic, CodeEditorProps } from './types'

  const codeEditorHighlightStyle = HighlightStyle.define([
    {
      tag: [tags.heading1, tags.heading2, tags.heading3, tags.heading],
      color: '#1d4ed8',
      fontWeight: '600',
    },
    {
      tag: tags.monospace,
      color: '#be185d',
      backgroundColor: 'rgba(15, 23, 42, 0.05)',
      borderRadius: '3px',
    },
    { tag: tags.quote, color: '#475569', fontStyle: 'italic' },
    { tag: [tags.link, tags.url], color: '#0284c7', textDecoration: 'underline' },
    { tag: tags.strong, fontWeight: '600', color: '#0f172a' },
    { tag: tags.emphasis, fontStyle: 'italic' },
    { tag: tags.strikethrough, textDecoration: 'line-through', color: '#64748b' },
    { tag: [tags.processingInstruction, tags.contentSeparator], color: '#64748b' },
  ])

  const codeEditorTooltipTheme = EditorView.theme({
    '.cm-tooltip': {
      border: '1px solid var(--color-border, #e2e8f0)',
      borderRadius: '0.375rem',
      backgroundColor: 'var(--color-popover, #ffffff)',
      color: 'var(--color-popover-foreground, #0f172a)',
      boxShadow: '0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1)',
      overflowY: 'auto',
      overscrollBehavior: 'contain',
      maxWidth: 'min(32rem, calc(100vw - 2rem))',
    },
    '.cm-tooltip.cm-tooltip-hover': {
      border: '1px solid var(--color-border, #e2e8f0)',
      borderRadius: '0.375rem',
      backgroundColor: 'var(--color-popover, #ffffff)',
    },
    '.cm-tooltip .cm-tooltip-section': {
      border: 'none',
      backgroundColor: 'transparent',
    },
    '.cm-tooltip .cm-tooltip-section:not(:first-child)': {
      borderTop: '1px solid var(--color-border, #e2e8f0)',
    },
  })

  // Props ---------------------------------------------------------------------
  let {
    value,
    filename,
    readonly = false,
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

  // Lifecycle -----------------------------------------------------------------
  onMount(() => {
    if (!host) return
    const initialDoc = untrack(() => value)
    const initialReadOnly = untrack(() => readonly)
    const state = EditorState.create({
      doc: initialDoc,
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
        languageCompartment.of([]),
        readOnlyCompartment.of([
          EditorState.readOnly.of(initialReadOnly),
          EditorView.editable.of(!initialReadOnly),
        ]),
        EditorView.contentAttributes.of({ 'aria-label': label }),
        hoverTooltip(async (_view, pos) => {
          if (!onhover) return null
          const hover = await onhover(pos)
          if (!hover || !hover.contents.trim()) return null
          const docLen = _view.state.doc.length
          const from = hover.from !== undefined ? Math.max(0, Math.min(docLen, hover.from)) : pos
          const to = hover.to !== undefined ? Math.max(from, Math.min(docLen, hover.to)) : undefined
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
