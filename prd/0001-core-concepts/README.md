# Core concepts: code and document review

- Status: draft
- Owner: Project maintainers
- Date: 2026-10-10
- Related ADRs: [Initial architecture](../../adr/0001-initial-architecture.md)
- Design: [LGTM v0 prototype](../../designs/prototypes/lgtm-v0/README.md)

## Summary

LGTM lets people and agents review code and documents from the web or Neovim. Reviewers can inspect local changes, files in any directory, or a GitHub pull request (PR). A PR is a proposed change on GitHub. Both clients must support the same review actions.

Local reviews work without GitHub and without a Git repository. They support private agent plans, including ignored and excluded files. Review sessions keep comments, replies, author identities, and completion status across restarts. GitHub is the only remote review service in scope.

## Problem and Evidence

Reviewers need feedback before they create a PR. Agent plans can live in ignored directories and never become committed files. A review tool that requires committed code or a PR cannot cover this work.

The `poc/standalone-web-and-nvim` branch demonstrates web and Neovim clients for local and GitHub reviews. Its reference revision is `6a23923e1b65f8c45cbafb55e2a2dc402b907462`. Its web client lists files without a directory tree, limits the thread pane to one file, and lacks the requested review controls.

The product requirements come from the design discussion that produced this PRD. That discussion requires Neovim parity, directory reviews, ignored-file inclusion, resumable local sessions, session closure, durable comment locations, and agent identity. The [prototype](../../designs/prototypes/lgtm-v0/index.html) illustrates the proposed experience with sample data. Its saves and submissions are simulated.

## Users and Use Cases

The first release serves three groups:

- Engineers review local edits or GitHub PRs, then reply to feedback and complete the review.
- Plan authors review Markdown and other text documents before agents implement their instructions.
- Agents inspect a review, leave attributed feedback, address changes, and reply within the same session.

A person can start a review in the browser and continue in Neovim. An agent can review an ignored plan while its author addresses comments. All participants use the same session and see the same discussion.

## Goals

The first release must provide these outcomes:

- Review local code and documents without requiring a commit or GitHub account.
- Resume named local sessions and retain completed reviews in history.
- Keep web and Neovim review capabilities equivalent.
- Keep comments useful when lines change or files disappear.
- Preserve each human or agent author independently of the GitHub posting account.
- Publish chosen review feedback to GitHub through an explicit submission action.

## Non-Goals

The first release excludes these capabilities:

- Support for GitLab, Bitbucket, or any other remote review service.
- Replacement of GitHub repository management, merging, or issue tracking.
- A hosted collaboration platform for shared local reviews across machines.
- Agent execution, agent scheduling, or automatic code changes as part of the review UI.
- Review of binary content, rendered office documents, or image annotations.

## Proposed Experience

### Core concepts

| Concept                | Meaning                                                            |
| ---------------------- | ------------------------------------------------------------------ |
| Review session         | A named discussion about selected code or documents.               |
| Local source           | A worktree comparison, unstaged comparison, or selected directory. |
| GitHub review          | A session tied to an explicit repository and PR number.            |
| Revision               | A saved version of the reviewed content or comparison.             |
| File                   | A document identified by its path within the source.               |
| Anchor                 | A comment location within a file or revision.                      |
| Thread                 | A top comment and its replies, with open or resolved status.       |
| File comment           | A thread about a whole file, without line numbers.                 |
| Review thread          | A thread about the session, without a current file location.       |
| Author identity        | The person or agent responsible for a comment or action.           |
| GitHub posting account | The authenticated account that publishes an action to GitHub.      |

A local session keeps its identity when its files change. A GitHub session stays separate from local sessions, even when they use the same branch. Saving, submitting, resolving a thread, and closing a session are distinct actions.

### Local and GitHub review

The opening screen offers worktree changes, unstaged changes, and a local directory. An Include ignored files checkbox applies to both Git comparisons. A local session picker opens existing Active and Closed sessions. A separate picker opens GitHub PRs.

The review screen uses one top navigation bar for the session or PR title, status, progress, and actions. A file tree sits on the left. The file or diff occupies the center. A collapsible pane on the right shows threads across the whole review.

### People and agents

A person and an agent can exchange feedback until a local session is complete. Each participant uses a stable LGTM identity. Replies, resolutions, and closure retain the identity that performed the action. An agent's GitHub comments can use a person's posting account while retaining explicit agent attribution.

## Requirements

All requirements in this section are required for the first release. The screenshots illustrate layout and behavior. They do not replace acceptance criteria or prove production support.

### Start or resume a review

Worktree changes compare HEAD, the latest checked-out commit, with the current files. This scope includes staged and unstaged changes. Unstaged changes compare the index, Git's staged snapshot, with the current files. This scope excludes staged changes. Both scopes include supported untracked files that are not ignored.

