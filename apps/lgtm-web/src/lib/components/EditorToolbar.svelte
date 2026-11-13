<script lang="ts">
  import { WhichKeyMode } from '@cloudvoyant/helical-svelte'
  import { ChevronRight } from 'lucide-svelte'
  import { useService } from '../services/context'
  import LspStatusMenu from './LspStatusMenu.svelte'

  // State ---------------------------------------------------------------------
  const workspace = useService('workspace')
</script>

<div
  data-testid="editor-toolbar"
  class="relative flex h-7 min-w-0 flex-1 items-center justify-between gap-4 bg-sidebar px-4 py-0 text-[10px] text-muted-foreground"
  role="status"
  aria-live="polite"
>
  <div class="flex min-w-0 items-center gap-2.5">
    <LspStatusMenu {workspace} />
    <nav
      aria-label="Breadcrumb"
      class="document-path flex min-w-0 items-center gap-1 truncate"
      title={workspace.selectedPath}
    >
      {#if workspace.breadcrumbs.length > 0}
        {#each workspace.breadcrumbs as segment, idx (idx)}
          {#if idx > 0}
            <ChevronRight size={11} strokeWidth={1.5} class="shrink-0 opacity-60" />
          {/if}
          <span
            class={idx === workspace.breadcrumbs.length - 1
              ? 'truncate text-foreground'
              : 'truncate'}
          >
            {segment}
          </span>
        {/each}
      {:else}
        <span class="truncate">No file selected</span>
      {/if}
    </nav>
  </div>
  <div class="flex shrink-0 items-center gap-2.5">
    <WhichKeyMode showPreset aria-label="Editor mode" />
    <span class={workspace.openingError ? 'shrink-0 text-warn' : 'shrink-0'}
      >{workspace.status}</span
    >
  </div>
</div>
