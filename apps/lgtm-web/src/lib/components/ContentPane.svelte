<script lang="ts">
  import { tick } from 'svelte'
  import { Button } from '@cloudvoyant/helical-svelte'
  import { CodeEditor } from 'lgtm-ui'
  import { FileText } from 'lucide-svelte'
  import type { WorkspaceService } from '../services/workspaceService.svelte'

  // Props ---------------------------------------------------------------------
  interface ContentPaneProps {
    workspace: WorkspaceService
  }
  let { workspace }: ContentPaneProps = $props()

  // State ---------------------------------------------------------------------
  let editorRef = $state<{ focus: () => void }>()

  // Lifecycle -----------------------------------------------------------------
  $effect(() => {
    const request = workspace.focusRequest
    if (request) void tick().then(() => editorRef?.focus())
  })
</script>

<main class="document-pane" aria-label="File viewer">
  {#if workspace.selected}
    <div class="document-heading">
      <span class="document-path" title={workspace.selected.path}>{workspace.selectedTitle}</span>
      {#if workspace.selected.name.includes('.')}
        <span class="format-label">{workspace.selected.name.split('.').at(-1)?.toUpperCase()}</span>
      {/if}
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
  {:else if workspace.fileContent && workspace.selected}
    <div class="editor-scroll">
      {#key workspace.fileVersion}
        <CodeEditor
          bind:this={editorRef}
          value={workspace.fileContent.content}
          filename={workspace.selected.name}
          label={workspace.selected.name}
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
    <span title={workspace.selected?.path}>{workspace.selectedTitle}</span>
    <span class={{ error: Boolean(workspace.openingError) }}>{workspace.status}</span>
  </footer>
</main>
