<script lang="ts">
  import { tick } from 'svelte'
  import { Button } from '@cloudvoyant/helical-svelte'
  import { CodeEditor } from 'lgtm-ui'
  import { FileText, X } from 'lucide-svelte'
  import type { WorkspaceService } from '../services/workspaceService.svelte'
  import LspStatusMenu from './LspStatusMenu.svelte'

  // Props ---------------------------------------------------------------------
  interface ContentPaneProps {
    workspace: WorkspaceService
  }
  let { workspace }: ContentPaneProps = $props()

  // State ---------------------------------------------------------------------
  let editorRef = $state<{ focus: () => void }>()

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

  // Lifecycle -----------------------------------------------------------------
  $effect(() => {
    const request = workspace.focusRequest
    if (request) void tick().then(() => editorRef?.focus())
  })
</script>

<main class="document-pane" aria-label="File viewer">
  {#if workspace.tabs.length > 0}
    <div class="document-tabs" role="tablist" aria-label="Open editors">
      {#each workspace.tabs as tab (tab.id)}
        <div
          class="document-tab"
          class:active={workspace.activeTabId === tab.id}
          data-tab-id={tab.id}
        >
          <button
            type="button"
            role="tab"
            class="document-tab-trigger"
            aria-selected={workspace.activeTabId === tab.id}
            onclick={() => workspace.activateTab(tab.id)}
          >
            {tab.title}
          </button>
          <button
            type="button"
            class="document-tab-close"
            aria-label={`Close ${tab.title}`}
            onclick={() => workspace.closeTab(tab.id)}
          >
            <X size={11} strokeWidth={1.75} />
          </button>
        </div>
      {/each}
    </div>
  {/if}
  {#if workspace.selected || workspace.activeTab}
    <div class="document-heading">
      <span class="document-path" title={workspace.selectedPath}>{workspace.selectedTitle}</span>
    </div>
  {/if}
  {#if workspace.fileLoading}
    <div class="empty-pane" role="status">Opening file…</div>
  {:else if workspace.openingError}
    <div class="empty-pane">
      <p class="error" role="alert">{workspace.openingError}</p>
      {#if workspace.selected}<Button
          variant="outline"
          size="sm"
          onclick={() => workspace.selected && workspace.openFile(workspace.selected)}
          >Retry opening</Button
        >{/if}
    </div>
  {:else if workspace.fileContent && (workspace.selected || workspace.activeTab)}
    <div class="editor-scroll">
      {#key workspace.fileVersion}
        <CodeEditor
          bind:this={editorRef}
          value={workspace.fileContent.content}
          filename={workspace.selected?.name ?? 'diagnostics.md'}
          label={workspace.selectedTitle}
          readonly={workspace.activeTab?.readonly ?? false}
          diagnostics={editorDiagnostics}
          selectionOffset={workspace.selectionOffset}
          onchange={(content) => workspace.updateContent(content)}
          onhover={(offset) => workspace.requestHover(offset)}
          ongotodefinition={(offset) => workspace.gotoDefinition(offset)}
        />
      {/key}
    </div>
  {:else}
    <div class="empty-pane">
      <FileText size={32} strokeWidth={1.5} />
      <h1>Your workspace, ready for review.</h1>
      <p>Select a file from the sidebar, or create a new one.</p>
    </div>
  {/if}
  <footer class="document-status" role="status" aria-live="polite">
    <span title={workspace.selectedPath}>{workspace.selectedTitle}</span>
    <div class="document-status-right">
      <LspStatusMenu {workspace} />
      <span class={{ error: Boolean(workspace.openingError) }}>{workspace.status}</span>
    </div>
  </footer>
</main>
