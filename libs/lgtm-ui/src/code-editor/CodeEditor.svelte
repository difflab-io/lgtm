<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { ensureSyntaxTree } from '@codemirror/language'
  import { setDiagnostics } from '@codemirror/lint'
  import { Compartment, EditorState } from '@codemirror/state'
  import { EditorView } from '@codemirror/view'
  import { basicSetup } from 'codemirror'
  import { cn } from '@cloudvoyant/helical-ui'
  import { codeEditorLspExtension, lspOptionsFacet, toCodeMirrorDiagnostics } from './lspExtension'
  import { modalEditorExtension, modalOptionsFacet } from './modalKeymap'
  import { codeEditorRootVariants } from './styles'
  import {
    codeEditorThemeExtension,
    getCachedLanguageExtension,
    loadLanguageExtension,
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

  function warmSyntaxTree(targetView: EditorView) {
    ensureSyntaxTree(targetView.state, targetView.state.doc.length, 100)
  }

  // Lifecycle -----------------------------------------------------------------
  onMount(() => {
    if (!host) return
    const initialDoc = untrack(() => value)
    const initialReadOnly = untrack(() => readonly)
    const initialLineWrapping = untrack(() => lineWrapping)
    const initialClickToInsert = untrack(() => clickToInsert)
    const initialSelectionOffset = untrack(() => selectionOffset)
    const initialTargetName = untrack(() => filename ?? label)
    const initialLanguage = getCachedLanguageExtension(initialTargetName) ?? []
    const initialAnchor =
      initialSelectionOffset !== null && initialSelectionOffset !== undefined
        ? Math.max(0, Math.min(initialDoc.length, initialSelectionOffset))
        : null
    const state = EditorState.create({
      doc: initialDoc,
      ...(initialAnchor !== null ? { selection: { anchor: initialAnchor } } : {}),
      extensions: [
        basicSetup,
        EditorState.allowMultipleSelections.of(true),
        codeEditorThemeExtension(),
        languageCompartment.of(initialLanguage),
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
      warmSyntaxTree(view)
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
    if (!currentView) return
    const cached = getCachedLanguageExtension(targetName)
    if (cached !== null) {
      currentView.dispatch({ effects: languageCompartment.reconfigure(cached) })
      warmSyntaxTree(currentView)
      return
    }
    let cancelled = false
    void loadLanguageExtension(targetName).then((extension) => {
      if (!cancelled && view === currentView) {
        currentView.dispatch({ effects: languageCompartment.reconfigure(extension) })
        warmSyntaxTree(currentView)
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
