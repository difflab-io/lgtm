<script lang="ts">
  import { Pane } from '@cloudvoyant/helical-svelte'
  import { useService } from '../services/context'
  import { terminalStatusDotClass } from '../services/terminalService.svelte'
  import SidebarPaneHeader from './SidebarPaneHeader.svelte'

  // State ---------------------------------------------------------------------
  const workspace = useService('workspace')
  const settings = useService('settings')
  const terminal = useService('terminal')

  const agents = $derived(terminal.detectedAgents)

  function workspaceLabel(root: string): string {
    return root.split('/').filter(Boolean).at(-1) ?? root
  }

  function startVerticalResize(event: PointerEvent) {
    event.preventDefault()
    const startY = event.clientY
    const startHeight = workspace.agentsPaneHeight
    workspace.setResizing(true)

    function onMove(moveEvent: PointerEvent) {
      const delta = startY - moveEvent.clientY
      workspace.setAgentsPaneHeight(startHeight + delta)
    }

    function onUp() {
      workspace.setResizing(false)
      workspace.persistDockLayout()
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
    }

    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
  }
</script>

<div
  style="height: {workspace.agentsPaneHeight}px"
  class="relative flex shrink-0 flex-col border-t border-border bg-sidebar"
>
  <div
    role="separator"
    aria-orientation="horizontal"
    aria-label="Resize Agents section"
    data-testid="agents-pane-resizer"
    class="absolute top-0 left-0 z-10 h-1 w-full cursor-row-resize hover:bg-ring/40"
    onpointerdown={startVerticalResize}
  ></div>

  <Pane
    id="agents-dock-pane"
    data-testid="agents-dock-pane"
    paneId="agents"
    paneTitle="Agents"
    paneDisabled={!workspace.projectsOpen}
    class={[
      'relative flex min-h-0 flex-1 flex-col overflow-hidden bg-sidebar outline-none',
      settings.paneOutline &&
        'data-[pane-active=true]:after:pointer-events-none data-[pane-active=true]:after:absolute data-[pane-active=true]:after:inset-0 data-[pane-active=true]:after:z-20 data-[pane-active=true]:after:ring-1 data-[pane-active=true]:after:ring-ring/40 data-[pane-active=true]:after:ring-inset',
    ]
      .filter(Boolean)
      .join(' ')}
    aria-label="Agents"
  >
    <SidebarPaneHeader title="Agents" class="px-3" />

    <div class="min-h-0 flex-1 overflow-y-auto py-1">
      {#if agents.length === 0}
        <p class="px-3 py-1 text-[11px] text-muted-foreground">No active agents.</p>
      {:else}
        <ul class="flex flex-col" role="list" aria-label="Detected coding agents">
          {#each agents as session (session.id)}
            <li>
              <button
                type="button"
                data-testid="agent-row"
                data-agent-app-id={session.agent?.appId ?? ''}
                data-agent-status={session.status}
                data-agent-session-id={session.id}
                class="flex h-6 w-full cursor-pointer items-center justify-between gap-2 rounded-none px-3 text-left text-[11px] text-foreground hover:bg-accent"
                onclick={() => void terminal.jumpToSession(session.id)}
              >
                <span class="flex min-w-0 items-center gap-1.5">
                  <span
                    data-testid="agent-status-dot"
                    data-status={session.status}
                    class={[
                      'size-1.5 shrink-0 rounded-full',
                      terminalStatusDotClass(session.status),
                    ].join(' ')}
                    aria-label={`Agent status: ${session.status}`}
                  ></span>
                  <span class="truncate font-medium">
                    {session.agent?.label ?? session.title}
                  </span>
                </span>
                <span class="truncate text-[10px] text-muted-foreground">
                  {workspaceLabel(session.workspaceRoot)}
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </Pane>
</div>
