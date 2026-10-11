<script lang="ts">
  import ActorBadge from "./ActorBadge.svelte";
  import type { Actor } from "@lgtm/client";
  let {
    path,
    line,
    side,
    actor,
    hasReview,
    busy,
    onsave,
    oncancel,
  }: {
    path: string;
    line: number;
    side: string;
    actor: Actor;
    hasReview: boolean;
    busy: boolean;
    onsave: (body: string, standalone: boolean) => Promise<boolean>;
    oncancel: () => void;
  } = $props();
  let body = $state("");
  let standalone = $state(false);
  async function save() {
    if (await onsave(body, standalone)) oncancel();
  }
</script>

<form
  class="thread-card new-thread"
  onsubmit={(e) => {
    e.preventDefault();
    void save();
  }}
>
  <header class="thread-header">
    <span>New thread · {path}:{line} {side === "LEFT" ? "· old line" : ""}</span
    ><ActorBadge {actor} />
  </header>
  <textarea
    aria-label="Comment body"
    bind:value={body}
    placeholder="Leave a review comment…"
    required
    onkeydown={(e) => {
      if (e.key === "Escape") {
        e.preventDefault();
        oncancel();
      }
      if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
        e.preventDefault();
        void save();
      }
    }}></textarea>
  <div class="composer-actions">
    {#if hasReview}<label
        ><input type="checkbox" bind:checked={standalone} /> Standalone note</label
      >{:else}<span class="muted">Standalone note</span>{/if}<span
      class="shortcut">⌘ Enter to save</span
    ><button class="ghost" type="button" onclick={oncancel}>Cancel</button
    ><button class="primary" disabled={busy || !body.trim()}
      >Save comment</button
    >
  </div>
</form>
