<script lang="ts">
  import { onMount } from "svelte";
  import {
    execute,
    type State,
    type Review,
    type Actor,
    type Request,
    type ChangedFile,
  } from "@lgtm/client";
  import DiffView from "@lgtm/ui/DiffView.svelte";
  import ThreadCard from "@lgtm/ui/ThreadCard.svelte";
  import ActorBadge from "@lgtm/ui/ActorBadge.svelte";
  import "@lgtm/ui/review.css";
  let workspace = $state<State>({ reviews: [], comments: [] });
  let choosing = $state(true);
  let selected = $state("");
  let path = $state("");
  let files = $state<ChangedFile[]>([]);
  let text = $state("");
  let source = $state(false);
  let allFiles = $state(false);
  let search = $state("");
  let repo = $state("");
  let pulls = $state<{ number: number; title: string; draft: boolean }[]>([]);
  let pullError = $state("");
  let loadingPulls = $state(false);
  let error = $state("");
  let busy = $state(false);
  let checkout = $state("Local checkout");
  let actor = $state<Actor>({ name: "me", kind: "human" });
  let identityEditing = $state(false);
  let onlyOpen = $state(false);
  let summary = $state("");
  let finishDialog: HTMLDialogElement;
  let historyDialog: HTMLDialogElement;
  let history = $state("");
  const review = $derived(workspace.reviews.find((r) => r.id === selected));
  const roots = $derived(
    workspace.comments.filter(
      (c) =>
        !c.threadId &&
        c.path === path &&
        (c.reviewId === selected || !c.reviewId),
    ),
  );
  const visibleThreads = $derived(
    roots.filter((c) => !onlyOpen || !c.resolved),
  );
  const fileComments = $derived(
    workspace.comments.filter(
      (c) => c.path === path && (c.reviewId === selected || !c.reviewId),
    ),
  );
  const editable = $derived(!review || review.status === "draft");
  let selectionEpoch = 0;
  async function run(q: Request) {
    busy = true;
    error = "";
    try {
      const out = await execute(q);
      workspace = out.state;
      return out;
    } catch (e) {
      error = String(e);
      return undefined;
    } finally {
      busy = false;
    }
  }
  async function loadFile(next: string, asSource = source) {
    const epoch = ++selectionEpoch;
    path = next;
    source = asSource;
    const out = await run({
      op: asSource ? "file" : "diff",
      path: next,
      id: selected,
    });
    if (epoch === selectionEpoch) text = out?.text || "";
  }
  async function reload() {
    const out = await run({
      op: "files",
      id: selected,
      kind: allFiles ? "all" : "",
    });
    files = out?.files || [];
    const next = files.some((f) => f.path === path) ? path : files[0]?.path;
    if (next) await loadFile(next);
    else {
      text = "";
      path = "";
    }
  }
  async function enter(id: string) {
    selected = id;
    choosing = false;
    source = false;
    allFiles = false;
    await reload();
  }
  async function create(kind: "local" | "github", pr?: number) {
    const out = await run({ op: "create", kind, repo, pr, actor });
    if (out) await enter(out.state.reviews.at(-1)!.id);
  }
  async function comment(
    line: number,
    side: string,
    body: string,
    standalone: boolean,
  ) {
    return !!(await run({
      op: "comment",
      path,
      line,
      side,
      body,
      actor,
      reviewId: standalone ? "" : selected,
    }));
  }
  async function reply(id: string, body: string) {
    return !!(await run({ op: "reply", id, body, actor }));
  }
  function resolve(id: string, reopen: boolean) {
    void run({ op: "resolve", id, event: reopen ? "REOPEN" : "RESOLVE" });
  }
  function promote(id: string, reviewId: string) {
    void run({ op: "promote", id, reviewId });
  }
  async function finish(event: string) {
    if (!review) return;
    if (
      review.kind === "github" &&
      !confirm(`Submit ${event} to ${review.repo} #${review.pr}?`)
    )
      return;
    if (await run({ op: "edit", id: selected, body: summary })) {
      if (await run({ op: "submit", id: selected, event }))
        finishDialog.close();
    }
  }
  function saveIdentity() {
    actor = { name: actor.name.trim() || "me", kind: actor.kind };
    localStorage.setItem("lgtm.actor", JSON.stringify(actor));
    identityEditing = false;
  }
  function jump(line: number, side: string) {
    document
      .querySelector(`.diff-scroll [data-anchor="${side}:${line}"]`)
      ?.scrollIntoView({ block: "center", behavior: "smooth" });
  }
  async function loadPulls() {
    loadingPulls = true; pullError = "";
    try {
      const out = await execute({ op: "pulls" });
      repo = out.repository || ""; pulls = out.pulls || [];
    } catch (e) { pullError = String(e instanceof Error ? e.message : e); }
    finally { loadingPulls = false; }
  }
  onMount(() => {
    const saved = localStorage.getItem("lgtm.actor");
    try {
      if (saved) actor = JSON.parse(saved);
    } catch {}
    void loadPulls();
    void run({ op: "state" }).then((out) => {
      if (out?.repoPath) checkout = out.repoPath;
    });
  });
