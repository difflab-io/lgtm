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
  import { setServicesContext } from '../services/context'
  import { HotkeyService } from '../services/hotkeyService.svelte'
  import { WorkspaceService } from '../services/workspaceService.svelte'
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
  const workspace = untrack(() => new WorkspaceService(client))
  const { session } = setServicesContext(workspace)
  const hotkeys = untrack(() => new HotkeyService(workspace))
  const theme = untrack(
    () =>
      new ThemeService({
        storageKey: 'lgtm.theme',
        themeClass: 'theme-lgtm',
        windowBridge: client.themeWindowBridge?.(),
      }),
  )

  // Derived -------------------------------------------------------------------
  let lastOpenSidebarWidth = $state(236)
  const activeSidebarWidth = $derived(session.leftDockWidth)
  $effect(() => {
    if (activeSidebarWidth > 0) {
      lastOpenSidebarWidth = activeSidebarWidth
    }
  })

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
          onSessionUpdated: (nextSession) => {
            workspace.applySession(nextSession)
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
      open={session.projectsOpen || session.explorerOpen}
      onOpenChange={() => {}}
      class="workspace-shell h-dvh min-h-0 flex-col gap-0 overflow-hidden"
      style="--sidebar-width: {activeSidebarWidth || lastOpenSidebarWidth}px"
    >
      <Titlebar />
      <Row class="min-h-0 flex-1 items-stretch gap-0">
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
    <CommandMenu
      open={(hotkeys.effectiveCommandStyle === 'menu' ||
        hotkeys.effectiveCommandStyle === 'colon') &&
        hotkeys.commandOpen}
      onOpenChange={(open) => hotkeys.setCommandOpen(open)}
      hotkey={false}
      showShortcuts
    />
    <CommandPalette
      open={hotkeys.effectiveCommandStyle === 'palette' && hotkeys.commandOpen}
      onOpenChange={(open) => hotkeys.setCommandOpen(open)}
      hotkey={false}
    />
  </CommandProvider>
  <WhichKey />
</WhichKeyProvider>