The PR picker supports search by number or title. It shows the PR number, title, author, branch, and draft status. It must provide loading, empty, authentication-error, and retry states. Selecting a PR opens its review without changing local files or overwriting an existing local session.

The local picker supports search by session name, source path, or status. Each result shows its name, source scope, location, Active or Closed status, and last activity. Starting a new session and continuing an existing session are separate actions. Selecting a session restores its discussion, summary, source selection, and review progress.

Acceptance: Open each local scope and a GitHub PR. Resume two sessions for the same source and make sure that their comments remain separate.

![Review selection with local scopes and ignored-file inclusion](../../designs/prototypes/lgtm-v0/snapshots/start.png)

![Searchable open GitHub PR picker](../../designs/prototypes/lgtm-v0/snapshots/pull-requests.png)

![Local session selection with Active and Closed sessions](../../designs/prototypes/lgtm-v0/snapshots/local-session-picker.png)

### Review a directory and ignored files

A reviewer can select any accessible local directory. Git is optional. Directory review lists supported files beneath the chosen directory and opens their content for annotation. It does not require a comparison or treat every file as an added diff.

The initial supported content is readable UTF-8 text, including Markdown, plain text, source code, and text configuration files. File extensions do not prevent a readable text file from opening. Unsupported or unreadable files show a reason. An empty directory shows an empty state.

Git ignore rules and application exclusions must not block explicitly chosen review content. Include ignored files adds supported ignored or excluded files to a Git-based review. Directory review includes those files within the selected directory. The file tree identifies their status. Reviewers can narrow the selected files before saving a revision.

Opening a plan does not stage it, change ignore rules, or publish it. A reviewer can comment on its lines and on the whole file. Without a comparison, the UI shows the file. When two saved revisions exist, reviewers can compare them.

Acceptance: Review an ignored `.plans/plan.md`, an application-excluded plan, and a directory outside Git. Annotate and resume each without staging or publishing files.

![Directory selection](../../designs/prototypes/lgtm-v0/snapshots/directory-picker.png)

![Directory review of agent plans](../../designs/prototypes/lgtm-v0/snapshots/local-directory-review.png)

### Navigate files and maximize review space

The left pane must be a directory tree with collapsible folders. It supports path filtering, file status, viewed indicators, and open-thread counts. Directory sessions show all selected files. Git comparisons show changed files and included untracked or ignored files.

The top navigation contains the PR or session name and its picker. It also contains status, review progress, thread visibility, and submission or local actions. There is no second title row. Files, content, and threads scroll independently.

Acceptance: Filter a nested path, collapse its folder, select another file, and mark it viewed. The current session and its discussion remain open.

![Review with a combined title and navigation bar](../../designs/prototypes/lgtm-v0/snapshots/review-stacked.png)

### Inspect differences and complete files

A reviewer can switch between stacked and side-by-side differences. Both modes show old and new line numbers and distinguish additions from removals. The UI preserves which side receives a comment. Reviewers can also read the complete file.

Unchanged source appears behind clickable ellipsis rows. Reviewers can expand and collapse that source without losing comments. Missing source context must show an explicit unavailable state. Long lines must remain readable through wrapping or scrolling.

Added, deleted, and renamed files retain their status. Renamed files show their previous path. Removed lines remain reviewable on the old side. Large files must not prevent access to other files and threads.

Acceptance: Inspect the same change in both diff modes. Expand surrounding source, add an old-side comment, and read the full file.

![Side-by-side differences](../../designs/prototypes/lgtm-v0/snapshots/review-side-by-side.png)

![Expanded unchanged source](../../designs/prototypes/lgtm-v0/snapshots/review-context.png)

### Comment on lines, ranges, files, and reviews

A reviewer can add comments to a line, a line range, a whole file, or the review. A comment records its author identity. A range records its start, end, and comparison side when applicable. File comments appear in the file discussion and the all-file thread pane.

The composer supports writing, preview, cancellation, and a keyboard save action. Draft text survives navigation, refresh, and service failures. The UI identifies comments that remain local or await GitHub submission. It must not report a successful save before storage acknowledges it.

Acceptance: Add each comment type from both clients. Restart the clients and make sure that saved comments and unfinished drafts remain available.

![Line comment composer](../../designs/prototypes/lgtm-v0/snapshots/review-line-comment.png)

![File comment composer](../../designs/prototypes/lgtm-v0/snapshots/review-file-comment.png)

### Keep local comments attached as content changes

A local comment uses the session, file path, and line or range as its location. A commit reference is optional. A reviewer can place comments directly in an uncommitted document. Saved source context retains the content needed to understand the original feedback.

