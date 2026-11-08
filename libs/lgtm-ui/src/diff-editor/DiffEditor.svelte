<script lang="ts">
  import { onMount } from 'svelte'
  import { markdown } from '@codemirror/lang-markdown'
  import { HighlightStyle, LanguageDescription, syntaxHighlighting } from '@codemirror/language'
  import { languages } from '@codemirror/language-data'
  import { lintGutter, setDiagnostics, type Diagnostic } from '@codemirror/lint'
  import { MergeView, unifiedMergeView } from '@codemirror/merge'
  import { Compartment, EditorState, type Extension } from '@codemirror/state'
  import { EditorView, hoverTooltip, keymap, tooltips } from '@codemirror/view'
  import { tags } from '@lezer/highlight'
  import { basicSetup } from 'codemirror'
  import { cn, getDocumentWhichKeyController, registerModalEditor } from '@cloudvoyant/helical-ui'
  import { renderMarkdownToDom } from '../code-editor/markdown'
  import {
    createCodeMirrorModalTarget,
    editorCursorTooltipField,
    goToNextHunk as stepNextHunk,
    goToPrevHunk as stepPrevHunk,
  } from '../code-editor/modalKeymap'
  import { codeEditorHoverTooltipVariants } from '../code-editor/styles'
  import type { CodeEditorDiagnostic, CodeEditorLineWrapping } from '../code-editor/types'
  import { diffEditorRootVariants } from './styles'
  import type { DiffEditorMode, DiffEditorProps } from './types'

  const diffEditorHighlightStyle = HighlightStyle.define([
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
    { tag: tags.inserted, color: 'var(--cm-syntax-inserted, #2ebd6b)' },
    { tag: tags.deleted, color: 'var(--cm-syntax-deleted, #f32e40)' },
    { tag: tags.changed, color: 'var(--cm-syntax-changed, #ffae00)' },
  ])

  const diffEditorTheme = EditorView.theme({
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
    '&.cm-merge-a .cm-changedLine, .cm-deletedChunk': {
      backgroundColor: 'var(--cm-diff-del-line, rgba(244, 63, 94, 0.14))',
    },
    '&.cm-merge-a .cm-changedText, .cm-deletedChunk .cm-changedText': {
      backgroundColor: 'var(--cm-diff-del-word, rgba(244, 63, 94, 0.32))',
    },
    '&.cm-merge-b .cm-changedLine': {
      backgroundColor: 'var(--cm-diff-add-line, rgba(16, 185, 129, 0.14))',
    },
    '&.cm-merge-b .cm-changedText': {
      backgroundColor: 'var(--cm-diff-add-word, rgba(16, 185, 129, 0.32))',
    },
    '.cm-collapsedLines': {
      backgroundColor: 'var(--color-muted, #1a1a1a)',
      color: 'var(--color-muted-foreground, #a1a1a1)',
      borderTop: '1px solid var(--color-border, #2e2e2e)',
      borderBottom: '1px solid var(--color-border, #2e2e2e)',
      cursor: 'pointer',
    },
  })

  // Props ---------------------------------------------------------------------
  let {
    baseValue,
    headValue = '',
    filename,
    mode = $bindable<DiffEditorMode>('split'),
    showFullFile = $bindable(false),
    readonly = true,
    lineWrapping = 'auto',
    clickToInsert = true,
    collapseUnchangedMargin = 3,
    collapseUnchangedMinSize = 4,
    size = 'sm',
    label = 'Diff editor',
    class: className = '',
    diagnostics = [],
    onchange,
    onsave,
    onhover,
    ongotodefinition,
    onmodetoggle,
    onfullfiletoggle,
    onViewReady,
  }: DiffEditorProps = $props()

  // State ---------------------------------------------------------------------
  let host: HTMLDivElement | undefined = $state()
  let mergeInstance: MergeView | undefined
  let unifiedInstance: EditorView | undefined
  let mountedMode: DiffEditorMode | undefined
  let mountedFullFile: boolean | undefined
  let scrollCleanup: (() => void) | undefined
  const modalTarget = createCodeMirrorModalTarget(() => mergeInstance?.b ?? unifiedInstance)

  const baseLanguageCompartment = new Compartment()
  const headLanguageCompartment = new Compartment()
  const baseWrapCompartment = new Compartment()
  const headWrapCompartment = new Compartment()
  const headReadOnlyCompartment = new Compartment()

  // Derived -------------------------------------------------------------------
  const rootClasses = $derived(cn(diffEditorRootVariants({ size, mode }), className))

  function isMarkdownTarget(targetName: string): boolean {
    const lower = targetName.toLowerCase()
    return (
      lower.endsWith('.md') ||
      lower.endsWith('.markdown') ||
      lower.endsWith('.mdx') ||
      lower.endsWith('.mkd')
    )
  }

  function toLineWrappingExtension(wrapMode: CodeEditorLineWrapping): Extension {
    if (!wrapMode || wrapMode === 'off') {
      return []
    }
    if (typeof wrapMode === 'number' && Number.isFinite(wrapMode) && wrapMode > 0) {
      const cols = Math.max(1, Math.floor(wrapMode))
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

  function buildCollapseConfig(fullFile: boolean) {
    return fullFile
      ? undefined
      : {
          margin: collapseUnchangedMargin,
          minSize: collapseUnchangedMinSize,
        }
  }

  function bindSplitScrollSync(aView: EditorView, bView: EditorView): () => void {
    let syncing = false
    const syncFromA = () => {
      if (syncing) return
      syncing = true
      bView.scrollDOM.scrollTop = aView.scrollDOM.scrollTop
      bView.scrollDOM.scrollLeft = aView.scrollDOM.scrollLeft
      requestAnimationFrame(() => {
        syncing = false
      })
    }
    const syncFromB = () => {
      if (syncing) return
      syncing = true
      aView.scrollDOM.scrollTop = bView.scrollDOM.scrollTop
      aView.scrollDOM.scrollLeft = bView.scrollDOM.scrollLeft
      requestAnimationFrame(() => {
        syncing = false
      })
    }
    aView.scrollDOM.addEventListener('scroll', syncFromA, { passive: true })
    bView.scrollDOM.addEventListener('scroll', syncFromB, { passive: true })
    return () => {
      aView.scrollDOM.removeEventListener('scroll', syncFromA)
      bView.scrollDOM.removeEventListener('scroll', syncFromB)
    }
  }

  function buildBaseExtensions(): Extension[] {
    return [
      basicSetup,
      syntaxHighlighting(diffEditorHighlightStyle),
      diffEditorTheme,
      baseLanguageCompartment.of([]),
      baseWrapCompartment.of(toLineWrappingExtension(lineWrapping)),
      EditorState.readOnly.of(true),
      EditorView.editable.of(false),
      EditorView.contentAttributes.of({ 'aria-label': `${label} (base)` }),
    ]
  }

  function buildHeadExtensions(isReadOnly: boolean): Extension[] {
    return [
      basicSetup,
      EditorState.allowMultipleSelections.of(true),
      syntaxHighlighting(diffEditorHighlightStyle),
      diffEditorTheme,
      editorCursorTooltipField,
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
      headLanguageCompartment.of([]),
      headWrapCompartment.of(toLineWrappingExtension(lineWrapping)),
      headReadOnlyCompartment.of([
        EditorState.readOnly.of(isReadOnly),
        EditorView.editable.of(!isReadOnly),
      ]),
      EditorView.contentAttributes.of({
        'aria-label': `${label} (head)`,
        'data-pane-focus-target': '',
        'data-modal-editor': 'true',
      }),
      hoverTooltip(async (_view, pos) => {
        if (!onhover) return null
        const ch = _view.state.doc.sliceString(pos, pos + 1)
        if (!ch || /[\s{}[\](),;:]/.test(ch)) return null
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
          key: 'Mod-s',
          run(currentView) {
            if (currentView.state.readOnly) return false
            onsave?.(currentView.state.doc.toString())
            return true
          },
        },
        {
          key: 'F12',
          run(currentView) {
            if (!ongotodefinition) return false
            void ongotodefinition(currentView.state.selection.main.head)
            return true
          },
        },
      ]),
      EditorView.domEventHandlers({
        mousedown(event, currentView) {
          if (ongotodefinition && (event.metaKey || event.ctrlKey) && event.button === 0) {
            const pos = currentView.posAtCoords({ x: event.clientX, y: event.clientY })
            if (pos === null) return false
            event.preventDefault()
            void ongotodefinition(pos)
            return true
          }
          if (event.button === 0 && clickToInsert && !readonly) {
            const controller = getDocumentWhichKeyController(currentView.dom.ownerDocument)
            if (controller && controller.getState().mode === 'normal') {
              controller.setMode('insert')
            }
          }
          return false
        },
        beforeinput(event, currentView) {
          const modalMode = currentView.dom.dataset.modalMode
          if (modalMode && modalMode !== 'insert') {
            event.preventDefault()
            return true
          }
          return false
        },
      }),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          onchange?.(update.state.doc.toString())
        }
      }),
    ]
  }

  function applyLanguageSupport(targetName: string | undefined) {
    const aView = mergeInstance?.a
    const bView = mergeInstance?.b ?? unifiedInstance
    if (!bView) return

    if (targetName && isMarkdownTarget(targetName)) {
      const ext = markdown({ codeLanguages: languages })
      aView?.dispatch({ effects: baseLanguageCompartment.reconfigure(ext) })
      bView.dispatch({ effects: headLanguageCompartment.reconfigure(ext) })
      return
    }

    const description = targetName ? LanguageDescription.matchFilename(languages, targetName) : null
    if (!description) {
      aView?.dispatch({ effects: baseLanguageCompartment.reconfigure([]) })
      bView.dispatch({ effects: headLanguageCompartment.reconfigure([]) })
      return
    }

    void description.load().then((support) => {
      if (mergeInstance?.a === aView) {
        aView?.dispatch({ effects: baseLanguageCompartment.reconfigure(support) })
      }
      if ((mergeInstance?.b ?? unifiedInstance) === bView) {
        bView.dispatch({ effects: headLanguageCompartment.reconfigure(support) })
      }
    })
  }

  function destroyViews() {
    scrollCleanup?.()
    scrollCleanup = undefined
    mergeInstance?.destroy()
    mergeInstance = undefined
    unifiedInstance?.destroy()
    unifiedInstance = undefined
    mountedMode = undefined
    mountedFullFile = undefined
  }

  function mountEditor(
    targetMode: DiffEditorMode,
    baseDoc: string,
    headDoc: string,
    fullFile: boolean,
    isReadOnly: boolean,
  ) {
    if (!host) return
    const previousHead = mergeInstance?.b ?? unifiedInstance
    const preservedLine = previousHead
      ? previousHead.state.doc.lineAt(previousHead.state.selection.main.head).number
      : 1
    const preservedScrollTop = previousHead?.scrollDOM.scrollTop ?? 0

    destroyViews()
    host.replaceChildren()

    const collapseUnchanged = buildCollapseConfig(fullFile)

    if (targetMode === 'split') {
      const instance = new MergeView({
        parent: host,
        orientation: 'a-b',
        highlightChanges: true,
        gutter: true,
        collapseUnchanged,
        a: {
          doc: baseDoc,
          extensions: buildBaseExtensions(),
        },
        b: {
          doc: headDoc,
          extensions: buildHeadExtensions(isReadOnly),
        },
      })
      mergeInstance = instance
      scrollCleanup = bindSplitScrollSync(instance.a, instance.b)
    } else {
      const state = EditorState.create({
        doc: headDoc,
        extensions: [
          ...buildHeadExtensions(isReadOnly),
          unifiedMergeView({
            original: baseDoc,
            highlightChanges: true,
            gutter: true,
            mergeControls: false,
            syntaxHighlightDeletions: true,
            collapseUnchanged,
          }),
        ],
      })
      unifiedInstance = new EditorView({ state, parent: host })
    }

    mountedMode = targetMode
    mountedFullFile = fullFile
    applyLanguageSupport(filename ?? label)

    const nextHead = mergeInstance?.b ?? unifiedInstance
    if (nextHead) {
      const lineCount = nextHead.state.doc.lines
      const targetLine = Math.max(1, Math.min(lineCount, preservedLine))
      const pos = nextHead.state.doc.line(targetLine).from
      nextHead.dispatch({ selection: { anchor: pos } })
      nextHead.scrollDOM.scrollTop = preservedScrollTop
      if (mergeInstance) {
        mergeInstance.a.scrollDOM.scrollTop = preservedScrollTop
      }
      const controller = getDocumentWhichKeyController(host.ownerDocument)
      if (controller) {
        const wkState = controller.getState()
        modalTarget.setModalState({ mode: wkState.mode, preset: wkState.preset })
      } else {
        modalTarget.setModalState({ mode: 'normal', preset: 'helix' })
      }
    }
    onViewReady?.(nextHead ?? null)
  }

  // Lifecycle -----------------------------------------------------------------
  onMount(() => {
    if (!host) return
    const unregisterModal = registerModalEditor(host, modalTarget)
    mountEditor(mode, baseValue, headValue, showFullFile, readonly)
    return () => {
      onViewReady?.(null)
      unregisterModal()
      destroyViews()
    }
  })

  $effect(() => {
    const currentMode = mode
    const currentBase = baseValue
    const currentHead = headValue
    const currentFullFile = showFullFile
    const currentReadOnly = readonly
    if (!host) return

    const activeHead = mergeInstance?.b ?? unifiedInstance
    const activeBaseDoc = mergeInstance ? mergeInstance.a.state.doc.toString() : null
    const activeHeadDoc = activeHead ? activeHead.state.doc.toString() : null

    if (
      currentMode !== mountedMode ||
      currentFullFile !== mountedFullFile ||
      (currentMode === 'split' && activeBaseDoc !== currentBase) ||
      activeHeadDoc !== currentHead
    ) {
      mountEditor(currentMode, currentBase, currentHead, currentFullFile, currentReadOnly)
    }
  })

  $effect(() => {
    const nextReadOnly = readonly
    const bView = mergeInstance?.b ?? unifiedInstance
    if (!bView) return
    bView.dispatch({
      effects: headReadOnlyCompartment.reconfigure([
        EditorState.readOnly.of(nextReadOnly),
        EditorView.editable.of(!nextReadOnly),
      ]),
    })
  })

  $effect(() => {
    const nextWrap = lineWrapping
    const aView = mergeInstance?.a
    const bView = mergeInstance?.b ?? unifiedInstance
    const ext = toLineWrappingExtension(nextWrap)
    aView?.dispatch({ effects: baseWrapCompartment.reconfigure(ext) })
    bView?.dispatch({ effects: headWrapCompartment.reconfigure(ext) })
  })

  $effect(() => {
    const targetName = filename ?? label
    applyLanguageSupport(targetName)
  })

  $effect(() => {
    const bView = mergeInstance?.b ?? unifiedInstance
    const nextDiagnostics = diagnostics
    if (!bView) return
    const mapped = toCodeMirrorDiagnostics(bView.state.doc.length, nextDiagnostics)
    bView.dispatch(setDiagnostics(bView.state, mapped))
  })

  // Methods -------------------------------------------------------------------
  /**
   * Toggle between `'split'` (`MergeView`) and `'unified'` (`unifiedMergeView`) modes.
   */
  export function toggleMode(): DiffEditorMode {
    const next: DiffEditorMode = mode === 'split' ? 'unified' : 'split'
    mode = next
    onmodetoggle?.(next)
    return next
  }

  /**
   * Explicitly set the diff editor layout mode (`'split'` or `'unified'`).
   */
  export function setMode(nextMode: DiffEditorMode): void {
    if (mode === nextMode) return
    mode = nextMode
    onmodetoggle?.(nextMode)
  }

  /**
   * Toggle between showing only diff hunks (`showFullFile = false`) and the full uncollapsed file (`showFullFile = true`).
   */
  export function toggleFullFile(): boolean {
    const next = !showFullFile
    showFullFile = next
    onfullfiletoggle?.(next)
    return next
  }

  /**
   * Explicitly set whether unchanged lines are expanded (`true`) or collapsed around diff hunks (`false`).
   */
  export function setShowFullFile(next: boolean): void {
    if (showFullFile === next) return
    showFullFile = next
    onfullfiletoggle?.(next)
  }

  /**
   * Move the cursor to the next diff hunk in the active editor.
   */
  export function goToNextHunk(): boolean {
    const activeView = mergeInstance?.b ?? unifiedInstance
    return stepNextHunk(activeView)
  }

  /**
   * Move the cursor to the previous diff hunk in the active editor.
   */
  export function goToPrevHunk(): boolean {
    const activeView = mergeInstance?.b ?? unifiedInstance
    return stepPrevHunk(activeView)
  }

  /**
   * Focus the head (`b`) diff editor pane.
   */
  export function focus(): void {
    const activeView = mergeInstance?.b ?? unifiedInstance
    activeView?.focus()
  }
</script>

<div
  bind:this={host}
  data-modal-ignore-pointer={!clickToInsert || readonly ? 'true' : undefined}
  class={rootClasses}
  data-slot="diff-editor"
  data-mode={mode}
  data-full-file={showFullFile ? 'true' : 'false'}
></div>
