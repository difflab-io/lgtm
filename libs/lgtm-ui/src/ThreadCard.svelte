<script lang="ts">
  import type { Comment, Review } from "@lgtm/client";
  import ActorBadge from "./ActorBadge.svelte";
  let {
    thread,
    replies = [],
    reviews = [],
    editable = true,
    busy = false,
    onreply,
    onresolve,
    onpromote,
    onjump,
  }: {
    thread: Comment;
    replies?: Comment[];
    reviews?: Review[];
    editable?: boolean;
    busy?: boolean;
    onreply: (id: string, body: string) => Promise<boolean>;
    onresolve: (id: string, reopen: boolean) => void;
    onpromote: (id: string, reviewId: string) => void;
    onjump?: () => void;
  } = $props();
  let replying = $state(false);
  let body = $state("");
  async function reply() {
    if (await onreply(thread.id, body)) {
      body = "";
      replying = false;
    }
  }
</script>

<article
  class="thread-card"
  class:resolved={thread.resolved}
  data-thread={thread.id}
>
  <header class="thread-header">
    <button class="anchor-link" onclick={onjump}
      >{thread.path}:{thread.line}
      <span>{thread.side === "LEFT" ? "old" : ""}</span></button
    ><span class="thread-status"
      >{thread.resolved ? "resolved" : !thread.reviewId ? "note" : "open"}</span
    >
  </header>
  {#each [thread, ...replies] as c}<div class="comment">
      <ActorBadge actor={c.actor} />
      <p>{c.body}</p>
    </div>{/each}
  <div class="thread-actions">
    <button
      class="ghost"
      onclick={() => onresolve(thread.id, !!thread.resolved)}
      >{thread.resolved ? "Reopen" : "Resolve"}</button
    >{#if editable}<button class="ghost" onclick={() => (replying = !replying)}
        >Reply</button
      >{/if}
    {#if !thread.reviewId && reviews.some((r) => r.status === "draft")}<select
        aria-label="Promote comment"
        disabled={busy}
        onchange={(e) => {
          if (e.currentTarget.value)
            onpromote(thread.id, e.currentTarget.value);
        }}
        ><option value="">Promote to review…</option
        >{#each reviews.filter((r) => r.status === "draft") as r}<option
            value={r.id}
            >{r.kind === "local" ? "Worktree" : `${r.repo} #${r.pr}`} · {r.id.slice(
              0,
              6,
            )}</option
          >{/each}</select
      >{/if}
  </div>
  {#if replying}<form
      onsubmit={(e) => {
        e.preventDefault();
        void reply();
      }}
    >
      <textarea
        aria-label="Reply body"
        bind:value={body}
        placeholder="Reply to this thread…"
        required
        onkeydown={(e) => {
          if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
            e.preventDefault();
            void reply();
          }
        }}></textarea>
      <div class="composer-actions">
        <button class="ghost" type="button" onclick={() => (replying = false)}
          >Cancel</button
        ><button class="primary" disabled={busy || !body.trim()}
          >Post reply</button
        >
      </div>
    </form>{/if}
</article>
