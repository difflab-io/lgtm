<script lang="ts">
  import { onMount, tick, untrack } from 'svelte'
  import { getDocumentWhichKeyController } from '@cloudvoyant/helical-ui'
  import {
    CommandMenu,
    CommandPalette,
    CommandProvider,
    createToaster,
    Row,
    SidebarProvider,
    Toaster,
    ToastItem,
    WhichKey,
    WhichKeyProvider,
  } from '@cloudvoyant/helical-svelte'
  import { ThemeService } from 'lgtm-ui'
  import { errorMessage, workspaceClient, type WorkspaceClient } from '../clients/workspaceClient'
  import { createAppServices, setServicesContext } from '../services/context'
  import ContentPane from './ContentPane.svelte'
  import EditorToolbar from './EditorToolbar.svelte'
  import LeftDock from './LeftDock.svelte'
  import LeftDockToolbar from './LeftDockToolbar.svelte'
  import Titlebar from './Titlebar.svelte'

  // Props ---------------------------------------------------------------------
  interface ShellProps {
    client?: WorkspaceClient
  }
  let { client = workspaceClient }: ShellProps = $props()

  // State ---------------------------------------------------------------------
  const services = untrack(() => createAppServices(client))
  const { workspace, session, lsp, settings, hotkeys } = setServicesContext(services)
  const resolvedHotkeys = hotkeys!
  const theme = untrack(
    () =>
      new ThemeService({
        storageKey: 'lgtm.theme',
        themeClass: 'theme-lgtm',
        windowBridge: client.themeWindowBridge?.(),
      }),
  )
  const appToaster = createToaster({
    placement: 'top-end',
    overlap: true,
    max: 5,
    gap: 8,
    offsets: { top: '2.625rem', right: '0.75rem', bottom: '0.75rem', left: '0.75rem' },
  })

  // Derived -------------------------------------------------------------------
  let lastOpenSidebarWidth = $state(236)
  const activeSidebarWidth = $derived(workspace.leftDockWidth)
  $effect(() => {
    if (activeSidebarWidth > 0) {
      lastOpenSidebarWidth = activeSidebarWidth
    }
  })

  // Lifecycle -----------------------------------------------------------------
  $effect(() => {
    if (!settings.snapshot) return
    const nextTheme = settings.theme
    untrack(() => {
      if (theme.preference !== nextTheme) {
        theme.setPreference(nextTheme)
      }
    })
  })

  $effect(() => {
    const creationError = workspace.creationError
    untrack(() => {
      if (creationError) {
        appToaster.create({
          id: 'lgtm-creation-error',
          title: creationError,
          type: 'error',
          duration: 5000,
        })
      } else {
        appToaster.dismiss('lgtm-creation-error')
      }
    })
  })

  $effect(() => {
    const workspaceError = workspace.error
    untrack(() => {
      if (workspaceError) {
        appToaster.create({
          id: 'lgtm-workspace-error',
          title: workspaceError,
          type: 'error',
          duration: 5000,
        })
      } else {
        appToaster.dismiss('lgtm-workspace-error')
      }
    })
  })

  onMount(() => {
    let active = true
    const handleKeydown = (event: KeyboardEvent) => {
      if (
        resolvedHotkeys.commandOpen &&
        (event.metaKey || event.ctrlKey) &&
        !event.altKey &&
        !event.shiftKey &&
        event.key.toLowerCase() === 'k'
      ) {
        event.preventDefault()
        event.stopPropagation()
        resolvedHotkeys.setCommandOpen(false)
      }
    }
    window.addEventListener('keydown', handleKeydown, true)
    const unlisteners: (() => void)[] = [
      theme.init(),
      () => window.removeEventListener('keydown', handleKeydown, true),
    ]
    void workspace.open().then(async () => {
      if (!active || workspace.selected) return
      await tick()
      requestAnimationFrame(() => {
        if (
          active &&
          !workspace.selected &&
          (!document.activeElement || document.activeElement === document.body)
        ) {
          getDocumentWhichKeyController(document)?.focusPane('workspace')
        }
      })
    })
    if (client.isDesktop()) {
      void client
        .registerQuitHandlers(async () => {
          await workspace.flushSave()
          await session.saveCurrentSession()
        })
        .then(async (stops) => {
          for (const stop of stops) {
            if (active) unlisteners.push(stop)
            else stop()
          }
          if (active) await client.frontendReady()
        })
        .catch((error) => {
          if (active) workspace.openingError = errorMessage(error)
        })
      void client
        .registerLspHandlers({
          onStatus: (event) => {
            if (active) lsp.handleStatus(event)
          },
          onDiagnostics: (event) => {
            if (!active) return
            lsp.handleDiagnostics(event)
            if (workspace.activeTab?.kind === 'diagnostics') {
              void workspace.openDiagnosticsBuffer(workspace.activeTab.languageId ?? undefined)
            }
          },
        })
        .then((stops) => {
          for (const stop of stops) {
            if (active) unlisteners.push(stop)
            else stop()
          }
          if (active) void lsp.syncState()
        })
        .catch(() => {})
      void client
        .registerConfigHandlers?.({
          onConfigUpdated: (snapshot) => {
            settings.handleUpdated(snapshot)
            void workspace.reloadCleanConfigTabs()
          },
          onOpenFile: (event) => {
            void workspace.openPath(event.path, {
              title: event.title ?? undefined,
              offset: event.offset ?? undefined,
              focus: true,
            })
          },
        })
        .then((stops) => {
          for (const stop of stops) {
            if (active) unlisteners.push(stop)
            else stop()
          }
        })
        .catch(() => {})
      void client
        .registerSessionHandlers?.({
          onSessionUpdated: (nextSession) => {
            session.applySession(nextSession)
          },
        })
        .then((stops) => {
          for (const stop of stops) {
            if (active) unlisteners.push(stop)
            else stop()
          }
        })
        .catch(() => {})
    }
    return () => {
      active = false
      unlisteners.forEach((stop) => stop())
      workspace.dispose()
      session.dispose()
    }
  })