When content changes, LGTM attempts to place the comment at a matching location. It must not silently choose an uncertain line. If the line no longer matches, the thread moves to the file level. If the file is gone, the thread remains at the review level.

Both fallbacks preserve the top comment, replies, author identities, resolution status, and former path and line numbers. The UI explains why the location changed. A reviewer can assign a new valid location without losing the original reference. Missing-file threads remain discoverable in the all-file pane.

Acceptance: Comment on an ignored plan without a commit. Insert lines, remove the commented passage, and delete the file. Make sure that each fallback preserves the discussion.

![Unmatched lines become file comments and removed files become review threads](../../designs/prototypes/lgtm-v0/snapshots/local-orphaned-comments.png)

### Read and resolve threads across all files

The right pane shows threads from every file and from the review itself. It supports open-only filtering and displays pending, resolved, unmatched, and missing-file states. Closing the pane expands the content area. Opening it restores the review-wide thread view.

Collapsed cards show only the top comment, its author, location, state, and reply count. Opening a card reveals the whole conversation and selects its file when available. Replies are visually nested. Reviewers can reply, resolve, and reopen threads while the session remains active.

Acceptance: Open a thread on another file, read its replies, reply, resolve it, and show it again through the filter. Repeat for a missing-file thread.

![Expanded thread with nested replies](../../designs/prototypes/lgtm-v0/snapshots/review-thread.png)

![Review content with the thread pane collapsed](../../designs/prototypes/lgtm-v0/snapshots/review-threads-collapsed.png)

### Save and close local sessions

Local sessions have Active and Closed status. Saving retains notes and the overall summary while keeping the session Active. Closure means that the review session is done, comparable to a completed PR. It is not approval, rejection, or resolution of every thread.

The UI provides a separate Close review action. It saves the review before marking it Closed. If open threads remain, the closure dialog shows their count and explains that their discussion remains in history. A failed save must leave the session Active.

Closed sessions remain selectable and readable. Their reviewed content, former locations, comments, summary, participants, and closure identity remain available. Closed sessions do not accept new comments or content revisions. Further work starts a new session. Reopening completed sessions is outside the initial scope.

Acceptance: Exchange feedback between a person and an agent. Save and continue the session, then close it with an open thread. Resume it from history and make sure that the discussion remains readable.

![Save local review without GitHub outcomes](../../designs/prototypes/lgtm-v0/snapshots/save-local-review.png)

![Close a local session while retaining open threads](../../designs/prototypes/lgtm-v0/snapshots/close-local-review.png)

![Closed local session](../../designs/prototypes/lgtm-v0/snapshots/local-review-closed.png)

### Preserve human and agent identity

LGTM must model authors as stable identities with an identifier, display name, and human or agent kind. An optional run reference distinguishes executions of the same agent. Session participation and every comment or review action retain the responsible identity. A displayed name alone is insufficient to identify an author.

The GitHub posting account is a separate identity. When an agent publishes through a user's GitHub account, LGTM retains the agent author and the posting account. Published text includes readable agent attribution when GitHub cannot store that distinction. The UI must not relabel the agent as the human account holder.

Synchronizing a published comment must preserve its original LGTM author. LGTM must use its recorded publication relationship, rather than treating arbitrary attribution text as proof of identity. Imported GitHub comments without an LGTM relationship retain their GitHub author.

Acceptance: A person and two agents comment under separate LGTM identities. Publish agent feedback through one GitHub account and synchronize it again. Make sure that the identities remain distinct in both clients.

![Agent author identity and separate GitHub posting account](../../designs/prototypes/lgtm-v0/snapshots/review-agent-identity.png)

### Submit a GitHub review

The top-bar Submit review action opens a dialog. It supports an overall comment and the outcomes Comment, Approve, and Request changes. Request changes requires an overall comment. A Comment outcome requires an overall comment or pending feedback.

The dialog identifies which comments will publish. If GitHub cannot accept a file comment or an out-of-diff note inline, the UI explains its inclusion in the summary. LGTM retains the original file or line location locally. It must not silently discard feedback.

LGTM reads current GitHub discussion before opening a PR review and again before publishing. Existing GitHub drafts remain distinct from unsent LGTM drafts. A stale or incomplete remote view blocks publication and explains how to refresh. New commits preserve old anchors and clearly mark outdated feedback.

A failed submission retains the summary and pending comments. Retrying must not duplicate published feedback. GitHub permissions and closed or merged PR status determine available actions. A submitted review outcome is distinct from the PR's lifecycle status.

Acceptance: Submit each outcome, interrupt publication, and retry. Make sure that every chosen comment appears once and that rejected publication retains the draft.

![GitHub review submission](../../designs/prototypes/lgtm-v0/snapshots/submit-review.png)

