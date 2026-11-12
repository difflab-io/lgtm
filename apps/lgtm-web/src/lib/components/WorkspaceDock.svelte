<script lang="ts">
  import {
    Button,
    Notice,
    Row,
    Sidebar,
    SidebarContent,
    SidebarFooter,
  } from '@cloudvoyant/helical-svelte'
  import { FilePlus2, FolderPlus, GitBranch } from 'lucide-svelte'
  import type { WorkspaceClient } from '../clients/workspaceClient'
  import type { WorkspaceService } from '../services/workspaceService.svelte'
  import DockViewSwitcher from './DockViewSwitcher.svelte'
  import ProjectsPane from './ProjectsPane.svelte'
  import SidebarPaneHeader from './SidebarPaneHeader.svelte'
  import WorkspaceExplorer from './WorkspaceExplorer.svelte'

  // Props ---------------------------------------------------------------------
  interface WorkspaceDockProps {
    workspace: WorkspaceService
    client: WorkspaceClient
  }
  let { workspace, client }: WorkspaceDockProps = $props()
</script>

<Sidebar
  id="workspace-sidebar"
  collapsible="offcanvas"
  class="workspace-sidebar top-9 h-[calc(100dvh-36px)] overflow-hidden bg-sidebar outline-none"
  aria-label="Workspace"
>
  <Row class="min-h-0 flex-1 items-stretch gap-0 overflow-hidden">
    <Sidebar
      id="projects-dock-pane"
      data-testid="projects-dock-pane"
      paneId="projects"
      paneTitle="Projects"
      paneDisabled={!workspace.projectsOpen}
      collapsible="none"
      hidden={!workspace.projectsOpen}
      inert={!workspace.projectsOpen ? true : undefined}
      class={[
        'h-full min-h-0 w-45 shrink-0 overflow-hidden bg-sidebar outline-none',
        workspace.projectsOpen ? '' : 'hidden',
        workspace.projectsOpen && workspace.explorerOpen ? 'border-r border-border' : '',
      ].join(' ')}
      aria-label="Projects"
    >
      <SidebarPaneHeader title="Projects" class="px-3">
        {#snippet actions()}
          <Button
            variant="text"
            size="sm"
            class="size-6 p-1 text-[10px] font-medium"
            aria-label="Add project"
            title="Add project"
            disabled={workspace.blocked}
            onclick={() => void workspace.selectWorkspace('__open_folder__')}
            ><FolderPlus size={14} strokeWidth={1.5} /></Button
          >
          <Button
            variant="text"
            size="sm"
            class="size-6 p-1 text-[10px] font-medium"
            aria-label="New worktree"
            title="New worktree"
            disabled={!workspace.activeProject || workspace.blocked}
            onclick={() => workspace.openNewWorktreeDialog()}
            ><GitBranch size={14} strokeWidth={1.5} /></Button
          >
        {/snippet}
      </SidebarPaneHeader>
      <SidebarContent class="min-h-0 flex-1 overflow-y-auto">
        {#if workspace.creationError && !workspace.explorerOpen}
          <Notice variant="error" class="mx-3.5 my-2 text-xs">{workspace.creationError}</Notice>
        {/if}
        <ProjectsPane {workspace} />
      </SidebarContent>
    </Sidebar>

    <Sidebar
      id="explorer-dock-pane"
      data-testid="explorer-dock-pane"
      paneId="workspace"
      paneTitle="Workspace"
      paneDisabled={!workspace.explorerOpen}
      collapsible="none"
      hidden={!workspace.explorerOpen}
      inert={!workspace.explorerOpen ? true : undefined}
      class={[
        'h-full min-h-0 w-59 shrink-0 overflow-hidden bg-sidebar outline-none',
        workspace.explorerOpen ? '' : 'hidden',
      ].join(' ')}
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
          {#if client.isDesktop()}<Button variant="text" size="sm" onclick={() => workspace.open()}
              >Retry</Button
            >{/if}
        {:else if !workspace.entries.length && !workspace.pendingEntry}
          <Notice variant="none" class="mx-3.5 my-2 text-xs">
            Your workspace is empty. Create your first file with New file.
          </Notice>
        {/if}
        <div
          hidden={Boolean(workspace.error) ||
            (!workspace.entries.length && !workspace.pendingEntry)}
          class={Boolean(workspace.error) || (!workspace.entries.length && !workspace.pendingEntry)
            ? 'hidden'
            : ''}
        >
          <WorkspaceExplorer {workspace} />
        </div>
      </SidebarContent>
    </Sidebar>
  </Row>
  <SidebarFooter
    class="shrink-0 flex-row items-center justify-start gap-1 border-t border-border bg-sidebar px-2.5 py-1.5"
  >
    <DockViewSwitcher {workspace} />
  </SidebarFooter>
</Sidebar>
