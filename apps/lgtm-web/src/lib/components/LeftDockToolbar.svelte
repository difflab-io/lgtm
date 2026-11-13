<script lang="ts">
  import { Button } from '@cloudvoyant/helical-svelte'
  import { Box, FolderTree } from 'lucide-svelte'
  import type { SidebarWidget } from '../clients/workspaceClient'
  import { useService } from '../services/context'

  // State ---------------------------------------------------------------------
  const session = useService('session')
  const dockOpen = $derived(session.projectsOpen || session.explorerOpen)
  let buttonsWidth = $state(50)
  const minToolbarWidth = $derived(buttonsWidth + 20)
  const toolbarWidth = $derived(
    dockOpen ? Math.max(session.leftDockWidth, minToolbarWidth) : minToolbarWidth,
  )
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
  style="width: {toolbarWidth}px"
  class={[
    'flex h-7 w-min min-w-min shrink-0 items-center justify-start gap-0.5 border-r border-border bg-sidebar px-2.5 py-0',
    !session.resizing ? 'transition-[width] duration-200 ease-linear' : '',
  ].join(' ')}
  role="group"
  aria-label="Sidebar view"
>
  <div bind:offsetWidth={buttonsWidth} class="flex w-min shrink-0 items-center gap-0.5">
    {@render dockModeButton('projects', 'Projects')}
    {@render dockModeButton('explorer', 'Explorer')}
  </div>
</div>
