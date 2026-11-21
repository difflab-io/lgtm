<script lang="ts">
  import { tick } from 'svelte'
  import { Button, Notice, Pane, Row } from '@cloudvoyant/helical-svelte'
  import { CodeEditor, DiffEditor, Tabs } from 'lgtm-ui'
  import { FileText, GitCompare } from 'lucide-svelte'
  import { useService } from '../services/context'

  // State ---------------------------------------------------------------------
  const workspace = useService('workspace')
  const lsp = useService('lsp')
  const settings = useService('settings')
  const hotkeys = useService('hotkeys')
  let editorRef = $state<{ focus: () => void }>()
  let diffEditorRef = $state<{
    focus: () => void
    goToNextHunk: () => boolean
    goToPrevHunk: () => boolean
  }>()

  // Derived -------------------------------------------------------------------
  const editorDiagnostics = $derived(
    lsp.getDiagnosticsForPath(workspace.selected?.path).map((diag) => ({
      from: diag.from,
      to: diag.to,
      severity: diag.severity,
      message: diag.message,
      ...(diag.source ? { source: diag.source } : {}),
      ...(diag.code ? { code: diag.code } : {}),
    })),
  )

  const activeDiffEntry = $derived(
    workspace.diff.activeFilePath
      ? workspace.diff.getFileEntry(workspace.diff.activeFilePath)
      : undefined,
  )

  // Lifecycle -----------------------------------------------------------------
  $effect(() => {
    const request = workspace.focusRequest
    if (request) {
      void tick().then(() => {
        if (workspace.sidebarMode === 'diffs') {
          diffEditorRef?.focus()
        } else {
          editorRef?.focus()
        }
      })
    }
  })
</script>

