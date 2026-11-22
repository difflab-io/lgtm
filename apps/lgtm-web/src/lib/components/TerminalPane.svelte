<script lang="ts">
  import { Button } from '@cloudvoyant/helical-svelte'
  import { TerminalSurface, type TerminalSurfaceHandle } from 'lgtm-ui'
  import { Plus, SquareTerminal, X } from 'lucide-svelte'
  import { useService } from '../services/context'
  import { terminalStatusDotClass } from '../services/terminalService.svelte'

  // State ---------------------------------------------------------------------
  const workspace = useService('workspace')
  const terminal = useService('terminal')

  const tabs = $derived(terminal.activeWorkspaceTabs)
  const activeSessionId = $derived(terminal.activeSessionId)

  // Lifecycle -----------------------------------------------------------------
  $effect(() => {
    const open = workspace.rightDockOpen && workspace.rightDockWidget === 'terminal'
    const root = workspace.root
    if (open && root) {
      void terminal.ensureActiveWorkspaceTerminal(root)
    }
  })

  $effect(() => {
    const sid = activeSessionId
    if (!sid) return
    return () => {
      void terminal.detachSurface(sid)
    }
  })

  function handleSurfaceReady(sessionId: string, handle: TerminalSurfaceHandle) {
    void terminal.attachSurface(sessionId, 80, 24, handle)
  }
</script>

<div
  data-testid="terminal-pane"
  class="flex h-full min-h-0 flex-1 flex-col overflow-hidden bg-background"
>
  <div
    data-testid="terminal-tab-bar"
    role="tablist"
    aria-label="Workspace terminals"
    class="flex h-7 shrink-0 items-center justify-between gap-1 border-b border-border bg-sidebar px-1.5"
  >
    <div class="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto">
      {#each tabs as tab (tab.id)}
        {@const isSelected = tab.id === activeSessionId}
        <div
          role="tab"
          tabindex={isSelected ? 0 : -1}
          aria-selected={isSelected}
          data-testid="terminal-tab"
          data-terminal-tab-id={tab.id}
          data-terminal-status={tab.status}
          class={[
            'group flex h-6 max-w-[160px] shrink-0 cursor-pointer items-center gap-1.5 rounded-none px-2 text-[11px] transition-colors',
            isSelected
              ? 'bg-background text-foreground'
              : 'text-muted-foreground hover:bg-accent hover:text-foreground',
          ].join(' ')}
          onclick={() => void terminal.selectTab(tab.id)}
          onkeydown={(event) => {
            if (event.key === 'Enter' || event.key === ' ') {
              event.preventDefault()
              void terminal.selectTab(tab.id)
            }
          }}
        >
          <span
            data-testid="terminal-tab-dot"
            data-status={tab.status}
            class={['size-1.5 shrink-0 rounded-full', terminalStatusDotClass(tab.status)].join(' ')}
            aria-label={`Status: ${tab.status}`}
          ></span>
          <span class="truncate">{tab.title}</span>
          <button
            type="button"
            aria-label={`Close terminal ${tab.title}`}
            data-testid="terminal-tab-close"
            class="ml-0.5 inline-flex size-4 shrink-0 items-center justify-center rounded-none text-muted-foreground opacity-60 hover:bg-accent hover:text-foreground hover:opacity-100"
            onclick={(event) => {
              event.stopPropagation()
              void terminal.closeTab(tab.id)
            }}
          >
            <X size={11} strokeWidth={1.75} />
          </button>
        </div>
      {/each}
    </div>

    <div class="flex shrink-0 items-center gap-0.5">
      <Button
        variant="text"
        size="sm"
        aria-label="New terminal"
        title="New terminal"
        data-testid="terminal-new-button"
        class="size-6 rounded-none p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
        onclick={() => void terminal.createTab()}
      >
        <Plus size={13} strokeWidth={1.75} />
      </Button>
    </div>
  </div>

  {#if terminal.error}
    <div
      role="alert"
      data-testid="terminal-error"
      class="border-b border-border bg-destructive/10 px-3 py-1.5 text-[11px] text-destructive"
    >
      {terminal.error}
    </div>
  {/if}

  <div class="relative flex min-h-0 flex-1 flex-col overflow-hidden">
    {#if activeSessionId && workspace.rightDockOpen && workspace.rightDockWidget === 'terminal'}
      {#key activeSessionId}
        <TerminalSurface
          sessionId={activeSessionId}
          onReady={(handle) => handleSurfaceReady(activeSessionId, handle)}
          onInput={(bytes) => void terminal.writeInput(activeSessionId, bytes)}
          onResize={(cols, rows) => void terminal.resize(activeSessionId, cols, rows)}
        />
      {/key}
    {:else}
      <div
        class="flex flex-1 flex-col items-center justify-center gap-2 p-4 text-xs text-muted-foreground"
      >
        <SquareTerminal size={18} strokeWidth={1.5} class="opacity-60" />
        <span>No active terminal in this workspace.</span>
        <Button variant="outline" size="sm" onclick={() => void terminal.createTab()}>
          New terminal
        </Button>
      </div>
    {/if}
  </div>
</div>
