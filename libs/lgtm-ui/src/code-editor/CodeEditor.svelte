<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import { Compartment, EditorState } from '@codemirror/state'
  import { EditorView } from '@codemirror/view'
  import { basicSetup } from 'codemirror'
  import { cn } from '@cloudvoyant/helical-ui'
  import { codeEditorRootVariants } from './styles'
  import type { CodeEditorProps } from './types'

  // Props ---------------------------------------------------------------------
  let {
    value,
    readonly = false,
    size = 'sm',
    label = 'Code editor',
    class: className = '',
    onchange,
  }: CodeEditorProps = $props()

  // State ---------------------------------------------------------------------
  let host: HTMLDivElement | undefined = $state()
  let view: EditorView | undefined
  const readOnlyCompartment = new Compartment()

  // Derived -------------------------------------------------------------------
  const rootClasses = $derived(cn(codeEditorRootVariants({ size }), className))

  // Lifecycle -----------------------------------------------------------------
  onMount(() => {
    if (!host) return
    const initialDoc = untrack(() => value)
    const initialReadOnly = untrack(() => readonly)
    const state = EditorState.create({
      doc: initialDoc,
      extensions: [
        basicSetup,
        readOnlyCompartment.of([
          EditorState.readOnly.of(initialReadOnly),
          EditorView.editable.of(!initialReadOnly),
        ]),
        EditorView.contentAttributes.of({ 'aria-label': label }),
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

  // Methods -------------------------------------------------------------------
  export function focus() {
    view?.focus()
  }
</script>

<div bind:this={host} class={rootClasses} data-slot="code-editor"></div>
