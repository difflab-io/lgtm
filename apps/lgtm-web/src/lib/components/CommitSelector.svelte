<script lang="ts">
  import type { DiffService } from '../services/diffService.svelte'

  // Props ---------------------------------------------------------------------
  interface CommitSelectorProps {
    diff: DiffService
  }
  let { diff }: CommitSelectorProps = $props()
</script>

<section
  class="commit-selector flex h-44 shrink-0 flex-col overflow-y-auto border-t border-border bg-sidebar"
  aria-label="Commits checklist"
>
  <div
    class="sticky top-0 z-1 flex items-center justify-between gap-2 border-b border-border/60 bg-sidebar px-3.5 py-1.5 text-[10px] font-semibold tracking-wide text-muted-foreground uppercase"
  >
    <span>Commits ({diff.selectedCommitIds.length}/{diff.commits.length})</span>
    {#if diff.branch}
      <span class="truncate font-mono text-[10px] font-normal lowercase">{diff.branch}</span>
    {/if}
  </div>
  {#if diff.commits.length === 0}
    <p class="px-3.5 py-2 text-xs text-muted-foreground">No commits available.</p>
  {:else}
    <ul class="flex flex-col py-0.5" role="list" aria-label="Commit range checklist">
      {#each diff.commits as commit (commit.id)}
        {@const checked = diff.selectedCommitIds.includes(commit.id)}
        <li>
          <label
            class={[
              'flex w-full cursor-pointer items-center gap-2 px-3.5 py-1 text-left text-[11px] select-none hover:bg-accent hover:text-accent-foreground',
              checked && 'bg-accent/60 font-medium text-foreground',
            ]}
            data-testid={`commit-item-${commit.shortSha}`}
          >
            <input
              type="checkbox"
              class="size-3 shrink-0 accent-primary"
              {checked}
              aria-label={`${commit.shortSha} ${commit.title}`}
              onchange={() => void diff.toggleCommit(commit.id)}
            />
            <span class="shrink-0 font-mono text-[10px] text-muted-foreground"
              >{commit.shortSha}</span
            >
            <span class="min-w-0 flex-1 truncate">{commit.title}</span>
          </label>
        </li>
      {/each}
    </ul>
  {/if}
</section>
