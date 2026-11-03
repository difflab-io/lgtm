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

  // Lifecycle -----------------------------------------------------------------
  onMount(() => {
    let active = true
    const unlisteners: (() => void)[] = []
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

<SidebarProvider class="workspace-shell" style="--sidebar-width: 236px">
  <header class="titlebar" data-tauri-drag-region>
    <SidebarTrigger title="Toggle sidebar" aria-controls="workspace-sidebar"
      ><PanelLeft size={14} strokeWidth={1.5} /></SidebarTrigger
    >
    <span class="brand" data-tauri-drag-region>lgtm</span>
    <div class="workspace-chooser">
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
  <Row class="workspace-body">
    <Sidebar
      id="workspace-sidebar"
      collapsible="offcanvas"
      class="workspace-sidebar"
      aria-label="Workspace"
    >
      <SidebarHeader class="sidebar-heading">
        <Row class="workspace-heading-row">
          <span>Workspace</span>
          <Row class="workspace-actions">
            <Button
              variant="text"
              size="sm"
              class="new-file"
              aria-label="New file"
              title="New file"
              disabled={workspace.cannotCreate}
              onclick={() => workspace.startCreate('file')}
              ><FilePlus2 size={14} strokeWidth={1.5} /></Button
            >
            <Button
              variant="text"
              size="sm"
              class="new-file"
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
        {#if workspace.creationError}<p class="sidebar-message error" role="alert">
            {workspace.creationError}
          </p>{/if}
        {#if workspace.loading}
          <p class="sidebar-message" role="status">Opening workspace…</p>
        {:else if workspace.error}
          <p class="sidebar-message error" role="alert">{workspace.error}</p>
          {#if client.isDesktop()}<Button variant="text" size="sm" onclick={() => workspace.open()}
              >Retry</Button
            >{/if}
        {:else if !workspace.entries.length && !workspace.pendingEntry}
          <p class="sidebar-message">
            Your workspace is empty. Create your first file with New file.
          </p>
        {:else}
          {#key workspace.treeVersion}<WorkspaceExplorer {workspace} />{/key}
        {/if}
      </SidebarContent>
    </Sidebar>
    <ContentPane {workspace} />
  </Row>
</SidebarProvider>
