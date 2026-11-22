<script lang="ts">
  import { Sidebar } from '@cloudvoyant/helical-svelte'
  import { useService } from '../services/context'
  import TerminalPane from './TerminalPane.svelte'

  // State ---------------------------------------------------------------------
  const workspace = useService('workspace')
  const settings = useService('settings')
  const terminal = useService('terminal')

  function startResize(event: PointerEvent) {
    event.preventDefault()
    const startX = event.clientX
    const startWidth = workspace.rightDockWidth
    workspace.setResizing(true)

    function onMove(moveEvent: PointerEvent) {
      const delta = startX - moveEvent.clientX
      workspace.setRightDockWidth(startWidth + delta)
    }

    function onUp() {
      workspace.setResizing(false)
      workspace.persistDockLayout()
      requestAnimationFrame(() => terminal.fitActiveSurface())
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
    }

    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
  }
</script>

<div
  style="width: {workspace.rightDockOpen ? `${workspace.rightDockWidth}px` : '0px'}"
  class={[
    'relative h-full min-h-0 shrink-0 overflow-hidden',
    !workspace.resizing ? 'transition-[width] duration-200 ease-linear' : '',
    workspace.rightDockOpen ? 'border-l border-border' : '',
  ].join(' ')}
>
  {#if workspace.rightDockOpen}
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize right dock"
      data-testid="right-dock-resizer"
      class="absolute top-0 left-0 z-10 h-full w-1 cursor-col-resize hover:bg-ring/40"
      onpointerdown={startResize}
    ></div>
  {/if}

  <Sidebar
    id="right-dock-pane"
    data-testid="right-dock-pane"
    paneId="right-dock"
    paneTitle="Terminal"
    paneDisabled={!workspace.rightDockOpen}
    collapsible="none"
    aria-hidden={!workspace.rightDockOpen}
    inert={!workspace.rightDockOpen ? true : undefined}
    style="width: {workspace.rightDockWidth}px"
    class={[
      'relative h-full min-h-0 shrink-0 overflow-hidden bg-sidebar outline-none',
      settings.paneOutline &&
        'data-[pane-active=true]:after:pointer-events-none data-[pane-active=true]:after:absolute data-[pane-active=true]:after:inset-0 data-[pane-active=true]:after:z-20 data-[pane-active=true]:after:ring-1 data-[pane-active=true]:after:ring-ring/40 data-[pane-active=true]:after:ring-inset',
    ]
      .filter(Boolean)
      .join(' ')}
    aria-label="Right dock"
  >
    {#if workspace.rightDockOpen && workspace.rightDockWidget === 'terminal'}
      <TerminalPane />
    {/if}
  </Sidebar>
</div>
