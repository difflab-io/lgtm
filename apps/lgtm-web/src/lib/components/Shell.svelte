<script lang="ts">
  import { onMount, tick, untrack } from 'svelte'
  import { getDocumentWhichKeyController } from '@cloudvoyant/helical-ui'
  import {
    Button,
    CommandMenu,
    CommandPalette,
    CommandProvider,
    Row,
    SidebarProvider,
    SidebarTrigger,
    Sidebar,
    SidebarHeader,
    SidebarContent,
    Select,
    SelectTrigger,
    SelectValue,
    SelectIndicator,
    SelectContent,
    SelectItem,
    SelectItemText,
    WhichKey,
    WhichKeyProvider,
  } from '@cloudvoyant/helical-svelte'
  import { FilePlus2, FolderPlus, PanelLeft, ChevronDown } from 'lucide-svelte'
  import { ThemeService } from 'lgtm-ui'
  import { errorMessage, workspaceClient, type WorkspaceClient } from '../clients/workspaceClient'
  import { HotkeyService } from '../services/hotkeyService.svelte'
  import { WorkspaceService } from '../services/workspaceService.svelte'
  import WorkspaceExplorer from './WorkspaceExplorer.svelte'
  import ContentPane from './ContentPane.svelte'

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
    const handleKeydown = (event: KeyboardEvent) => {
      if (
        hotkeys.commandOpen &&
        (event.metaKey || event.ctrlKey) &&
        !event.altKey &&
        !event.shiftKey &&
        event.key.toLowerCase() === 'k'
      ) {
        event.preventDefault()
        event.stopPropagation()
        hotkeys.setCommandOpen(false)
        return
      }
      const isComma = event.key === ',' || event.key === '<' || event.code === 'Comma'
      if ((event.metaKey || event.ctrlKey) && !event.altKey && isComma) {
        event.preventDefault()
        if (event.shiftKey || event.key === '<') {
          void workspace.openPath('.lgtm/config.json', { title: 'Settings (project)', focus: true })
        } else {
          void workspace.openPath('~/.lgtm/config.json', { title: 'Settings', focus: true })
        }
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
        .registerQuitHandlers(() => workspace.flushSave())
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

{#snippet sidebarNotice(text: string, role?: 'alert' | 'status')}
  <p
    class={['mx-3.5 my-2 text-xs leading-relaxed wrap-anywhere', role === 'alert' && 'text-warn']}
    {role}
  >
    {text}
  </p>
{/snippet}

<WhichKeyProvider
  preset={workspace.settings.keybindPreset}
  leaderOnly={workspace.settings.leaderOnly}
  storageKey="lgtm"
  scoped={false}
  keymap={hotkeys.keymap}
  commands={hotkeys.commands}
  onPresetChange={(next) => {
    if (workspace.settings.keybindPreset !== next) {
      hotkeys.setPreset(next)
    }
  }}
  onLeaderOnlyChange={(next) => {
    if (workspace.settings.leaderOnly !== next) {
      hotkeys.setLeaderOnly(next)
    }
  }}
>
  <CommandProvider
    items={hotkeys.commandItems}
    open={hotkeys.commandOpen}
    onOpenChange={(open) => hotkeys.setCommandOpen(open)}
    style={hotkeys.effectiveCommandStyle}
  >
    <SidebarProvider
      open={hotkeys.sidebarOpen}
      onOpenChange={(open) => (hotkeys.sidebarOpen = open)}
      data-pane-outline={workspace.settings.paneOutline ? 'true' : 'false'}
      class="workspace-shell h-dvh min-h-0 flex-col gap-0 overflow-hidden"
      style="--sidebar-width: 236px"
    >
      <header
        class="titlebar flex h-9 shrink-0 items-center gap-4 border-b border-border bg-sidebar pr-3 pl-20 text-sidebar-foreground select-none"
        data-tauri-drag-region
      >
        <SidebarTrigger title="Toggle sidebar" aria-controls="workspace-sidebar"
          ><PanelLeft size={14} strokeWidth={1.5} /></SidebarTrigger
        >
        <span class="brand text-xs font-semibold tracking-tight" data-tauri-drag-region>lgtm</span>
        <div
          class="workspace-chooser w-45 min-w-0 [&_[data-part=trigger]]:h-6.5 [&_[data-part=trigger]]:w-full [&_[data-part=trigger]]:min-w-0 [&_[data-part=trigger]]:border-0 [&_[data-part=trigger]]:bg-transparent [&_[data-part=trigger]]:text-[11px] [&_[data-part=trigger]]:shadow-none"
        >
          <Select
            items={workspace.items}
            value={[workspace.root]}
            size="sm"
            onValueChange={({ value }) => {
              if (value[0]) void workspace.selectWorkspace(value[0])
            }}
          >
            <SelectTrigger aria-label="Workspace selector" title={workspace.root} size="sm"
              ><SelectValue /><SelectIndicator
                ><ChevronDown size={12} strokeWidth={1.5} /></SelectIndicator
              ></SelectTrigger
            >
            <SelectContent
              >{#each workspace.items as item (item.value)}<SelectItem {item}
                  ><SelectItemText>{item.label}</SelectItemText></SelectItem
                >{/each}</SelectContent
            >
          </Select>
        </div>
      </header>
      <Row class="min-h-0 flex-1 items-stretch gap-0">
        <Sidebar
          id="workspace-sidebar"
          paneId="workspace"
          paneTitle="Workspace"
          collapsible="offcanvas"
          class={[
            'workspace-sidebar bg-sidebar top-9 h-[calc(100dvh-36px)] overflow-y-auto outline-none',
            workspace.settings.paneOutline &&
              'data-[pane-active=true]:ring-1 data-[pane-active=true]:ring-ring/40 data-[pane-active=true]:ring-inset',
          ]
            .filter(Boolean)
            .join(' ')}
          aria-label="Workspace"
        >
          <SidebarHeader
            class="sticky top-0 z-1 shrink-0 items-stretch gap-1 bg-sidebar px-3.5 pt-3.5 pb-2.5 text-[11px] font-semibold text-muted-foreground"
          >
            <Row class="items-center justify-between gap-2">
              <span>Workspace</span>
              <Row class="items-center gap-1">
                <Button
                  variant="text"
                  size="sm"
                  class="size-6 p-1 text-[10px] font-medium"
                  aria-label="New file"
                  title="New file"
                  disabled={workspace.cannotCreate}
                  onclick={() => workspace.startCreate('file')}
                  ><FilePlus2 size={14} strokeWidth={1.5} /></Button
                >
                <Button
                  variant="text"
                  size="sm"
                  class="size-6 p-1 text-[10px] font-medium"
                  aria-label="New folder"
                  title="New folder"
                  disabled={workspace.cannotCreate}
                  onclick={() => workspace.startCreate('directory')}
                  ><FolderPlus size={14} strokeWidth={1.5} /></Button
                >
              </Row>
            </Row>
          </SidebarHeader>
          <SidebarContent>
            {#if workspace.creationError}
              {@render sidebarNotice(workspace.creationError, 'alert')}
            {/if}
            {#if workspace.loading}
              {@render sidebarNotice('Opening workspace…', 'status')}
            {:else if workspace.error}
              {@render sidebarNotice(workspace.error, 'alert')}
              {#if client.isDesktop()}<Button
                  variant="text"
                  size="sm"
                  onclick={() => workspace.open()}>Retry</Button
                >{/if}
            {:else if !workspace.entries.length && !workspace.pendingEntry}
              {@render sidebarNotice(
                'Your workspace is empty. Create your first file with New file.',
              )}
            {:else}
              {#key workspace.treeVersion}<WorkspaceExplorer {workspace} />{/key}
            {/if}
          </SidebarContent>
        </Sidebar>
        <ContentPane {workspace} {hotkeys} />
      </Row>
    </SidebarProvider>
    <CommandMenu hotkey={false} showShortcuts />
    <CommandPalette hotkey={false} />
  </CommandProvider>
  <WhichKey />
</WhichKeyProvider>
