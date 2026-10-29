# LGTM user guide

Use LGTM to review saved changes and discuss code. The Neovim workflows below are available now. The browser review interface remains pending.

## Set up Neovim

Install the compatible LGTM CLI and plugin using the [plugin setup instructions](../apps/lgtm.nvim/README.md#setup). Supported lazy.nvim installs resolve NuiComponents, NUI, CodeDiff, WhichKey, and nvim-notify from one LGTM declaration. Other managers need explicit dependencies and runtime setup. CodeDiff also needs its native library. It also needs Neovim 0.11 or later and curl. Git reviews need Git.

Start LGTM before opening a review:

```sh
lgtm up --no-open
```

For GitHub reviews, sign in with `gh auth login` as the same local user who runs LGTM. Notes and local reviews do not require GitHub credentials. Run `:checkhealth lgtm` in Neovim if setup or connection fails.

The shortcuts below use the default `<leader>r` prefix. Press `<leader>r` to see the WhichKey menu, or use the equivalent `:Lgtm` commands. Your configured leader key and plugin configuration determine the actual prefix.

## Start a review

Open Neovim in the repository that you want to review, then run `:Lgtm review`. Choose the content that you want to inspect:

| Choice | What you review |
| --- | --- |
| Working | Saved working files against the starting commit, including staged changes. |
| Staged | Changes currently added to the Git index. |
| Branch | Changes since the current branch diverged from a selected base branch. |
| PR | An open GitHub pull request associated with the current branch. |

For Branch, enter the base branch when prompted. The default is `main`. A clean Working or Staged comparison reports that there are no changes. A resumed local review keeps its original comparison. A PR review follows the current PR commits when you reopen, refresh, or save a draft.

### Select a pull request

Run `:Lgtm prs` or press `<leader>rp`. LGTM infers the repository from your Git remote and searches for relevant open pull requests. If it cannot find a GitHub remote, enter the repository URL when prompted.

Type to filter the results. Use Up/Down or Ctrl-N/Ctrl-P to move the selection. Press Enter to open the selected pull request, or Esc to cancel. Wait for the review-ready notification before adding comments.

### Review ignored files or a directory

Run the directory command with the selected path:

```vim
:Lgtm directory .difflab/plans/specific-plan
```

This includes ignored and untracked text files under the directory. You can also run `:Lgtm directory` and enter a path when prompted. Binary, oversized, and unavailable files can produce an unsupported-content message.

## Navigate files and track progress

Git reviews use CodeDiff's file tree and diff panes. Directory reviews show an LGTM file pane. Select a file in the tree and press Enter to open it.

| Action | Command or key |
| --- | --- |
| Show or hide the file pane | `f` from a diff pane |
| Focus the file list | `:Lgtm files` or `<leader>rF` |
| Next or previous file | `]f` / `[f` from a diff pane |
| Next or previous unreviewed file | Tab / Shift-Tab from a diff pane |
| Toggle the selected tree file as reviewed | `m` |
| Toggle the current file as reviewed | `<leader>rm` |
| Switch side-by-side and inline layouts | `:Lgtm layout` |

A checkmark identifies a reviewed file. Toggle the same action to remove it. Marks apply to Working, Staged, Branch, PR, and directory reviews. Local marks persist across editor restarts. PR marks read and update GitHub's Viewed state.

The plugin clears a mark when either compared side changes. A hidden file also loses its mark when its working or staged source changes. Use side-by-side layout when you need to select deleted lines for comments.

## Add review comments

Place the cursor on a diff line and press lowercase `c` or `<leader>rc`. For a range comment, select the lines visually before pressing `c`. Write the comment in the centered composer, then press Ctrl-S to save it.

Use `:Lgtm comment file` or `<leader>rf` for a whole-file comment. Use `:Lgtm comment general` or `<leader>rg` for feedback about the review as a whole. In the composer, normal-mode `q` or Esc cancels without sending the text.

Comments appear as bordered cards with status, author, reply count, and a one-line preview. File comments appear above the file. Notes and local review comments stay local.

PR line comments, file comments, and supported replies save as private GitHub drafts. General PR comments stay local until you submit the review. A notification confirms whether the comment stayed local or saved on GitHub.

### Read and manage a thread

Place the cursor on an annotated line and press Enter or `K`. The centered thread window separates the top-level comment from its indented replies. Use Tab or Shift-Tab to select the comment that you want to act on.

| Key in the thread window | Action |
| --- | --- |
| `c` | Reply to the selected thread |
| `e` | Edit the selected owned comment |
| `dd` | Confirm deletion of the selected owned comment |
| `x` | Resolve or reopen the thread |
| `q` / Esc | Close the window and return to the diff |

The edit title and selected highlight identify whether you are editing the top-level comment or a reply. Only comments that you own and that support the action can be edited or deleted. For local drafts, delete replies before deleting their top-level comment.

Use `:Lgtm threads` or `<leader>rt` to read threads across all files, including general comments. Select a file thread in that pane and press Enter to visit its file. From a diff pane, use `]c` / `[c` to visit comments.

Use `za` to expand the card at the cursor, `zA` for cards across the file, and `gH` to toggle overlays. Replying, editing, committing, and closing a review do not automatically resolve its threads. Resolve a discussion explicitly with `x` when you consider it addressed. Resolving and deleting show progress immediately. The card changes after GitHub confirms the action. Deleted GitHub comments disappear from the diff, thread window, and export. Replies remain visible when the original comment is deleted. A refresh also removes comments deleted on GitHub; comments that have never been posted remain local.

## Save, close, and resume

Each successful comment change saves immediately. Run `:Lgtm save` if you want confirmation that the active review is saved. Run `:Lgtm close` or press `<leader>rq` to close the review view without completing it.

To resume, run `:Lgtm sessions` or press `<leader>rS`. Filter the list, select a saved review, and press Enter. Saved reviews keep their recorded comparison.

Run `:Lgtm sessions completed` to inspect saved and completed reviews. Run `:Lgtm complete` when you want to finish a local review. Completion retains its discussion and open threads. It does not submit anything to GitHub.

Lowercase `<leader>rs` submits the active PR review. Uppercase `<leader>rS` opens saved review selection. These are different actions.

## Submit a pull request review

1. Open the PR review that you want to submit.
2. Run `:Lgtm submit` or press `<leader>rs`.
3. Choose comment, approve, or request changes.
4. Enter an optional final comment, or leave the composer empty.
5. Press Ctrl-S to submit.
6. Wait for the review-submitted notification.

For your own PR, choose `comment`; GitHub rejects self-approval and self-request-changes. A rejected submission keeps your review editable and retains its pending comments and final text. Choose another outcome and submit again. If the submission result is unclear, use `:Lgtm reconcile` before retrying.

After a successful submission, keep reviewing in the same diff. Your next comment or reply starts a new pending review for that PR. Published comments remain visible, and the earlier review keeps its final message and outcome. Merely opening a discussion or cancelling a composer does not start another review.

The pending GitHub draft becomes the submitted review. Closing the view or saving a comment alone does not submit it. If submission fails, retain the final text and follow the recovery notification before trying again.

## Recover a save or submission

| What you see | What to do |
| --- | --- |
| A comment saved locally but failed to save on GitHub | Keep the saved comment. Run `:Lgtm sync` or `<leader>ry`. |
| A draft save lost its confirmation | Inspect the existing thread, then use `:Lgtm sync`. Avoid adding the same text again immediately. |
| Submission has an uncertain result | Run `:Lgtm reconcile` before another submission attempt. |
| A composer reports a failed save | Keep its text and correct the reported problem before saving again. |
| PR comments failed to load | Fix the reported backend or GitHub error, then run `:Lgtm refresh`. The diff stays open while you retry. |
| The displayed discussion is stale | Run `:Lgtm refresh`. Existing drafts remain if the refresh fails. |

Synchronizing drafts does not submit the review. Reconciliation can finish a submission that you already requested, so use it as an explicit recovery action. If LGTM cannot establish the result, inspect the pending review on GitHub before taking further action.

## Export a review

Run `:Lgtm export` from an open review. Choose Preview, Save file, or Copy to clipboard.

Exports contain numbered feedback, readable file and line locations, useful replies, and any final review comment. `~` marks old-side lines. Moved or lost locations include the original evidence. Internal database IDs and synchronization metadata are omitted.

For Save file, enter the output path and confirm an overwrite if the file already exists. Press `q` to close an export preview. Exporting keeps the review unchanged and does not submit anything.

## After files, commits, or pull requests change

Review panes show saved revisions. Editing, moving or deleting the original source does not alter an open review. Working reviews capture saved files; staged reviews capture the index. Unsaved editor changes are not included.

Run `:Lgtm refresh` to capture another revision in the same active session. Use `:Lgtm threads` to select feedback from an earlier revision and open its original source. Resume from `:Lgtm sessions` after a restart, even if the original directory has moved or disappeared.

PR pushes retain the session, pending GitHub review and draft text. GitHub supplies current locations and outdated status. LGTM does not search for matching text or move a comment to another line. A historical draft that cannot be attached to the pending review stays local with a synchronization error. A push during final submission requires a refresh; an uncertain submission requires receipt recovery.

Standalone live-buffer notes and promotion are no longer available. Existing note records remain stored and can be read or exported through the archived notes API scope.

## Appearance and notifications

Comment cards, thread windows, and composers share the plugin's theme. Their colors follow your Neovim colorscheme, with configurable borders and highlight overrides. Use the [appearance instructions](../apps/lgtm.nvim/README.md#appearance) to customize them.

Notifications distinguish a saved local comment, a saved GitHub draft, a confirmed submission, and a failure. Keep failed text until you have a confirmed result or decide to cancel it. If notifications or review windows do not appear, run `:checkhealth lgtm`.

## Web interface

The bundled browser interface currently shows a demonstration. Review selection, diffs, commenting, threads, and review submission in the web interface remain pending. Use the Neovim plugin for the review workflows in this guide.

## Further reading

- [Plugin installation and troubleshooting](../apps/lgtm.nvim/README.md#setup).
- [Editor help](../apps/lgtm.nvim/doc/lgtm.txt).
- [CLI commands](../apps/lgtm-cli/README.md).
- [Review concepts and comment behavior](architecture/index.md).
