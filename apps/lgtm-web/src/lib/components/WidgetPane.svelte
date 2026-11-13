<script lang="ts">
  import { Button, Notice, Sidebar, SidebarContent } from '@cloudvoyant/helical-svelte'
  import { FilePlus2, FolderPlus } from 'lucide-svelte'
  import { useService } from '../services/context'
  import FileExplorer from './FileExplorer.svelte'
  import SidebarPaneHeader from './SidebarPaneHeader.svelte'

  // State ---------------------------------------------------------------------
  const workspace = useService('workspace')
  const session = useService('session')
</script>

<Sidebar
  id="explorer-dock-pane"
  data-testid="explorer-dock-pane"
  paneId="workspace"
  paneTitle="Workspace"
  paneDisabled={!session.explorerOpen}
  collapsible="none"
  aria-hidden={!session.explorerOpen}
  inert={!session.explorerOpen ? true : undefined}
  style="width: {session.widgetWidth}px"
  class="h-full min-h-0 shrink-0 overflow-hidden bg-sidebar outline-none"
  aria-label="Workspace Explorer"
>
  <SidebarPaneHeader title="Workspace">
    {#snippet actions()}
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
    {/snippet}
  </SidebarPaneHeader>
  <SidebarContent class="min-h-0 flex-1 overflow-y-auto">
    {#if workspace.creationError}
      <Notice variant="error" class="mx-3.5 my-2 text-xs">{workspace.creationError}</Notice>
    {/if}
    {#if workspace.loading && !workspace.entries.length}
      <Notice variant="info" class="mx-3.5 my-2 text-xs">Opening workspace…</Notice>
    {:else if workspace.error}
      <Notice variant="error" class="mx-3.5 my-2 text-xs">{workspace.error}</Notice>
      {#if workspace.client.isDesktop()}<Button
          variant="text"
          size="sm"
          onclick={() => workspace.open()}>Retry</Button
        >{/if}
    {:else if !workspace.entries.length && !workspace.pendingEntry}
      <Notice variant="none" class="mx-3.5 my-2 text-xs">
        Your workspace is empty. Create your first file with New file.
      </Notice>
    {/if}
    <div
      hidden={Boolean(workspace.error) || (!workspace.entries.length && !workspace.pendingEntry)}
      class={Boolean(workspace.error) || (!workspace.entries.length && !workspace.pendingEntry)
        ? 'hidden'
        : ''}
    >
      <FileExplorer />
    </div>
  </SidebarContent>
</Sidebar>
