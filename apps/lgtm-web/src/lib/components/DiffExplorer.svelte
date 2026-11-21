<script lang="ts">
  import { TreeView } from 'lgtm-ui'
  import { Folder, FolderOpen, FileText } from 'lucide-svelte'
  import type { DiffFileStatus } from '../clients/workspaceClient'
  import type { DiffService } from '../services/diffService.svelte'

  // Props ---------------------------------------------------------------------
  interface DiffExplorerProps {
    diff: DiffService
  }
  let { diff }: DiffExplorerProps = $props()

  // Helpers -------------------------------------------------------------------
  function statusCode(status: DiffFileStatus): string {
    switch (status) {
      case 'added':
        return 'A'
      case 'modified':
        return 'M'
      case 'deleted':
        return 'D'
      case 'renamed':
        return 'R'
    }
  }

  function statusClass(status: DiffFileStatus): string {
    switch (status) {
      case 'added':
        return 'text-emerald-600 dark:text-emerald-400'
      case 'modified':
        return 'text-amber-600 dark:text-amber-400'
      case 'deleted':
        return 'text-rose-600 dark:text-rose-400'
      case 'renamed':
        return 'text-sky-600 dark:text-sky-400'
    }
  }
</script>

<div class="diff-explorer min-h-0 flex-1 overflow-y-auto" aria-label="Changed files explorer">
  {#if diff.files.length === 0}
    <p class="mx-3.5 my-2 text-xs leading-relaxed text-muted-foreground" role="status">
      No changed files in the selected commit range.
    </p>
  {:else}
    {#key `${diff.expandedFolders.join('|')}:${diff.files.map((f) => f.path).join('|')}`}
      <TreeView
        label="Changed files"
        nodes={diff.treeNodes}
        defaultExpandedValue={diff.expandedFolders}
        selected={diff.activeFilePath}
        onselect={(path) => void diff.selectFile(path)}
      >
        {#snippet icon(node, expanded)}
          {#if node.branch}
            {#if expanded}<FolderOpen size={13} strokeWidth={1.5} />{:else}<Folder
                size={13}
                strokeWidth={1.5}
              />{/if}
          {:else}<FileText size={13} strokeWidth={1.5} />{/if}
        {/snippet}
        {#snippet trailing(node)}
          {@const entry = diff.getFileEntry(node.value)}
          {#if entry}
            <span
              class="ml-auto flex shrink-0 items-center gap-1.5 font-mono text-[10px]"
              data-testid={`diff-file-stats-${entry.path}`}
            >
              <span
                class={['font-semibold', statusClass(entry.status)]}
                data-testid={`diff-file-status-${entry.path}`}>{statusCode(entry.status)}</span
              >
              <span class="text-emerald-600 dark:text-emerald-400">+{entry.additions}</span>
              <span class="text-rose-600 dark:text-rose-400">-{entry.deletions}</span>
            </span>
          {/if}
        {/snippet}
      </TreeView>
    {/key}
  {/if}
</div>