</script>

<svelte:head><title>LGTM · Code review</title></svelte:head>
{#if choosing}
  <main class="start-screen">
    <div class="start-content">
      <header class="start-heading">
        <span class="logo">lgtm</span>
        <h1>Choose a review</h1>
        <p class="muted checkout-path">{checkout}</p>
      </header>
      {#if error}<div class="error" role="alert">{error}</div>{/if}
      <div class="review-choices">
        <section class="choice-card">
          <h2>Local worktree</h2>
          <p>Review staged, unstaged, and new files in this checkout.</p>
          <button
            class="primary"
            disabled={busy}
            onclick={() => create("local")}>Review worktree</button
          ><button class="ghost" disabled={busy} onclick={() => enter("")}
            >Browse standalone notes</button
          >
        </section>
        <section class="choice-card">
          <h2>Open pull requests</h2>
          <p>{repo || "GitHub repository from this checkout’s origin"}</p>
          <button class="ghost" disabled={loadingPulls} onclick={loadPulls}>Refresh pull requests</button>
          {#if loadingPulls}<p class="muted">Loading open pull requests…</p>
          {:else if pullError}<p role="status" class="muted">{pullError}</p>
          {:else if !pulls.length}<p class="muted">No open pull requests.</p>
          {:else}{#each pulls as pull}<button disabled={busy} onclick={() => create("github", pull.number)}>#{pull.number} · {pull.title}{pull.draft ? " · draft" : ""}</button>{/each}{/if}
        </section>
      </div>
      {#if workspace.reviews.length}<section class="recent-reviews">
          <h2>Continue a review</h2>
          {#each [...workspace.reviews].reverse() as r}<button
              onclick={() => enter(r.id)}
              ><span
                >{r.kind === "local"
                  ? "Local worktree"
                  : `${r.repo} #${r.pr}`}</span
              ><ActorBadge actor={r.actor} /><span class="muted"
                >{r.status === "draft"
                  ? "Draft"
                  : r.status.toLowerCase().replace("_", " ")}</span
              ></button
            >{/each}
        </section>{/if}
    </div>
  </main>
{:else}
  <div class="review-app">
    <header class="app-toolbar">
      <button class="logo" onclick={() => (choosing = true)}>lgtm</button><span
        class="toolbar-separator"
      ></span><button class="ghost" onclick={() => (choosing = true)}
        >‹ Reviews</button
      ><span class="review-title"
        >{review?.kind === "github"
          ? `${review.repo} #${review.pr}`
          : selected
            ? "Local worktree"
            : "Checkout notes"}</span
      ><span class="review-state">{review?.status || "standalone"}</span>
      <div class="toolbar-spacer"></div>
      {#if review?.kind === "github"}<button
          class="ghost"
          disabled={busy}
          onclick={async () => {
            history = (await run({ op: "history", id: selected }))?.text || "";
            historyDialog.showModal();
          }}>GitHub discussion</button
        >{/if}<button class="ghost" disabled={busy} onclick={reload}
        >Reload</button
      >{#if review}<button
          class="primary"
          disabled={busy || !editable}
          onclick={() => {
            summary = review?.body || "";
            finishDialog.showModal();
          }}>Finish review</button
        >{/if}
    </header>
    {#if error}<div class="error" role="alert">{error}</div>{/if}
    <div class="review-layout">
      <aside class="files-pane">
        <header class="section-header">
          <span>Changed files</span><span>{files.length}</span>
        </header>
        <div class="file-tools">
          <input
            aria-label="Filter files"
            placeholder="Filter files…"
            bind:value={search}
          />{#if review?.kind !== "github"}<select
              aria-label="File scope"
              bind:value={allFiles}
              onchange={() => reload()}
              ><option value={false}>Changed files</option><option value={true}
                >All checkout files</option
              ></select
            >{/if}
        </div>
        <nav class="file-list" aria-label="Files">
          {#each files.filter((f) => f.path
              .toLowerCase()
              .includes(search.toLowerCase())) as f}<button
              class="file-item"
              class:active={path === f.path}
              onclick={() => loadFile(f.path, false)}
              title={f.path}
              ><span
                class="file-status"
                class:deleted={f.status === "deleted"}
                class:modified={f.status === "modified"}>{f.status}</span
              ><span class="file-path">{f.path}</span
              >{#if workspace.comments.filter((c) => !c.threadId && c.path === f.path && !c.resolved && (c.reviewId === selected || !c.reviewId)).length}<span
                  class="count"
                  >{workspace.comments.filter(
                    (c) =>
                      !c.threadId &&
                      c.path === f.path &&
                      !c.resolved &&
                      (c.reviewId === selected || !c.reviewId),
                  ).length}</span
                >{/if}</button
            >{/each}{#if !files.length}<div class="empty">
              No changed files
            </div>{/if}
        </nav>
        <form
          class="open-file"
          onsubmit={(e) => {
            e.preventDefault();
            const data = new FormData(e.currentTarget);
            void loadFile(String(data.get("path")), true);
          }}
        >
          <input
            aria-label="Open checkout file"
            name="path"
            placeholder="Open any file by path…"
          /><button class="ghost">Open</button>
        </form>
      </aside>
      <main class="diff-pane">
        <header class="diff-header">
          <span class="selected-path">{path || "Select a file"}</span
          >{#if busy}<span class="muted">Loading…</span>{/if}
          <div class="toolbar-spacer"></div>
          <div class="segmented">
            <button
              class:active={!source}
              disabled={!path || busy}
              onclick={() => loadFile(path, false)}>Diff</button
            ><button
              class:active={source}
              disabled={!path || busy}
              onclick={() => loadFile(path, true)}>File</button
            >
          </div>
        </header>
        {#if path}<DiffView
            {text}
            {path}
            {source}
            comments={fileComments}
            {actor}
            reviewId={selected}
            {busy}
            {editable}
            reviews={workspace.reviews}
            oncomment={comment}
            onreply={reply}
            onresolve={resolve}
            onpromote={promote}
          />{:else}<div class="empty">
            Select a file to review its changes.
          </div>{/if}
      </main>
      <aside class="threads-pane">
        <header class="section-header">
          <span>Threads · {visibleThreads.length}</span><label
            ><input type="checkbox" bind:checked={onlyOpen} /> open only</label
          >
        </header>
        <div class="identity">
          {#if identityEditing}<form
              onsubmit={(e) => {
                e.preventDefault();
                saveIdentity();
              }}
            >
              <input aria-label="Author name" bind:value={actor.name} /><select
                aria-label="Author type"
                bind:value={actor.kind}
                ><option value="human">User</option><option value="agent"
                  >Agent</option
                ></select
              ><button class="primary">Save</button>
            </form>{:else}<span>Commenting as <ActorBadge {actor} /></span
            ><button class="ghost" onclick={() => (identityEditing = true)}
              >Change</button
            >{/if}
        </div>
        <div class="threads-list">
          {#each visibleThreads as thread}<ThreadCard
              {thread}
              replies={fileComments.filter((c) => c.threadId === thread.id)}
              {busy}
              {editable}
              reviews={workspace.reviews}
              onreply={reply}
              onresolve={resolve}
              onpromote={promote}
              onjump={() => jump(thread.line, thread.side)}
            />{/each}{#if !visibleThreads.length}<div class="empty">
              No threads for this file.<br />Use + beside a line to comment.
            </div>{/if}
        </div>
      </aside>
    </div>
    <footer class="statusbar">
      <span
        >{checkout.split("/").at(-1)} · {review?.kind === "github"
          ? "GitHub PR"
          : "HEAD ↔ worktree"}</span
      ><span>{path ? "Hover a line to comment · ⌘ Enter to save" : ""}</span
      ><span>{actor.name} · {actor.kind}</span>
    </footer>
  </div>
{/if}
<dialog class="review-dialog" bind:this={finishDialog}>
  <h2>Finish review</h2>
  {#if review}<p class="muted">
      {review.kind === "github"
        ? `${review.repo} #${review.pr}`
        : "Local worktree"} · <ActorBadge actor={review.actor} />
    </p>{/if}<textarea
    aria-label="Review summary"
    bind:value={summary}
    placeholder="Summarize your review…"></textarea>{#if error}<p class="error">
      {error}
    </p>{/if}
  <div class="dialog-actions">
    <button class="ghost" onclick={() => finishDialog.close()}>Cancel</button
    ><button
      class="ghost"
      disabled={busy}
      onclick={async () => {
        if (await run({ op: "edit", id: selected, body: summary }))
          finishDialog.close();
      }}>Save draft</button
    ><button class="ghost" disabled={busy} onclick={() => finish("COMMENT")}
      >Submit review</button
    ><button
      class="danger"
      disabled={busy || !summary.trim()}
      onclick={() => finish("REQUEST_CHANGES")}>Request changes</button
    ><button class="primary" disabled={busy} onclick={() => finish("APPROVE")}
      >Approve</button
    >
  </div>
</dialog>
<dialog class="review-dialog" bind:this={historyDialog}>
  <h2>GitHub discussion</h2>
  <pre>{history || "No reviews or inline comments yet."}</pre>
  <button class="ghost" onclick={() => historyDialog.close()}>Close</button>
</dialog>
