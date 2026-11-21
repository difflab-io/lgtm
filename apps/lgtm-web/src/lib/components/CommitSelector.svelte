<script lang="ts">
  import {
    Checkbox,
    CheckboxControl,
    CheckboxIndicator,
    CheckboxLabel,
  } from '@cloudvoyant/helical-svelte'
  import { Check } from 'lucide-svelte'
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
          <Checkbox
            size="sm"
            {checked}
            onCheckedChange={() => void diff.toggleCommit(commit.id)}
            class={`w-full cursor-pointer px-3.5 py-1 text-left text-[11px] select-none hover:bg-accent hover:text-accent-foreground ${checked ? 'bg-accent/60 font-medium text-foreground' : ''}`}
            data-testid={`commit-item-${commit.shortSha}`}
          >
            <CheckboxControl
              class="size-3 rounded-none data-[state=checked]:border-primary data-[state=checked]:bg-primary"
            >
              <CheckboxIndicator class="[&_svg]:size-2.5">
                <Check size={10} strokeWidth={2.5} />
              </CheckboxIndicator>
            </CheckboxControl>
            <CheckboxLabel
              class="flex min-w-0 flex-1 items-center gap-2 text-[11px] leading-normal font-normal"
            >
              <span class="shrink-0 font-mono text-[10px] text-muted-foreground"
                >{commit.shortSha}</span
              >
              <span class="min-w-0 flex-1 truncate">{commit.title}</span>
            </CheckboxLabel>
          </Checkbox>
        </li>
      {/each}
    </ul>
  {/if}
</section>
