<script lang="ts">
  import { tick } from 'svelte'
  import { Button, Pane, WhichKeyMode } from '@cloudvoyant/helical-svelte'
  import { CodeEditor, Tabs } from 'lgtm-ui'
  import { ChevronRight, FileText, FolderTree, GitBranch } from 'lucide-svelte'
  import type { SidebarWidget } from '../clients/workspaceClient'
  import type { WorkspaceService } from '../services/workspaceService.svelte'
  import LspStatusMenu from './LspStatusMenu.svelte'

  // Props ---------------------------------------------------------------------
  interface ContentPaneProps {
    workspace: WorkspaceService
    hotkeys: HotkeyService
  }
  let { workspace, hotkeys }: ContentPaneProps = $props()

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

{#snippet sidebarModeButton(mode: SidebarWidget, label: string)}
  {@const active = workspace.sidebarWidget === mode}
  <button
    type="button"
    aria-label="{label} view"
    aria-pressed={active}
    class={[
      'inline-flex h-5 items-center gap-1 rounded-none px-1.5 text-[10px] font-medium transition-colors outline-none focus-visible:ring-1 focus-visible:ring-ring',
      active
        ? 'bg-accent text-accent-foreground'
        : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground',
    ].join(' ')}
    onclick={() => workspace.setSidebarWidget(mode)}
  >
    {#if mode === 'explorer'}
      <FolderTree size={11} strokeWidth={1.5} />
    {:else}
      <GitBranch size={11} strokeWidth={1.5} />
    {/if}
    <span>{label}</span>
  </button>
{/snippet}

<main
  data-pane-id="editor"
  data-pane-title="Editor"
  class="document-pane flex min-h-0 min-w-0 flex-1 flex-col bg-background text-foreground outline-none"
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
          clickToInsert={workspace.settings.clickToInsert}
          diagnostics={editorDiagnostics}
          selectionOffset={workspace.selectionOffset}
          onchange={(content) => workspace.updateContent(content)}
          onhover={(offset) => workspace.requestHover(offset)}
          ongotodefinition={(offset) => workspace.gotoDefinition(offset)}
          oncompletion={(offset) => workspace.requestCompletions(offset)}
          onsignaturehelp={(offset) => workspace.requestSignatureHelp(offset)}
          onViewReady={(v) => hotkeys.setEditorView(v)}
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
  <footer
    class="relative flex items-center justify-between gap-4 border-t border-border bg-sidebar px-4 py-2 text-[10px] text-muted-foreground"
    role="status"
    aria-live="polite"
  >
    <div class="flex min-w-0 items-center gap-2.5">
      <div
        class="flex shrink-0 items-center gap-0.5 border-r border-border pr-2"
        role="group"
        aria-label="Sidebar view"
      >
        {@render sidebarModeButton('explorer', 'Explorer')}
        {@render sidebarModeButton('projects', 'Projects')}
      </div>
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
    <div class="flex shrink-0 items-center gap-2.5">
      <WhichKeyMode showPreset aria-label="Editor mode" />
      <span class={workspace.openingError ? 'shrink-0 text-warn' : 'shrink-0'}
        >{workspace.status}</span
      >
    </div>
  </footer>
</Pane>
