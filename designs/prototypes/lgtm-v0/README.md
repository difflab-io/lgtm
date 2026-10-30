# LGTM v0 web review design (historical)

This prototype is a historical visual reference, not the current application or an implemented contract. The current app is a hello-review skeleton; see [current architecture](../../../docs/architecture/index.md).

Open [index.html](index.html) in a browser. The prototype uses sample reviews and source files. Saves, closure, and GitHub submissions are simulated. Reloading resets the samples. Selecting a directory shows sample plans rather than reading the filesystem.

The reference branch is `poc/standalone-web-and-nvim` at `6a23923e1b65f8c45cbafb55e2a2dc402b907462`. The format matches Difflab's `designs/prototypes/difflab-v0`. It uses pinned CDN imports for Tailwind CSS 4.3.3, Preline UI 5.0.0, and DOMPurify 3.4.16. Preline supplies the design-state dropdown. The HTML requires internet access and no build step. The snapshots are offline-viewable 1120 × 640 PNGs.

The [core concepts PRD](../../../prd/0001-core-concepts/README.md) now describes future direction; the old production requirements illustrated by this prototype are retired.

## Screens and snapshots

| State                       | Interactive view                                  | Snapshot                                                               |
| --------------------------- | ------------------------------------------------- | ---------------------------------------------------------------------- |
| Review selection            | [Start](index.html#start)                         | [start.png](snapshots/start.png)                                       |
| Open GitHub PRs             | [PR picker](index.html#pulls)                     | [pull-requests.png](snapshots/pull-requests.png)                       |
| Existing local sessions     | [Session picker](index.html#local-sessions)       | [local-session-picker.png](snapshots/local-session-picker.png)         |
| Three-pane review           | [Stacked diff](index.html#review)                 | [review-stacked.png](snapshots/review-stacked.png)                     |
| Old and new sides           | [Side by side](index.html#split)                  | [review-side-by-side.png](snapshots/review-side-by-side.png)           |
| Unchanged source            | [Expanded context](index.html#context)            | [review-context.png](snapshots/review-context.png)                     |
| Nested replies              | [Thread](index.html#thread)                       | [review-thread.png](snapshots/review-thread.png)                       |
| File comment composer       | [File comment](index.html#file-comment)           | [review-file-comment.png](snapshots/review-file-comment.png)           |
| Line comment composer       | Click + next to a line                            | [review-line-comment.png](snapshots/review-line-comment.png)           |
| Hidden thread pane          | [Collapsed sidebar](index.html#collapsed)         | [review-threads-collapsed.png](snapshots/review-threads-collapsed.png) |
| GitHub submission           | [Submit](index.html#submit)                       | [submit-review.png](snapshots/submit-review.png)                       |
| Agent attribution           | [Agent identity](index.html#review-agent)         | [review-agent-identity.png](snapshots/review-agent-identity.png)       |
| Active local review         | [Local](index.html#local)                         | [local-review.png](snapshots/local-review.png)                         |
| Save local notes            | [Save](index.html#local-save)                     | [save-local-review.png](snapshots/save-local-review.png)               |
| Close local session         | Open Local, then Close review                     | [close-local-review.png](snapshots/close-local-review.png)             |
| Completed session           | [Closed](index.html#local-closed)                 | [local-review-closed.png](snapshots/local-review-closed.png)           |
| Choose any directory        | [Directory picker](index.html#directory-picker)   | [directory-picker.png](snapshots/directory-picker.png)                 |
| Read agent plans            | [Directory review](index.html#local-directory)    | [local-directory-review.png](snapshots/local-directory-review.png)     |
| Save directory feedback     | [Directory save](index.html#local-directory-save) | [save-directory-review.png](snapshots/save-directory-review.png)       |
| Include ignored files       | [Ignored files](index.html#local-ignored)         | [local-include-ignored.png](snapshots/local-include-ignored.png)       |
| Preserve unmatched feedback | [Orphaned comments](index.html#local-orphans)     | [local-orphaned-comments.png](snapshots/local-orphaned-comments.png)   |

## Review behavior

The first screen offers worktree changes, unstaged changes, and a local directory. Include ignored files applies to Git comparisons. Directory review opens supported text files without Git and includes ignored or excluded plans. Worktree and unstaged scopes share a sample file set. Production must load the selected comparison.

Continue session opens a searchable picker with Active and Closed local sessions. The title and status appear in the top navigation beside progress and review actions. There is no second header row. The picker beside the title changes the current PR or local session.

The left pane has collapsible folders, path filtering, file status, thread counts, and viewed indicators. The center offers stacked, side-by-side, and full-file views. Ellipsis rows expand unchanged source. Directory and ignored files without a comparison use full-file view. The three panes scroll independently.

The thread pane includes all files and review threads. Collapsed cards show only the top comment, its location, author, status, and reply count. Opening a card reveals nested replies and selects its file. Reviewers can reply, resolve, and reopen threads. Threads controls hide and restore the pane. Narrow screens use a drawer.

File comments appear in the file discussion and the thread pane. Line comments use the old or new side. Shift-clicking a second line selects a range. The composer supports writing, a basic preview, cancellation, and Command/Control Enter. An unmatched line becomes a file comment. A removed file leaves a review thread with its former location. The sample orphan state illustrates these fallbacks without computing matches.

The author selector distinguishes Sam as a human and Codex as an agent. Each new comment and reply records that identity. Agent feedback on GitHub shows the separate posting account. Identity persistence and synchronization were goals of this historical prototype; neither exists in the current skeleton.

Submit review offers an overall comment and Comment, Approve, or Request changes. Local reviews use Save review without an outcome selector. Saving keeps a session Active. Close review marks it complete and preserves its discussion. Closed sessions are readable, and New review starts further work.

## Production requirements

The PRD requires durable sessions, source revisions, local drafts, location fallback, agent identity, and equivalent web and Neovim actions. The prototype does not implement storage, filesystem access, Git comparisons, GitHub requests, or the plugin. Its session picker uses fixed samples. Production must restore each selected session's actual files and discussion.

The PoC's `internal/review/anchors.go` publishes comments within diff hunks inline. Other notes become summary text. File comments and range anchors need explicit support in the shared review model. The submit dialog explains summary export for unsupported inline locations. GitHub is the only remote service in scope.

Production must retain drafts on failure, detect changed PR content, avoid duplicate publication, and preserve agent attribution after GitHub synchronization. It must also support keyboard navigation, focus return after dialogs, unavailable source, unsupported files, and conflicting updates. Closing a session must save it successfully before changing its status. These requirements are described in the PRD rather than simulated by every prototype control.

## Refresh snapshots and check interactions

The capture script checks source selection, directory plans, ignored files, session selection, closure, orphaned threads, author identity, and the original review controls. It then refreshes all 21 PNGs. Dependencies can be installed outside the repository:

```sh
npm install --prefix /tmp/lgtm-design-browser playwright@1.64.0
LGTM_PLAYWRIGHT_MODULE=/tmp/lgtm-design-browser/node_modules/playwright/index.mjs \
LGTM_BROWSER_CHANNEL=chrome \
node designs/prototypes/lgtm-v0/capture.mjs
```

This command uses installed Google Chrome. With Playwright's Chromium installed, omit `LGTM_BROWSER_CHANNEL`. The script also accepts a normal local `playwright` installation. Refresh the snapshots after HTML changes.