</script>

<svelte:head>
  <title>LGTM</title>
  <meta name="description" content="LGTM Hello Review" />
</svelte:head>

<WhichKeyProvider
  preset={settings.keybindPreset}
  leaderOnly={settings.leaderOnly}
  storageKey="lgtm"
  scoped={false}
  keymap={resolvedHotkeys.keymap}
  commands={resolvedHotkeys.commands}
  onPresetChange={(next) => {
    if (settings.keybindPreset !== next) {
      resolvedHotkeys.setPreset(next)
    }
  }}
  onLeaderOnlyChange={(next) => {
    if (settings.leaderOnly !== next) {
      resolvedHotkeys.setLeaderOnly(next)
    }
  }}
>
  <CommandProvider
    items={resolvedHotkeys.commandItems}
    open={resolvedHotkeys.commandOpen}
    onOpenChange={(open) => resolvedHotkeys.setCommandOpen(open)}
    style={resolvedHotkeys.effectiveCommandStyle}
  >
    <SidebarProvider
      open={workspace.projectsOpen || workspace.explorerOpen}
      onOpenChange={() => {}}
      data-pane-outline={settings.paneOutline ? 'true' : 'false'}
      class="workspace-shell h-dvh min-h-0 flex-col gap-0 overflow-hidden"
      style="--sidebar-width: {activeSidebarWidth || lastOpenSidebarWidth}px"
    >
      <Titlebar />
      <Row class="min-h-0 flex-1 items-stretch gap-0 overflow-hidden">
        <LeftDock />
        <ContentPane />
      </Row>
      <footer
        data-testid="bottom-toolbar"
        class="flex h-7 shrink-0 items-stretch gap-0 border-t border-border bg-sidebar"
      >
        <LeftDockToolbar />
        <EditorToolbar />
      </footer>
    </SidebarProvider>
    <CommandMenu size="sm" showAliases hotkey={false} />
    <CommandPalette size="sm" hotkey={false} />
  </CommandProvider>
  <Toaster toaster={appToaster}>
    {#snippet children(toastAccessor)}
      {@const toastData = toastAccessor()}
      <ToastItem toast={toastData} role={toastData.type === 'error' ? 'alert' : 'status'} />
    {/snippet}
  </Toaster>
  <WhichKey />
</WhichKeyProvider>
