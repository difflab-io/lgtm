<script lang="ts">
  import { Box, FolderTree } from 'lucide-svelte'
  import type { SidebarWidget } from '../clients/workspaceClient'
  import type { WorkspaceService } from '../services/workspaceService.svelte'

  // Props ---------------------------------------------------------------------
  interface DockViewSwitcherProps {
    workspace: WorkspaceService
  }
  let { workspace }: DockViewSwitcherProps = $props()
</script>

{#snippet dockModeButton(mode: SidebarWidget, label: string)}
  {@const active = mode === 'projects' ? workspace.projectsOpen : workspace.explorerOpen}
  <button
    type="button"
    aria-label="{label} view"
    title={label}
    aria-pressed={active}
    class={[
      'inline-flex size-6 items-center justify-center rounded-none transition-colors outline-none focus-visible:ring-1 focus-visible:ring-ring',
      active
        ? 'bg-accent text-accent-foreground'
        : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground',
    ].join(' ')}
    onclick={() => {
      if (mode === 'projects') workspace.toggleProjectsPane()
      else workspace.toggleExplorerPane()
    }}
  >
    {#if mode === 'projects'}
      <Box size={13} strokeWidth={1.5} />
    {:else}
      <FolderTree size={13} strokeWidth={1.5} />
    {/if}
  </button>
{/snippet}

<div class="flex items-center gap-1" role="group" aria-label="Sidebar view">
  {@render dockModeButton('projects', 'Projects')}
  {@render dockModeButton('explorer', 'Explorer')}
</div>
