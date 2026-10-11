<script lang="ts">
  import { diffRows } from "./diff";
  import { highlight } from "./highlight";
  import ThreadCard from "./ThreadCard.svelte";
  import LineComposer from "./LineComposer.svelte";
  import type { Actor, Comment, Review } from "@lgtm/client";
  import type { ThemedToken } from "shiki";
  let {
    text,
    path,
    source = false,
    comments,
    actor,
    reviewId,
    busy,
    editable,
    reviews,
    oncomment,
    onreply,
    onresolve,
    onpromote,
  }: {
    text: string;
    path: string;
    source?: boolean;
    comments: Comment[];
    actor: Actor;
    reviewId: string;
    busy: boolean;
    editable: boolean;
    reviews: Review[];
    oncomment: (
      line: number,
      side: string,
      body: string,
      standalone: boolean,
    ) => Promise<boolean>;
    onreply: (id: string, body: string) => Promise<boolean>;
    onresolve: (id: string, reopen: boolean) => void;
    onpromote: (id: string, reviewId: string) => void;
  } = $props();
  const rows = $derived(diffRows(text, source));
  let tokens = $state<ThemedToken[][]>([]);
  let composing = $state<string | null>(null);
  $effect(() => {
    const current = rows;
    const file = path;
    composing = null;
    tokens = [];
    let cancelled = false;
    void highlight(
      current.map((r) => r.text),
      file,
    )
      .then((t) => {
        if (!cancelled) tokens = t;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });
  const roots = $derived(comments.filter((c) => !c.threadId));
</script>

<div class="diff-scroll" data-testid="diff-body">
  {#if !text}<div class="empty">
      {source
        ? "This file is empty."
        : "No changes in this file. Use File to view and comment on its source."}
    </div>{/if}
  {#each rows as row, i}
    <div
      class="diff-row"
      class:added={row.kind === "added"}
      class:removed={row.kind === "removed"}
      class:hunk={row.kind === "hunk"}
      class:meta={row.kind === "header"}
      data-anchor={`${row.side}:${row.line}`}
    >
      <span class="line-number">{row.oldLine || ""}</span><span
        class="line-number">{row.newLine || ""}</span
      ><span class="marker"
        >{row.kind === "added" ? "+" : row.kind === "removed" ? "-" : " "}</span
      >
      {#if row.line && editable}<button
          class="add-comment"
          aria-label={`Comment on ${row.side === "LEFT" ? "old" : "new"} line ${row.line}`}
          onclick={() => (composing = `${row.side}:${row.line}`)}>+</button
        >{:else}<span></span>{/if}
      <code
        >{#if tokens[i]}{#each tokens[i] as token}<span
              style:color={token.color}>{token.content}</span
            >{/each}{:else}{row.text}{/if}</code
      >
    </div>
    {#if composing === `${row.side}:${row.line}`}<div class="inline-thread">
        <LineComposer
          {path}
          line={row.line}
          side={row.side}
          {actor}
          {busy}
          hasReview={!!reviewId}
          onsave={(body, standalone) =>
            oncomment(row.line, row.side, body, standalone)}
          oncancel={() => (composing = null)}
        />
      </div>{/if}
    {#each roots.filter((c) => c.line === row.line && c.side === row.side) as thread}<div
        class="inline-thread"
      >
        <ThreadCard
          {thread}
          replies={comments.filter((c) => c.threadId === thread.id)}
          {busy}
          {editable}
          {reviews}
          {onreply}
          {onresolve}
          {onpromote}
        />
      </div>{/each}
  {/each}
</div>
