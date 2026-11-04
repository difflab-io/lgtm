<script lang="ts">
  import { Menu, type MenuItem, type MenuItemIndicatorTone } from 'lgtm-ui'
  import { Radio } from 'lucide-svelte'
  import type { WorkspaceService } from '../services/workspaceService.svelte'

  // Props ---------------------------------------------------------------------
  interface LspStatusMenuProps {
    workspace: WorkspaceService
  }
  let { workspace }: LspStatusMenuProps = $props()

  // Derived -------------------------------------------------------------------
  const triggerSummary = $derived.by(() => {
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

  const triggerTone = $derived.by((): MenuItemIndicatorTone => {
    const server = workspace.activeLspServer
    if (!server) return 'inactive'
    return workspace.lsp.getServerIndicator(server, workspace.activeLanguageId).tone
  })

  const menuItems = $derived.by((): MenuItem[] => {
    const items: MenuItem[] = []
    const servers = workspace.allLspServers
    if (servers.length === 0) {
      items.push({
        value: 'empty',
        label: 'No active language servers',
        indicator: 'inactive',
        disabled: true,
      })
    } else {
      for (const server of servers) {
        const indicator = workspace.lsp.getServerIndicator(server, workspace.activeLanguageId)
        const subItems: MenuItem[] = [
          {
            value: `diagnostics:${server.languageId}`,
            label: `View ${server.languageId} diagnostics`,
          },
          {
            value: `restart:${server.languageId}`,
            label: `Restart ${server.languageId} server`,
          },
        ]
        items.push({
          value: `server:${server.languageId}`,
          label: server.languageId,
          detail: `${server.status} · ${server.errorCount}E ${server.warningCount}W`,
          indicator: indicator.tone,
          indicatorLabel: indicator.label,
          items: subItems,
        })
      }
    }

    for (const [idx, diag] of workspace.lsp.diagnosticEntries.entries()) {
      items.push({
        value: `jump:${diag.path}:${diag.from}:${idx}`,
        label: `${diag.path}:${diag.line + 1}:${diag.character + 1} [${diag.severity.toUpperCase()}] ${diag.message}`,
        indicator:
          diag.severity === 'error'
            ? 'error'
            : diag.severity === 'warning'
              ? 'warning'
              : 'active-project',
        separatorBefore: idx === 0,
      })
    }

    items.push({
      value: 'diagnostics:all',
      label: 'Open diagnostics buffer',
      separatorBefore: true,
    })

    return items
  })

  function handleSelect(value: string) {
    if (value === 'diagnostics:all') {
      void workspace.openDiagnosticsBuffer()
      return
    }
    if (value.startsWith('diagnostics:')) {
      const languageId = value.slice('diagnostics:'.length)
      void workspace.openDiagnosticsBuffer(languageId)
      return
    }
    if (value.startsWith('restart:')) {
      const languageId = value.slice('restart:'.length)
      void workspace.restartLanguageServer(languageId)
      return
    }
    if (value.startsWith('jump:')) {
      const parts = value.split(':')
      const offset = Number(parts.at(-2) ?? 0)
      const path = parts.slice(1, -2).join(':')
      if (path) {
        void workspace.jumpToLocation(path, offset)
      }
    }
  }
</script>

<Menu
  items={menuItems}
  ariaLabel="LSP servers and diagnostics"
  triggerAriaLabel="LSP status"
  triggerTitle={triggerSummary}
  onopenchange={(open) => {
    if (open) void workspace.syncLspState()
  }}
  onselect={handleSelect}
>
  {#snippet trigger()}
    <Radio size={12} strokeWidth={1.75} data-lsp-summary={triggerSummary} />
    <span
      class={[
        'inline-block size-1.5 rounded-full',
        triggerTone === 'error' && 'bg-rose-500',
        triggerTone === 'warning' && 'bg-amber-500',
        triggerTone === 'active-page' && 'bg-emerald-500',
        triggerTone === 'active-project' && 'bg-sky-500',
        triggerTone === 'inactive' && 'bg-slate-300',
      ]}
      data-lsp-tone={triggerTone}
    ></span>
    <span class="sr-only">{triggerSummary}</span>
  {/snippet}
</Menu>
