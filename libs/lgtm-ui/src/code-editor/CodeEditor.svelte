<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { markdown } from '@codemirror/lang-markdown'
  import {
    ensureSyntaxTree,
    HighlightStyle,
    LanguageDescription,
    type LanguageSupport,
    syntaxHighlighting,
  } from '@codemirror/language'
  import { languages } from '@codemirror/language-data'
  import { lintGutter, setDiagnostics, type Diagnostic } from '@codemirror/lint'
  import { Compartment, EditorState, type Extension } from '@codemirror/state'
  import { EditorView, hoverTooltip, keymap, tooltips } from '@codemirror/view'
  import { tags } from '@lezer/highlight'
  import { basicSetup } from 'codemirror'
  import { cn } from '@cloudvoyant/helical-ui'
  import { codeEditorLspExtension, lspOptionsFacet, toCodeMirrorDiagnostics } from './lspExtension'
  import { modalEditorExtension, modalOptionsFacet } from './modalKeymap'
  import { codeEditorRootVariants } from './styles'
  import {
    codeEditorThemeExtension,
    resolveLanguageExtension,
    toLineWrappingExtension,
  } from './themeExtension'
  import type { CodeEditorProps } from './types'

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

  function isMarkdownTarget(targetName: string): boolean {
    const lower = targetName.toLowerCase()
    return (
      lower.endsWith('.md') ||
      lower.endsWith('.markdown') ||
      lower.endsWith('.mdx') ||
      lower.endsWith('.mkd')
    )
  }

  const loadedLanguageSupport = new Map<LanguageDescription, LanguageSupport>()
  for (const sample of [
    'file.rs',
    'file.ts',
    'file.tsx',
    'file.js',
    'file.json',
    'file.toml',
    'file.html',
    'file.css',
    'file.py',
    'file.yaml',
    'file.sh',
  ]) {
    const desc = LanguageDescription.matchFilename(languages, sample)
    if (desc && !desc.support) {
      void desc.load().then((support) => {
        loadedLanguageSupport.set(desc, support)
      })
    }
  }

  function resolveSyncLanguageExtension(targetName: string | undefined): Extension | null {
    if (!targetName) return []
    if (isMarkdownTarget(targetName)) {
      return markdown({ codeLanguages: languages })
    }
    const description = LanguageDescription.matchFilename(languages, targetName)
    if (!description) return []
    const cached = description.support ?? loadedLanguageSupport.get(description)
    return cached ?? null
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
    const initialLanguage = resolveSyncLanguageExtension(initialTarget)
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
    const syncExtension = resolveSyncLanguageExtension(targetName)
    if (syncExtension !== null) {
      configuredTargetName = targetName
      currentView.dispatch({
        effects: languageCompartment.reconfigure(syncExtension),
      })
      ensureSyntaxTree(currentView.state, currentView.state.doc.length, 100)
      return
    }
    const description = targetName ? LanguageDescription.matchFilename(languages, targetName) : null
    if (!description) {
      configuredTargetName = targetName
      currentView.dispatch({ effects: languageCompartment.reconfigure([]) })
      return
    }
    let cancelled = false
    void description.load().then((support) => {
      loadedLanguageSupport.set(description, support)
      if (!cancelled && view === currentView) {
        configuredTargetName = targetName
        currentView.dispatch({ effects: languageCompartment.reconfigure(support) })
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
