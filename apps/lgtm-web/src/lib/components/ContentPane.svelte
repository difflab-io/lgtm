<script lang="ts">
  import { tick } from 'svelte'
  import { Button, Notice, Pane } from '@cloudvoyant/helical-svelte'
  import { CodeEditor, Tabs } from 'lgtm-ui'
  import { FileText } from 'lucide-svelte'
  import { useService } from '../services/context'

  // State ---------------------------------------------------------------------
  const workspace = useService('workspace')
  const hotkeys = useService('hotkeys')
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

<Pane
  role="main"
  paneId="editor"
  paneTitle="Editor"
  paneOrder={3}
  tabindex={-1}
  class="document-pane relative flex min-h-0 min-w-0 flex-1 flex-col rounded-none border-0 bg-background text-foreground shadow-none outline-none {workspace
    .settings.paneOutline
    ? 'data-[pane-active=true]:ring-1 data-[pane-active=true]:ring-ring/40 data-[pane-active=true]:ring-inset'
    : 'ring-0 data-[pane-active=true]:ring-0'}"
  aria-label="File viewer"
>
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
          lineWrapping={workspace.settings.lineWrapping}
          clickToInsert={workspace.settings.clickToInsert}
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
</Pane>
