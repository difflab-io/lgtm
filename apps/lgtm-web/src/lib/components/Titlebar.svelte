<script lang="ts">
  import {
    Select,
    SelectContent,
    SelectIndicator,
    SelectItem,
    SelectItemText,
    SelectTrigger,
    SelectValue,
    SidebarTrigger,
  } from '@cloudvoyant/helical-svelte'
  import { ChevronDown, PanelLeft } from 'lucide-svelte'
  import type { WorkspaceService } from '../services/workspaceService.svelte'

  // Props ---------------------------------------------------------------------
  interface TitlebarProps {
    workspace: WorkspaceService
  }
  let { workspace }: TitlebarProps = $props()
</script>

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
