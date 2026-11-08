<script lang="ts">
  import { tick } from 'svelte'
  import { Button, Row } from '@cloudvoyant/helical-svelte'
  import { CodeEditor, DiffEditor, Tabs } from 'lgtm-ui'
  import { ChevronRight, FileText, GitCompare } from 'lucide-svelte'
  import type { WorkspaceService } from '../services/workspaceService.svelte'
  import LspStatusMenu from './LspStatusMenu.svelte'

  // Props ---------------------------------------------------------------------
  interface ContentPaneProps {
    workspace: WorkspaceService
  }
  let { workspace }: ContentPaneProps = $props()

  // State ---------------------------------------------------------------------
  let editorRef = $state<{ focus: () => void }>()
  let diffEditorRef = $state<{
    focus: () => void
    goToNextHunk: () => boolean
    goToPrevHunk: () => boolean
  }>()

  // Derived -------------------------------------------------------------------
  const editorDiagnostics = $derived(
    workspace.activeDiagnostics.map((diag) => ({
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

  // Public --------------------------------------------------------------------
  export function goToNextHunk(): boolean {
    return diffEditorRef?.goToNextHunk() ?? false
  }

  export function goToPrevHunk(): boolean {
    return diffEditorRef?.goToPrevHunk() ?? false
  }
</script>

<main
  class="document-pane flex min-h-0 min-w-0 flex-1 flex-col bg-background text-foreground"
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
        <span class="truncate font-mono text-[11px] font-medium" data-testid="diff-active-filepath">
          {workspace.diff.activeFilePath ?? 'No file selected'}
        </span>
        {#if activeDiffEntry}
          <span
            class="flex shrink-0 items-center gap-1 font-mono text-[10px]"
            data-testid="diff-toolbar-stats"
          >
            <span class="text-emerald-600 dark:text-emerald-400">+{activeDiffEntry.additions}</span>
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
          <button
            type="button"
            class={[
              'px-2 py-0.5 text-[10px] font-medium',
              workspace.review.mode === 'local'
                ? 'bg-accent text-accent-foreground'
                : 'text-muted-foreground hover:text-foreground',
            ]}
            aria-pressed={workspace.review.mode === 'local'}
            onclick={() => void workspace.review.setMode('local')}
          >
            Local
          </button>
          <button
            type="button"
            class={[
              'px-2 py-0.5 text-[10px] font-medium',
              workspace.review.mode === 'forge_backed'
                ? 'bg-accent text-accent-foreground'
                : 'text-muted-foreground hover:text-foreground',
            ]}
            aria-pressed={workspace.review.mode === 'forge_backed'}
            onclick={() => void workspace.review.setMode('forge_backed')}
          >
            Branch
          </button>
        </div>

        <div
          class="flex items-center border border-border bg-background/60 p-0.5"
          role="group"
          aria-label="Diff layout mode"
        >
          <button
            type="button"
            class={[
              'px-2 py-0.5 text-[10px] font-medium',
              workspace.diff.viewMode === 'split'
                ? 'bg-accent text-accent-foreground'
                : 'text-muted-foreground hover:text-foreground',
            ]}
            aria-pressed={workspace.diff.viewMode === 'split'}
            onclick={() => workspace.diff.setViewMode('split')}
          >
            Split
          </button>
          <button
            type="button"
            class={[
              'px-2 py-0.5 text-[10px] font-medium',
              workspace.diff.viewMode === 'unified'
                ? 'bg-accent text-accent-foreground'
                : 'text-muted-foreground hover:text-foreground',
            ]}
            aria-pressed={workspace.diff.viewMode === 'unified'}
            onclick={() => workspace.diff.setViewMode('unified')}
          >
            Unified
          </button>
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
            headValue={workspace.diff.activeDiff.headContent}
            filename={workspace.diff.activeDiff.path}
            label={`Diff editor for ${workspace.diff.activeDiff.path}`}
            mode={workspace.diff.viewMode}
            showFullFile={workspace.diff.showFullFile}
            readonly={workspace.diff.activeDiff.readonly}
            lineWrapping={workspace.settings.lineWrapping}
            onchange={(content) => workspace.diff.updateDraft(content)}
            onsave={(content) => {
              if (!workspace.diff.activeDiff) return
              void workspace.saveDiffFile(workspace.diff.activeDiff.path, content)
            }}
            onmodetoggle={(nextMode) => workspace.diff.setViewMode(nextMode)}
            onfullfiletoggle={(nextShow) => workspace.diff.setShowFullFile(nextShow)}
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
        <p class="max-w-115 text-xs leading-relaxed text-warn" role="alert">
          {workspace.openingError}
        </p>
        {#if workspace.selected}<Button
            variant="outline"
            size="sm"
            onclick={() => workspace.selected && workspace.openFile(workspace.selected)}
            >Retry opening</Button
          >{/if}
      </div>
    {:else if workspace.fileContent && (workspace.selected || workspace.activeTab)}
      <div class="flex min-h-0 flex-1 flex-col overflow-auto">
        {#key workspace.fileVersion}
          <CodeEditor
            bind:this={editorRef}
            value={workspace.fileContent.content}
            filename={workspace.activeEditorFilename}
            label={workspace.selectedTitle}
            readonly={workspace.activeTab?.readonly ?? false}
            lineWrapping={workspace.settings.lineWrapping}
            diagnostics={editorDiagnostics}
            selectionOffset={workspace.selectionOffset}
            onchange={(content) => workspace.updateContent(content)}
            onhover={(offset) => workspace.requestHover(offset)}
            ongotodefinition={(offset) => workspace.gotoDefinition(offset)}
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
  <footer
    class="relative flex items-center justify-between gap-4 border-t border-border bg-sidebar px-4 py-2 text-[10px] text-muted-foreground"
    role="status"
    aria-live="polite"
  >
    <div class="flex min-w-0 items-center gap-2.5">
      <LspStatusMenu {workspace} />
      <nav
        aria-label="Breadcrumb"
        class="document-path flex min-w-0 items-center gap-1 truncate"
        title={workspace.selectedPath}
      >
        {#if workspace.breadcrumbs.length > 0}
          {#each workspace.breadcrumbs as segment, idx (idx)}
            {#if idx > 0}
              <ChevronRight size={11} strokeWidth={1.5} class="shrink-0 opacity-60" />
            {/if}
            <span
              class={idx === workspace.breadcrumbs.length - 1
                ? 'truncate text-foreground'
                : 'truncate'}
            >
              {segment}
            </span>
          {/each}
        {:else}
          <span class="truncate">No file selected</span>
        {/if}
      </nav>
    </div>
    <span class={workspace.openingError ? 'shrink-0 text-warn' : 'shrink-0'}
      >{workspace.status}</span
    >
  </footer>
</main>
