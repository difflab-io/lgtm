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
  import { ChevronDown, GitBranch, PanelLeft, Plus } from 'lucide-svelte'
  import { useService } from '../services/context'

  // State ---------------------------------------------------------------------
  const workspace = useService('workspace')
  const project = useService('project')
  const session = useService('session')
</script>

<header
  class="titlebar flex h-9 shrink-0 items-center gap-2 border-b border-border bg-sidebar pr-3 pl-20 text-sidebar-foreground select-none"
  data-tauri-drag-region
>
  <SidebarTrigger
    title="Toggle sidebar"
    aria-controls="workspace-sidebar"
    onclick={() => session.toggleExplorerPane()}
    ><PanelLeft size={14} strokeWidth={1.5} /></SidebarTrigger
  >
  <span class="brand text-xs font-semibold tracking-tight" data-tauri-drag-region>lgtm</span>
  <div class="workspace-chooser flex min-w-0 items-center gap-0.5">
    {#if project.activeBranch || project.branches.length > 0 || project.activeProject?.gitCommonDir}
      <Select
        items={project.branchItems}
        value={project.activeBranch ? [project.activeBranch] : []}
        size="sm"
        class="w-auto min-w-0"
        onValueChange={({ value }) => {
          if (value[0] && !workspace.blocked) void project.selectBranch(value[0])
        }}
      >
        <SelectTrigger
          aria-label="Branch selector"
          title={project.activeBranch ? `Branch: ${project.activeBranch}` : 'Select branch'}
          size="sm"
          class="h-6 w-auto max-w-40 min-w-0 gap-1 rounded-sm border-0 bg-transparent px-1.5 py-0 text-[11px] shadow-none hover:bg-accent hover:text-accent-foreground [&_svg]:size-3"
        >
          <GitBranch size={12} strokeWidth={1.5} class="shrink-0 text-muted-foreground" />
          <SelectValue class="gap-1 truncate" placeholder={project.activeBranch ?? 'HEAD'} />
          <SelectIndicator><ChevronDown size={11} strokeWidth={1.5} /></SelectIndicator>
        </SelectTrigger>
        <SelectContent class="w-max max-w-56 min-w-32 p-0.5">
          {#each project.branchItems as item (item.value)}
            <SelectItem
              {item}
              class="min-h-6 gap-1.5 rounded-none px-2 py-0.5 pe-2.5 text-[11px] leading-tight [&_svg]:size-3"
            >
              {#if item.value === '__new_branch__'}
                <Plus size={12} strokeWidth={1.5} class="shrink-0 text-muted-foreground" />
              {:else}
                <GitBranch size={12} strokeWidth={1.5} class="shrink-0 text-muted-foreground" />
              {/if}
              <SelectItemText class="gap-1.5 truncate">{item.label}</SelectItemText>
            </SelectItem>
          {/each}
        </SelectContent>
      </Select>
    {/if}
  </div>
</header>
