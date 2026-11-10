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
    SidebarFooter,
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
  import {
    Box,
    FilePlus2,
    FolderPlus,
    FolderTree,
    GitBranch,
    PanelLeft,
    ChevronDown,
  } from 'lucide-svelte'
  import { ThemeService } from 'lgtm-ui'
  import {
    errorMessage,
    workspaceClient,
    type SidebarWidget,
    type WorkspaceClient,
  } from '../clients/workspaceClient'
  import { HotkeyService } from '../services/hotkeyService.svelte'
  import { WorkspaceService } from '../services/workspaceService.svelte'
  import WorkspaceExplorer from './WorkspaceExplorer.svelte'
  import ProjectsPane from './ProjectsPane.svelte'
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
    const unlisteners: (() => void)[] = [theme.init()]
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

{#snippet sidebarNotice(text: string, role?: 'alert' | 'status')}
  <p
    class={['mx-3.5 my-2 text-xs leading-relaxed wrap-anywhere', role === 'alert' && 'text-warn']}
    {role}
  >
    {text}
  </p>
{/snippet}

{#snippet sidebarModeButton(mode: SidebarWidget, label: string)}
  {@const active = mode === 'projects' ? workspace.projectsOpen : workspace.explorerOpen}
  <button
    type="button"
    aria-label="{label} view"
    title={label}
    aria-pressed={active}
    class={[
      'inline-flex size-6 items-center justify-center rounded-none transition-colors outline-none focus-visible:ring-1 focus-visible:ring-ring',
      active
        ? 'bg-accent text-accent-foreground'
        : 'text-muted-foreground hover:bg-accent/50 hover:text-foreground',
    ].join(' ')}
    onclick={() => {
      if (mode === 'projects') workspace.toggleProjectsPane()
      else workspace.toggleExplorerPane()
    }}
  >
    {#if mode === 'projects'}
      <Box size={13} strokeWidth={1.5} />
    {:else}
      <FolderTree size={13} strokeWidth={1.5} />
    {/if}
  </button>
{/snippet}

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
      <header
        class="titlebar flex h-9 shrink-0 items-center gap-4 border-b border-border bg-sidebar pr-3 pl-20 text-sidebar-foreground select-none"
        data-tauri-drag-region
      >
        <SidebarTrigger
          title="Toggle sidebar"
          aria-controls="workspace-sidebar"
          onclick={() => workspace.toggleExplorerPane()}
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
          collapsible="offcanvas"
          class="workspace-sidebar top-9 h-[calc(100dvh-36px)] overflow-hidden bg-sidebar outline-none"
          aria-label="Workspace"
        >
          <Row class="min-h-0 flex-1 items-stretch gap-0 overflow-hidden">
            <Sidebar
              id="projects-dock-pane"
              data-testid="projects-dock-pane"
              paneId="projects"
              paneTitle="Projects"
              paneDisabled={!workspace.projectsOpen}
              collapsible="none"
              hidden={!workspace.projectsOpen}
              inert={!workspace.projectsOpen ? true : undefined}
              class={[
                'h-full min-h-0 w-45 shrink-0 overflow-hidden bg-sidebar outline-none',
                workspace.projectsOpen ? '' : 'hidden',
                workspace.projectsOpen && workspace.explorerOpen ? 'border-r border-border' : '',
              ].join(' ')}
              aria-label="Projects"
            >
              <SidebarHeader
                class="sticky top-0 z-1 shrink-0 items-stretch gap-1 bg-sidebar px-3 pt-3.5 pb-2.5 text-[11px] font-semibold text-muted-foreground"
              >
                <Row class="items-center justify-between gap-1.5">
                  <span>Projects</span>
                  <Row class="items-center gap-0.5">
                    <Button
                      variant="text"
                      size="sm"
                      class="size-6 p-1 text-[10px] font-medium"
                      aria-label="Add project"
                      title="Add project"
                      disabled={workspace.blocked}
                      onclick={() => void workspace.selectWorkspace('__open_folder__')}
                      ><FolderPlus size={14} strokeWidth={1.5} /></Button
                    >
                    <Button
                      variant="text"
                      size="sm"
                      class="size-6 p-1 text-[10px] font-medium"
                      aria-label="New worktree"
                      title="New worktree"
                      disabled={!workspace.activeProject || workspace.blocked}
                      onclick={() => workspace.openNewWorktreeDialog()}
                      ><GitBranch size={14} strokeWidth={1.5} /></Button
                    >
                  </Row>
                </Row>
              </SidebarHeader>
              <SidebarContent class="min-h-0 flex-1 overflow-y-auto">
                {#if workspace.creationError && !workspace.explorerOpen}
                  {@render sidebarNotice(workspace.creationError, 'alert')}
                {/if}
                <ProjectsPane {workspace} />
              </SidebarContent>
            </Sidebar>

            <Sidebar
              id="explorer-dock-pane"
              data-testid="explorer-dock-pane"
              paneId="workspace"
              paneTitle="Workspace"
              paneDisabled={!workspace.explorerOpen}
              collapsible="none"
              hidden={!workspace.explorerOpen}
              inert={!workspace.explorerOpen ? true : undefined}
              class={[
                'h-full min-h-0 w-59 shrink-0 overflow-hidden bg-sidebar outline-none',
                workspace.explorerOpen ? '' : 'hidden',
              ].join(' ')}
              aria-label="Workspace Explorer"
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
              <SidebarContent class="min-h-0 flex-1 overflow-y-auto">
                {#if workspace.creationError}
                  {@render sidebarNotice(workspace.creationError, 'alert')}
                {/if}
                {#if workspace.loading && !workspace.entries.length}
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
                {/if}
                <div
                  hidden={Boolean(workspace.error) ||
                    (!workspace.entries.length && !workspace.pendingEntry)}
                  class={Boolean(workspace.error) ||
                  (!workspace.entries.length && !workspace.pendingEntry)
                    ? 'hidden'
                    : ''}
                >
                  <WorkspaceExplorer {workspace} />
                </div>
              </SidebarContent>
            </Sidebar>
          </Row>
          <SidebarFooter
            class="shrink-0 flex-row items-center justify-start gap-1 border-t border-border bg-sidebar px-2.5 py-1.5"
          >
            <div class="flex items-center gap-1" role="group" aria-label="Sidebar view">
              {@render sidebarModeButton('projects', 'Projects')}
              {@render sidebarModeButton('explorer', 'Explorer')}
            </div>
          </SidebarFooter>
        </Sidebar>
        <ContentPane {workspace} />
      </Row>
    </SidebarProvider>
    <CommandMenu hotkey={false} showShortcuts />
    <CommandPalette hotkey={false} />
  </CommandProvider>
  <WhichKey />
</WhichKeyProvider>