{#snippet segmentedToggleButton(active: boolean, label: string, onclick: () => void)}
  <button
    type="button"
    class={[
      'px-2 py-0.5 text-[10px] font-medium',
      active ? 'bg-accent text-accent-foreground' : 'text-muted-foreground hover:text-foreground',
    ]}
    aria-pressed={active}
    {onclick}
  >
    {label}
  </button>
{/snippet}

<div
  class="document-pane relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background text-foreground"
>
  <Pane
    role="main"
    paneId="editor"
    paneTitle="Editor"
    paneOrder={3}
    tabindex={-1}
    class="relative flex min-h-0 min-w-0 flex-1 flex-col rounded-none border-0 bg-background text-foreground shadow-none ring-0 outline-none data-[pane-active=true]:ring-0 {settings.paneOutline
      ? 'data-[pane-active=true]:after:pointer-events-none data-[pane-active=true]:after:absolute data-[pane-active=true]:after:inset-0 data-[pane-active=true]:after:z-20 data-[pane-active=true]:after:ring-1 data-[pane-active=true]:after:ring-ring/40 data-[pane-active=true]:after:ring-inset'
      : ''}"
    aria-label="File viewer"
  >
    {#if workspace.sidebarMode === 'diffs'}
      <div
        class="diff-toolbar flex h-9 shrink-0 items-center justify-between gap-3 border-b border-border bg-sidebar px-3.5 text-xs"
        role="toolbar"
        aria-label="Diff viewer controls"
      >
        <Row class="min-w-0 items-center gap-2">
          <GitCompare size={13} strokeWidth={1.5} class="shrink-0 text-muted-foreground" />
          <span
            class="truncate font-mono text-[11px] font-medium"
            data-testid="diff-active-filepath"
          >
            {workspace.diff.activeFilePath ?? 'No file selected'}
          </span>
          {#if activeDiffEntry}
            <span
              class="flex shrink-0 items-center gap-1 font-mono text-[10px]"
              data-testid="diff-toolbar-stats"
            >
              <span class="text-emerald-600 dark:text-emerald-400"
                >+{activeDiffEntry.additions}</span
              >
              <span class="text-rose-600 dark:text-rose-400">-{activeDiffEntry.deletions}</span>
            </span>
          {/if}
        </Row>
        <Row class="shrink-0 items-center gap-1">
          <div
            class="flex items-center border border-border bg-background/60 p-0.5"
            role="group"
            aria-label="Review mode"
          >
            {@render segmentedToggleButton(workspace.review.mode === 'local', 'Local', () => {
              void workspace.review.setMode('local')
            })}
            {@render segmentedToggleButton(
              workspace.review.mode === 'forge_backed',
              'Branch',
              () => {
                void workspace.review.setMode('forge_backed')
              },
            )}
          </div>

          <div
            class="flex items-center border border-border bg-background/60 p-0.5"
            role="group"
            aria-label="Diff layout mode"
          >
            {@render segmentedToggleButton(workspace.diff.viewMode === 'split', 'Split', () => {
              workspace.diff.setViewMode('split')
            })}
            {@render segmentedToggleButton(workspace.diff.viewMode === 'unified', 'Unified', () => {
              workspace.diff.setViewMode('unified')
            })}
          </div>

          <button
            type="button"
            class={[
              'border border-border px-2 py-0.5 text-[10px] font-medium',
              workspace.diff.showFullFile
                ? 'bg-accent text-accent-foreground'
                : 'bg-background/60 text-muted-foreground hover:text-foreground',
            ]}
            aria-pressed={workspace.diff.showFullFile}
            onclick={() => workspace.diff.toggleFullFile()}
          >
            Full File
          </button>

          {#if workspace.diff.activeDiff && !workspace.diff.activeDiff.readonly}
            <button
              type="button"
              class="border border-border bg-background/60 px-2 py-0.5 text-[10px] font-medium text-foreground hover:bg-accent"
              onclick={() => {
                if (!workspace.diff.activeDiff) return
                void workspace.saveDiffFile(
                  workspace.diff.activeDiff.path,
                  workspace.diff.draftHeadContent ?? workspace.diff.activeDiff.headContent,
                )
              }}
            >
              Save
            </button>
          {/if}
        </Row>
      </div>

      {#if workspace.diff.error}
        <div
          class="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center text-muted-foreground"
        >
          <p class="max-w-115 text-xs leading-relaxed text-warn" role="alert">
            {workspace.diff.error}
          </p>
        </div>
      {:else if workspace.diff.activeDiff}
        <div class="flex min-h-0 flex-1 flex-col overflow-auto">
          {#key `${workspace.diff.activeDiff.path}:${workspace.diff.diffVersion}`}
            <DiffEditor
              bind:this={diffEditorRef}
              baseValue={workspace.diff.activeDiff.baseContent}
              headValue={workspace.diff.draftHeadContent ?? workspace.diff.activeDiff.headContent}
              filename={workspace.diff.activeDiff.path}
              label={`Diff editor for ${workspace.diff.activeDiff.path}`}
              mode={workspace.diff.viewMode}
              showFullFile={workspace.diff.showFullFile}
              readonly={workspace.diff.activeDiff.readonly}
              lineWrapping={settings.lineWrapping}
              clickToInsert={settings.clickToInsert}
              onchange={(content) => workspace.diff.updateDraft(content)}
              onsave={(content) => {
                if (!workspace.diff.activeDiff) return
                void workspace.saveDiffFile(workspace.diff.activeDiff.path, content)
              }}
              onmodetoggle={(nextMode) => workspace.diff.setViewMode(nextMode)}
              onfullfiletoggle={(nextShow) => workspace.diff.setShowFullFile(nextShow)}
              onViewReady={(v) => hotkeys?.setEditorView(v)}
            />
          {/key}
        </div>
      {:else}
        <div
          class="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center text-muted-foreground"
        >
          <GitCompare size={32} strokeWidth={1.5} />
          <h1 class="text-[17px] font-medium text-foreground">No changes selected.</h1>
          <p class="max-w-115 text-xs leading-relaxed">
            Select a changed file or check one or more commits in the sidebar checklist.
          </p>
        </div>
      {/if}
    {:else}
      {#if workspace.tabItems.length > 0}
        <Tabs
          items={workspace.tabItems}
          value={workspace.activeTabId}
          ariaLabel="Open editors"
          onchange={(tabId) => workspace.activateTab(tabId)}
          onclose={(tabId) => workspace.closeTab(tabId)}
        />
      {/if}
      {#if workspace.fileLoading}
        <div
          class="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center text-muted-foreground"
          role="status"
        >
          Opening file…
        </div>
      {:else if workspace.openingError}
        <div
          class="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center text-muted-foreground"
        >
          <Notice variant="error" class="max-w-115 text-xs">
            {workspace.openingError}
          </Notice>
          {#if workspace.selected}<Button
              variant="outline"
              size="sm"
              onclick={() => workspace.selected && workspace.openFile(workspace.selected)}
              >Retry opening</Button
            >{/if}
        </div>
      {:else if workspace.fileContent && (workspace.selected || workspace.activeTab)}
        <div class="flex min-h-0 flex-1 flex-col overflow-hidden">
          {#key workspace.fileVersion}
            <CodeEditor
              bind:this={editorRef}
              value={workspace.fileContent.content}
              filename={workspace.activeEditorFilename}
              label={workspace.selectedTitle}
              readonly={workspace.activeTab?.readonly ?? false}
              lineWrapping={settings.lineWrapping}
              clickToInsert={settings.clickToInsert}
              diagnostics={editorDiagnostics}
              selectionOffset={workspace.selectionOffset}
              onchange={(content) => workspace.updateContent(content)}
              onhover={(offset) => workspace.requestHover(offset)}
              ongotodefinition={(offset) => workspace.gotoDefinition(offset)}
              oncompletion={(offset) => workspace.requestCompletions(offset)}
              onsignaturehelp={(offset) => workspace.requestSignatureHelp(offset)}
              onViewReady={(v) => hotkeys?.setEditorView(v)}
            />
          {/key}
        </div>
      {:else}
        <div
          class="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center text-muted-foreground"
        >
          <FileText size={32} strokeWidth={1.5} />
          <h1 class="text-[17px] font-medium text-foreground">Your workspace, ready for review.</h1>
          <p class="max-w-115 text-xs leading-relaxed">
            Select a file from the sidebar, or create a new one.
          </p>
        </div>
      {/if}
    {/if}
  </Pane>
</div>