### Neovim functional parity

The Neovim plugin is a required first-release client. Every web review capability must be available through keyboard actions in the plugin. Layouts can differ, but review behavior, stored state, and outcomes must agree. A CLI command that sends a user to the browser does not satisfy parity.

| Capability        | Required Neovim behavior                                                                          |
| ----------------- | ------------------------------------------------------------------------------------------------- |
| Local sources     | Choose worktree, unstaged, or any directory, with ignored-file inclusion.                         |
| Session selection | Search, create, resume, and inspect Active or Closed sessions.                                    |
| GitHub selection  | Search and open PRs, with loading and recovery states.                                            |
| Files             | Navigate a directory tree, filter paths, and mark files viewed.                                   |
| Content           | Read full files, switch diff layouts, and expand unchanged source.                                |
| Comments          | Compose, preview, save, and cancel line, range, file, and review comments.                        |
| Threads           | Show all-file threads, collapse top comments, expand nested replies, and filter resolved threads. |
| Locations         | Show old/new sides, outdated anchors, file fallbacks, and missing-file review threads.            |
| Participants      | Select and retain human or agent author identity separately from the GitHub account.              |
| Local completion  | Save without a GitHub outcome, close a session, and read its history.                             |
| GitHub completion | Add an overall comment and submit Comment, Approve, or Request changes.                           |
| Recovery          | Keep drafts after failures and avoid duplicate publication.                                       |

The plugin must provide documented commands and discoverable key mappings. It must not require mouse input. Switching between web and Neovim must retain session identity and refresh shared changes without losing local draft text. The plugin must report an unavailable review service and provide a way to reconnect.

Acceptance: Run the same review scenarios in both clients. Start a session in web, continue in Neovim, add an agent reply, and read it in web. Close the session in Neovim and read its completed state in web.

### Agent access and accessibility

Agents need a documented programmatic interface for the same session and discussion operations. It must support source selection, file and thread reads, attributed comments, replies, resolutions, refresh, save, and closure. GitHub publication requires an explicit operation. An agent must not need to operate a browser to review an ignored plan.

The web UI must support keyboard navigation, visible focus, labeled controls, and accessible dialogs. Closing a dialog returns focus to its opening control. Narrow screens retain access to files, content, and threads. Both clients show status through text as well as color.

Acceptance: Complete a local review with keyboard input alone. Use the agent interface to review an ignored plan and exchange replies within the same session.

## Success Measures

The release is successful when the required scenarios pass in web, Neovim, and the shared review interface. The acceptance set must cover a tracked code change, an ignored agent plan, a directory outside Git, and a GitHub PR. It must also cover session continuation, closure, missing locations, and agent attribution.

Restart and failure scenarios must preserve saved feedback and unfinished drafts. Cross-client scenarios must produce the same session and comments. Retry scenarios must publish no duplicate feedback. Local-only scenarios must produce no GitHub publication.

Initial evaluation uses acceptance runs and feedback from project maintainers. No adoption or timing targets are claimed before measurement. Later targets can use observed review completion, lost-draft reports, and parity defects.

## Risks and Dependencies

| Risk or dependency                                               | Required response                                                                       |
| ---------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Existing architecture describes repository-backed local contexts | Decide directory-session identity and optional commit references before implementation. |
| The PoC lacks some proposed operations                           | Extend the shared review model before connecting UI controls to production actions.     |
| GitHub cannot express every local annotation                     | Disclose summary export and preserve the original local anchor.                         |
| A local file changes during review                               | Save readable revisions and use explicit anchor fallbacks.                              |
| Clients implement review rules differently                       | Use shared behavior and the same acceptance scenarios.                                  |
| GitHub records the posting account as author                     | Preserve LGTM agent identity and provide readable public attribution.                   |
| Multiple participants update one session                         | Detect competing changes and preserve draft text while refreshing.                      |

## Open Questions

The requirements above fix the user-facing scope. These implementation choices remain open:

- What limits apply to directory depth, text-file size, and retained review revisions?
- How will moved directories and renamed worktrees retain local-session identity?
- What attribution format will identify agents in published GitHub comments?
- Which GitHub draft limitations must the UI disclose before publication?
- What interface and authentication will agents use to select their LGTM identity?

## Rollout

First, review this PRD and the prototype together. Resolve the directory and identity model questions in architecture records. Then implement shared session, location, identity, and GitHub behavior.

Release web and Neovim against the same acceptance set. Required Neovim functionality must not be deferred behind a web-only release. Exercise local code, ignored plans, directory sessions, agent exchanges, closure, and GitHub failure recovery before release.

The prototype remains a design reference. Production behavior must satisfy this PRD even when its layout differs from a sample screenshot.
