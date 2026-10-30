# Use a local web app and Neovim plugin for reviews

- Status: accepted
- Deciders: Siddharth Kapoor
- Date: 2026-10-13

Technical Story: [Code and document review PRD](../prd/0001-core-concepts/README.md)

As of the 2026-10-29 reset, LGTM is a hello-review skeleton with CLI and Svelte clients; review persistence and forge integration are removed, and the Neovim package remains a dependency-loading shell. The decision below records the original rationale rather than the current feature set; see the [current architecture](../docs/architecture/index.md).

## Context and Problem Statement

`lgtm` is intended to be a review tool which is meant to address the code review bottleneck which is being caused by agents pumping out code faster than people normally can. In addition to this, the code has some rough edges. For instance, while LLM code can be exhaustively guarded (some may even say over-engineered), it can suffer from readability and organization issues. For now we'll side step concerns of architectural correctness. The point is there's a lot more review to do, and the result is a lot of folks are giving up on review altogether.

`lgtm`'s solution to this is to make local code review easier (with or without PRs) as well as local plan reviews to make it quicker to iterate on and edit plans, which is a staple in a spec driven workflow. Asking for changes via chat can be exhausting, requires a tonne of typing and simply does not scale. Therefore lgtm will offer:

1. local code review
2. local forge synced PR reviews
3. Arbitrary file reviews which can be used for annotating plans, etc.

To be sure, there's a lot of existing tools, but all have specific niches. `tuicr`can handle local or forge synced reviews, but struggles with gitignored entities like plans and mandates working in a tui. `revdiff` is for local review only, `hunk` is for PRs only, `Atlas.nvim` is for PRs only, `review.nvim` is for local reviews only. So everything has an edge in one way or another, and nothing quite works for everyone's workflow on a team, which makes these tools individual centric rather than team centric.

## Decision Drivers

- We want local reviews, PR reviews, gitignored arbitrary file reviews.
- I want lgtm to be usable with difflab for in-platform reviews
- Support tui users
- Support non-tui users
- Support purely agentic local review to keep forge threads clean and focused on human exchanges

## Considered Options

### Desktop app

A desktop app provides a dedicated window for reviewing local files.

#### Pros

- Users can review code without a terminal editor.
- The app can offer native file selection and window controls.

#### Cons

- The project must package and distribute the app for each supported operating system.
- Terminal users must leave their editor to use the review interface.
- A separate desktop interface adds work alongside future integration with difflab.

### Terminal user interface

A terminal user interface, or TUI, runs inside a terminal.

#### Pros

- Terminal users can review code without opening a browser or desktop app.
- Keyboard actions can support fast file and thread navigation.

#### Cons

- Users who prefer graphical interfaces must learn terminal controls.
- Diffs, file trees, and long discussions compete for space in the terminal.
- A separate TUI duplicates editor features that a Neovim plugin can use.

### Local web app

A local web app uses a browser for the interface and a local process for review operations.

#### Pros

- Users can review code and plans in a familiar graphical interface.
- The browser supports file trees, diff layouts, and collapsible discussion panes.
- The web interface can support future integration with difflab.

#### Cons

- The project must manage the local process and its connection to the browser.
- Browser access to local files needs explicit access controls.
- Terminal users must switch to a browser unless another client supports their workflow.

### Neovim plugin

A Neovim plugin adds review actions to the editor.

#### Pros

- Neovim users can inspect code, address feedback, and reply without leaving their editor.
- The plugin can use editor navigation, buffers, and keyboard actions.
- The plugin complements the browser interface for users who prefer terminal workflows.

#### Cons

- Users must install and use Neovim to access this interface.
- The project must maintain plugin compatibility and document its commands.
- The plugin must offer every web review action through a different interface.

## Decision Outcome

Choosing to do a local web app launched from CLI as well as a nvim plugin. Nvim plugin will allow hitting tui power users, while web app will work for non power users. CLI can cater to agentic review as well.

### Positive Consequences

- Browser users can review code and plans without learning a terminal editor.
- Neovim users can review files and reply to comments inside their editor.
- Both clients can use the same review sessions, comments, and author identities.
- The web interface can support future integration with difflab.
- The CLI, a command-line interface, gives agents access to review actions without operating either visual client.

### Negative Consequences

- The project must maintain two interfaces and keep their review features equivalent.
- The web app needs a local process for file access and review storage. The CLI must manage its startup and failures.
- Users of terminal editors other than Neovim must use the browser or CLI. This choice does not provide a separate terminal interface. However, addressing this requires a full `tuicr` competitor and entry into a crowded devtool space.

## Links

Related review tools:

- [tuicr](https://github.com/agavra/tuicr): Terminal code review with Vim-style controls.
- [Plannotator](https://github.com/backnotprop/plannotator): Visual review of agent plans and code changes.
- [revdiff](https://github.com/umputun/revdiff): Terminal review of diffs, files, and documents with annotations.
- [atlas.nvim](https://github.com/emrearmagan/atlas.nvim): Pull request review and issue management in Neovim.
- [review.nvim](https://github.com/georgeguimaraes/review.nvim): CodeDiff annotations with Markdown export for agent feedback.
- [hunk](https://github.com/smolcars/hunk): Desktop diff viewer and agent orchestration.

Related LGTM documents:

- [Core concepts PRD](../prd/0001-core-concepts/README.md)
- [Web review design and snapshots](../designs/prototypes/lgtm-v0/README.md)

## Decision History

- 2026-10-13: Changed status from proposed to accepted with explicit approval. The decision remains a local web app and Neovim plugin, with CLI access for agents.
