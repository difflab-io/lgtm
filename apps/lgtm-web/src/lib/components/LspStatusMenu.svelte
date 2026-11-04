<script lang="ts">
  import { Menu, type MenuItem, type MenuItemIndicatorTone } from 'lgtm-ui'
  import { CircleX, Radio, TriangleAlert } from 'lucide-svelte'
  import type { LspServerStatusEvent } from '../clients/workspaceClient'
  import type { WorkspaceService } from '../services/workspaceService.svelte'

  // Props ---------------------------------------------------------------------
  interface LspStatusMenuProps {
    workspace: WorkspaceService
  }
  let { workspace }: LspStatusMenuProps = $props()

  // Derived -------------------------------------------------------------------
  const activeServer = $derived(workspace.activeLspServer)
  const activeErrorCount = $derived(activeServer?.errorCount ?? 0)
  const activeWarningCount = $derived(activeServer?.warningCount ?? 0)

  const triggerSummary = $derived.by(() => {
    const server = activeServer
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
    const server = activeServer
    if (!server) return 'inactive'
    return workspace.lsp.getServerIndicator(server, workspace.activeLanguageId).tone
  })

  const serversByLanguageId = $derived.by((): Record<string, LspServerStatusEvent> => {
    const map: Record<string, LspServerStatusEvent> = {}
    for (const server of workspace.allLspServers) {
      map[server.languageId] = server
    }
    return map
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
          indicator: indicator.tone,
          indicatorLabel: indicator.label,
          detailRender: serverCountsDetail,
          items: subItems,
        })
      }
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
  }
</script>

{#snippet serverCountsDetail(item: MenuItem)}
  {@const server = serversByLanguageId[item.label]}
  {#if server}
    {#if server.status !== 'ready'}
      <span>{server.status}</span>
    {/if}
    {#if server.errorCount > 0}
      <span class="inline-flex items-center gap-0.5 text-error" data-slot="lsp-server-errors">
        <CircleX size={11} strokeWidth={1.75} />
        <span>{server.errorCount}</span>
      </span>
    {/if}
    {#if server.warningCount > 0}
      <span class="inline-flex items-center gap-0.5 text-warn" data-slot="lsp-server-warnings">
        <TriangleAlert size={11} strokeWidth={1.75} />
        <span>{server.warningCount}</span>
      </span>
    {/if}
  {/if}
{/snippet}

<Menu
  items={menuItems}
  placement="top-start"
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
    {#if activeErrorCount > 0}
      <span
        class="inline-flex items-center gap-0.5 text-error"
        data-slot="lsp-error-count"
        aria-hidden="true"
      >
        <CircleX size={11} strokeWidth={1.75} />
        <span>{activeErrorCount}</span>
      </span>
    {/if}
    {#if activeWarningCount > 0}
      <span
        class="inline-flex items-center gap-0.5 text-warn"
        data-slot="lsp-warning-count"
        aria-hidden="true"
      >
        <TriangleAlert size={11} strokeWidth={1.75} />
        <span>{activeWarningCount}</span>
      </span>
    {/if}
    <span class="sr-only">{triggerSummary}</span>
  {/snippet}
</Menu>
