<script lang="ts">
  import { Button } from '@cloudvoyant/helical-svelte'
  import { SquareTerminal } from 'lucide-svelte'
  import type { RightDockWidget } from '../clients/workspaceClient'
  import { useService } from '../services/context'

  // State ---------------------------------------------------------------------
  const workspace = useService('workspace')
  let buttonsWidth = $state(28)
  const minToolbarWidth = $derived(buttonsWidth + 20)
  const toolbarWidth = $derived(
    workspace.rightDockOpen ? Math.max(workspace.rightDockWidth, minToolbarWidth) : minToolbarWidth,
  )
</script>

{#snippet rightWidgetButton(widget: RightDockWidget, label: string)}
  {@const active = workspace.rightDockOpen && workspace.rightDockWidget === widget}
  <Button
    variant="text"
    size="sm"
    aria-label="{label} dock"
    title={label}
    aria-pressed={active}
    data-testid={`right-dock-toggle-${widget}`}
    class={[
      'size-6 bg-transparent p-1 text-[10px] font-medium hover:bg-transparent',
      active ? 'text-foreground' : 'text-muted-foreground hover:text-foreground',
    ].join(' ')}
    onclick={() => workspace.toggleRightDock(widget)}
  >
    <SquareTerminal size={14} strokeWidth={1.5} />
  </Button>
{/snippet}

<div
  data-testid="right-dock-toolbar"
  style="width: {toolbarWidth}px"
  class={[
    'flex h-7 w-min min-w-min shrink-0 items-center justify-end gap-0.5 border-l border-border bg-sidebar px-2.5 py-0',
    !workspace.resizing ? 'transition-[width] duration-200 ease-linear' : '',
  ].join(' ')}
  role="group"
  aria-label="Right dock widgets"
>
  <div bind:offsetWidth={buttonsWidth} class="flex w-min shrink-0 items-center gap-0.5">
    {@render rightWidgetButton('terminal', 'Terminal')}
  </div>
</div>
