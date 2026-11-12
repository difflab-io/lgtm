<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import {
    CommandMenu,
    CommandPalette,
    CommandProvider,
    Row,
    SidebarProvider,
    WhichKey,
    WhichKeyProvider,
  } from '@cloudvoyant/helical-svelte'
  import { ThemeService } from 'lgtm-ui'
  import { errorMessage, workspaceClient, type WorkspaceClient } from '../clients/workspaceClient'
  import { HotkeyService } from '../services/hotkeyService.svelte'
  import { WorkspaceService } from '../services/workspaceService.svelte'
  import ContentPane from './ContentPane.svelte'
  import Titlebar from './Titlebar.svelte'
  import WorkspaceDock from './WorkspaceDock.svelte'

  // Props ---------------------------------------------------------------------
  interface ShellProps {
    client?: WorkspaceClient
  }
  let { client = workspaceClient }: ShellProps = $props()

  // State ---------------------------------------------------------------------
  const workspace = untrack(() => new WorkspaceService(client))
  const hotkeys = untrack(() => new HotkeyService(workspace))
  const theme = untrack(
    () =>
      new ThemeService({
        storageKey: 'lgtm.theme',
        themeClass: 'theme-lgtm',
        windowBridge: client.themeWindowBridge?.(),
      }),
  )

  // Lifecycle -----------------------------------------------------------------
  $effect(() => {
    if (!workspace.settings.snapshot) return
    const nextTheme = workspace.settings.theme
    untrack(() => {
      if (theme.preference !== nextTheme) {
        theme.setPreference(nextTheme)
      }
    })
  })

  onMount(() => {
    let active = true
    const unlisteners: (() => void)[] = [theme.init()]
    void workspace.open()
    if (client.isDesktop()) {
      void client
        .registerQuitHandlers(async () => {
          await workspace.flushSave()
          await workspace.saveCurrentWorkspaceSession()
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
          onStatus: (event) => workspace.handleLspStatus(event),
          onDiagnostics: (event) => workspace.handleLspDiagnostics(event),
        })
        .then((stops) => {
          for (const stop of stops) {
            if (active) unlisteners.push(stop)
            else stop()
          }
          if (active) void workspace.syncLspState()
        })
        .catch(() => {})
      void client
        .registerConfigHandlers?.({
          onConfigUpdated: (snapshot) => {
            workspace.settings.handleUpdated(snapshot)
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
          onSessionUpdated: (session) => {
            workspace.applySession(session)
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
    }
  })
</script>

<svelte:head>
  <title>LGTM</title>
  <meta name="description" content="LGTM Hello Review" />
</svelte:head>

<WhichKeyProvider
  preset="helix"
  storageKey="lgtm"
  scoped={false}
  keymap={hotkeys.keymap}
  commands={hotkeys.commands}
>
  <CommandProvider
    items={hotkeys.commandItems}
    open={hotkeys.commandOpen}
    onOpenChange={(open) => hotkeys.setCommandOpen(open)}
    style={hotkeys.effectiveCommandStyle}
  >
    <SidebarProvider
      open={workspace.projectsOpen || workspace.explorerOpen}
      onOpenChange={() => {}}
      class="workspace-shell h-dvh min-h-0 flex-col gap-0 overflow-hidden"
      style="--sidebar-width: {(workspace.projectsOpen ? 180 : 0) +
        (workspace.explorerOpen ? 236 : 0)}px"
    >
      <Titlebar {workspace} />
      <Row class="min-h-0 flex-1 items-stretch gap-0">
        <WorkspaceDock {workspace} {client} />
        <ContentPane {workspace} />
      </Row>
    </SidebarProvider>
    <CommandMenu hotkey={false} showShortcuts />
    <CommandPalette hotkey={false} />
  </CommandProvider>
  <WhichKey />
</WhichKeyProvider>
