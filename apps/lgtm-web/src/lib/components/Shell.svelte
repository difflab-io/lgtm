<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import {
    Button,
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
  } from '@cloudvoyant/helical-svelte'
  import { FilePlus2, FolderPlus, PanelLeft, ChevronDown } from 'lucide-svelte'
  import { ThemeService, ThemeToggle } from 'lgtm-ui'
  import { errorMessage, workspaceClient, type WorkspaceClient } from '../clients/workspaceClient'
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
  const theme = untrack(
    () =>
      new ThemeService({
        storageKey: 'lgtm.theme',
        themeClass: 'theme-lgtm',
        windowBridge: client.themeWindowBridge?.(),
      }),
  )
  workspace.setThemeHandler((nextTheme) => {
    if (theme.preference !== nextTheme) {
      theme.setPreference(nextTheme)
    }
  })

  // Lifecycle -----------------------------------------------------------------
  onMount(() => {
    let active = true
    const handleKeydown = (event: KeyboardEvent) => {
      if (
        (event.metaKey || event.ctrlKey) &&
        !event.shiftKey &&
        !event.altKey &&
        event.key === ','
      ) {
        event.preventDefault()
        void workspace.openSettingsTab('global')
      }
    }
    window.addEventListener('keydown', handleKeydown)
    const unlisteners: (() => void)[] = [
      theme.init(),
      () => window.removeEventListener('keydown', handleKeydown),
    ]
    void workspace.open()
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
          onConfigUpdated: (snapshot) => workspace.handleConfigUpdated(snapshot),
        })
        .then((stops) => {
          for (const stop of stops) {
            if (active) unlisteners.push(stop)
            else stop()
          }
          if (active) void workspace.syncLspState()
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

<SidebarProvider
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
    <div class="ml-auto flex items-center gap-1">
      <ThemeToggle
        value={theme.preference}
        onchange={(next) => {
          theme.setPreference(next)
          void workspace.setThemePreference(next)
        }}
      />
    </div>
  </header>
  <Row class="min-h-0 flex-1 items-stretch gap-0">
    <Sidebar
      id="workspace-sidebar"
      collapsible="offcanvas"
      class="workspace-sidebar bg-sidebar top-9 h-[calc(100dvh-36px)] overflow-y-auto"
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
          {#if client.isDesktop()}<Button variant="text" size="sm" onclick={() => workspace.open()}
              >Retry</Button
            >{/if}
        {:else if !workspace.entries.length && !workspace.pendingEntry}
          {@render sidebarNotice('Your workspace is empty. Create your first file with New file.')}
        {:else}
          {#key workspace.treeVersion}<WorkspaceExplorer {workspace} />{/key}
        {/if}
      </SidebarContent>
    </Sidebar>
    <ContentPane {workspace} />
  </Row>
</SidebarProvider>
