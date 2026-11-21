<script lang="ts">
  import { Button, Notice, Sidebar, SidebarContent } from '@cloudvoyant/helical-svelte'
  import { FilePlus2, FolderPlus, GitCompare } from 'lucide-svelte'
  import { useService } from '../services/context'
  import CommitSelector from './CommitSelector.svelte'
  import DiffExplorer from './DiffExplorer.svelte'
  import FileExplorer from './FileExplorer.svelte'
  import SidebarPaneHeader from './SidebarPaneHeader.svelte'

  // State ---------------------------------------------------------------------
  const workspace = useService('workspace')
  const settings = useService('settings')
</script>

<Sidebar
  id="explorer-dock-pane"
  data-testid="explorer-dock-pane"
  paneId="workspace"
  paneTitle="Workspace"
  paneDisabled={!workspace.explorerOpen}
  collapsible="none"
  aria-hidden={!workspace.explorerOpen}
  inert={!workspace.explorerOpen ? true : undefined}
  style="width: {workspace.widgetWidth}px"
  class={[
    'relative h-full min-h-0 shrink-0 overflow-hidden bg-sidebar outline-none',
    settings.paneOutline &&
      'data-[pane-active=true]:after:pointer-events-none data-[pane-active=true]:after:absolute data-[pane-active=true]:after:inset-0 data-[pane-active=true]:after:z-20 data-[pane-active=true]:after:ring-1 data-[pane-active=true]:after:ring-ring/40 data-[pane-active=true]:after:ring-inset',
  ]
    .filter(Boolean)
    .join(' ')}
  aria-label="Workspace Explorer"
>
  <SidebarPaneHeader title={workspace.sidebarMode === 'diffs' ? 'Diffs' : 'Workspace'}>
    {#snippet actions()}
      {#if workspace.sidebarMode === 'files'}
        <Button
          variant="text"
          size="sm"
          class="size-6 p-1 text-[10px] font-medium"
          aria-label="New file"
          title="New file"
          disabled={workspace.cannotCreate}
          onclick={() => workspace.startCreate('file')}
          ><FilePlus2 size={14} strokeWidth={1.5} /></Button
        >
        <Button
          variant="text"
          size="sm"
          class="size-6 p-1 text-[10px] font-medium"
          aria-label="New folder"
          title="New folder"
          disabled={workspace.cannotCreate}
          onclick={() => workspace.startCreate('directory')}
          ><FolderPlus size={14} strokeWidth={1.5} /></Button
        >
      {/if}
      <Button
        variant="text"
        size="sm"
        class={workspace.sidebarMode === 'diffs'
          ? 'size-6 bg-accent p-1 text-[10px] font-medium text-accent-foreground'
          : 'size-6 p-1 text-[10px] font-medium'}
        aria-label="Toggle diffs"
        aria-pressed={workspace.sidebarMode === 'diffs'}
        title="Toggle diffs"
        onclick={() => workspace.toggleSidebarMode()}
        ><GitCompare size={14} strokeWidth={1.5} /></Button
      >
    {/snippet}
  </SidebarPaneHeader>
  {#if workspace.sidebarMode === 'diffs'}
    <SidebarContent class="flex min-h-0 flex-1 flex-col overflow-hidden p-0">
      <DiffExplorer diff={workspace.diff} />
      <CommitSelector diff={workspace.diff} />
    </SidebarContent>
  {:else}
    <SidebarContent class="min-h-0 flex-1 overflow-x-hidden overflow-y-auto">
      {#if workspace.error && workspace.client.isDesktop()}
        <div class="mx-3.5 my-2">
          <Button variant="text" size="sm" onclick={() => workspace.open()}>Retry</Button>
        </div>
      {:else if !workspace.loading && !workspace.error && !workspace.entries.length && !workspace.pendingEntry}
        <Notice variant="none" class="mx-3.5 my-2 text-xs">
          Your workspace is empty. Create your first file with New file.
        </Notice>
      {/if}
      <div
        hidden={Boolean(workspace.error) || (!workspace.entries.length && !workspace.pendingEntry)}
        class={Boolean(workspace.error) || (!workspace.entries.length && !workspace.pendingEntry)
          ? 'hidden'
          : 'w-full min-w-0'}
      >
        <FileExplorer />
      </div>
    </SidebarContent>
  {/if}
</Sidebar>
