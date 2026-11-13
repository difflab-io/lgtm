<script lang="ts">
  import { Button, Row } from '@cloudvoyant/helical-svelte'
  import { Box, FolderTree } from 'lucide-svelte'
  import type { SidebarWidget } from '../clients/workspaceClient'
  import { useService } from '../services/context'

  // State ---------------------------------------------------------------------
  const session = useService('session')
  const dockOpen = $derived(session.projectsOpen || session.explorerOpen)
</script>

{#snippet dockModeButton(mode: SidebarWidget, label: string)}
  {@const active = mode === 'projects' ? session.projectsOpen : session.explorerOpen}
  <Button
    variant="text"
    size="sm"
    aria-label="{label} view"
    title={label}
    aria-pressed={active}
    class={[
      'size-6 bg-transparent p-1 text-[10px] font-medium hover:bg-transparent',
      active ? 'text-foreground' : 'text-muted-foreground hover:text-foreground',
    ].join(' ')}
    onclick={() => {
      if (mode === 'projects') session.toggleProjectsPane()
      else session.toggleExplorerPane()
    }}
  >
    {#if mode === 'projects'}
      <Box size={14} strokeWidth={1.5} />
    {:else}
      <FolderTree size={14} strokeWidth={1.5} />
    {/if}
  </Button>
{/snippet}

<div
  data-testid="left-dock-toolbar"
  style={dockOpen ? `width: ${session.leftDockWidth}px` : undefined}
  class={[
    'flex h-7 shrink-0 items-center justify-start gap-0.5 bg-sidebar px-2.5 py-0 transition-[width] duration-200 ease-linear',
    dockOpen ? 'border-r border-border' : '',
  ].join(' ')}
  role="group"
  aria-label="Sidebar view"
>
  <Row class="items-center gap-0.5">
    {@render dockModeButton('projects', 'Projects')}
    {@render dockModeButton('explorer', 'Explorer')}
  </Row>
</div>
