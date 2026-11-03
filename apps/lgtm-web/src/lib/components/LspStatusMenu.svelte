<script lang="ts">
  import { Button } from '@cloudvoyant/helical-svelte'
  import type { WorkspaceService } from '../services/workspaceService.svelte'

  // Props ---------------------------------------------------------------------
  interface LspStatusMenuProps {
    workspace: WorkspaceService
  }
  let { workspace }: LspStatusMenuProps = $props()

  // State ---------------------------------------------------------------------
  let open = $state(false)

  // Derived -------------------------------------------------------------------
  const triggerLabel = $derived.by(() => {
    const server = workspace.activeLspServer
    if (!server) {
      return workspace.activeLanguageId ? `LSP: ${workspace.activeLanguageId}` : 'LSP: idle'
    }
    const counts: string[] = []
    if (server.errorCount > 0) {
      counts.push(`${server.errorCount} ${server.errorCount === 1 ? 'error' : 'errors'}`)
    }
    if (server.warningCount > 0) {
      counts.push(`${server.warningCount} ${server.warningCount === 1 ? 'warning' : 'warnings'}`)
    }
    const detail = counts.length > 0 ? `${server.status}, ${counts.join(', ')}` : server.status
    return `LSP: ${server.languageId} (${detail})`
  })

  const diagnosticEntries = $derived.by(() => {
    const items: {
      path: string
      from: number
      line: number
      character: number
      severity: string
      message: string
    }[] = []
    for (const [path, list] of Object.entries(workspace.lspDiagnostics)) {
      for (const diag of list) {
        items.push({
          path,
          from: diag.from,
          line: diag.line,
          character: diag.character,
          severity: diag.severity,
          message: diag.message,
        })
      }
    }
    return items
  })

  async function openBuffer(languageId?: string) {
    open = false
    await workspace.openDiagnosticsBuffer(languageId)
  }

  async function jumpToDiagnostic(path: string, offset: number) {
    open = false
    await workspace.jumpToLocation(path, offset)
  }
</script>

<div class="lsp-status-container">
  <button
    type="button"
    class="lsp-status-trigger"
    aria-label="LSP status"
    aria-expanded={open}
    onclick={() => {
      open = !open
      if (open) void workspace.syncLspState()
    }}
  >
    {triggerLabel}
  </button>

  {#if open}
    <div class="lsp-status-popover" role="dialog" aria-label="LSP servers and diagnostics">
      <div class="lsp-popover-header">
        <span>Language Servers</span>
        <Button variant="outline" size="sm" onclick={() => openBuffer()}>
          Open diagnostics buffer
        </Button>
      </div>

      {#if workspace.allLspServers.length === 0}
        <p class="lsp-popover-empty">No active language servers.</p>
      {:else}
        <ul class="lsp-server-list" aria-label="Active language servers">
          {#each workspace.allLspServers as server (server.languageId)}
            <li class="lsp-server-item">
              <div class="lsp-server-meta">
                <strong>{server.languageId}</strong>
                <span class="lsp-server-badge" data-status={server.status}>{server.status}</span>
                <span class="lsp-server-counts">
                  {server.errorCount}E / {server.warningCount}W
                </span>
              </div>
              <Button variant="text" size="sm" onclick={() => openBuffer(server.languageId)}>
                View {server.languageId} diagnostics
              </Button>
            </li>
          {/each}
        </ul>
      {/if}

      {#if diagnosticEntries.length > 0}
        <div class="lsp-diagnostics-section">
          <div class="lsp-popover-subheader">Diagnostics</div>
          <ul class="lsp-diagnostic-list" aria-label="Workspace diagnostics">
            {#each diagnosticEntries as item, idx (`${item.path}:${item.from}:${idx}`)}
              <li>
                <button
                  type="button"
                  class="lsp-diagnostic-item"
                  onclick={() => jumpToDiagnostic(item.path, item.from)}
                >
                  <span class="lsp-diagnostic-loc"
                    >{item.path}:{item.line + 1}:{item.character + 1}</span
                  >
                  <span class="lsp-diagnostic-msg"
                    >[{item.severity.toUpperCase()}] {item.message}</span
                  >
                </button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
    </div>
  {/if}
</div>
