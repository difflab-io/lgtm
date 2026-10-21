/**
 * @module-tag e2e
 * @module-tag nvim
 * @module-tag github
 * @module-tag pr-review
 */
import { describe, it, expect } from 'vitest'
import { profiles } from './fixtures.js'
import { usePrEditor, selectBranchPr } from './pr-review.fixtures.js'
import { githubCredentials } from '../common/github.js'
import { waitFor } from '../common/runtime.js'
import { expectFileSeen } from './assertions.js'

for (const profile of profiles) {
  describe(`When choosing a PR from :Lgtm review [${profile}]`, { tags: [profile] }, () => {
    const fixture = usePrEditor(profile)

    it('The PR option infers the branch and opens its review in the native CodeDiff tree', async () => {
      // Arrange
      const { editor, github } = fixture()

      // Act
      await editor.choose('PR')
      await editor.command('Lgtm review')
      await editor.wait(
        'require("lgtm.picker").last and not require("lgtm.picker").last.closed',
        'PR picker displayed',
      )
      const options = await editor.value('_G.e2e.last_select.items')
      const requestedInput = await editor.value('_G.e2e.last_input')
      await editor.keys(`#${github.number} `)
      await editor.wait('#require("lgtm.picker").last.filtered == 1', 'branch PR listed')
      const selectedAt = performance.now()
      await editor.keys('<CR>')
      const firstFile = await editor.ready()
      const firstFileMs = performance.now() - selectedAt
      const state = await editor.loaded()

      // Assert
      expect(options).toEqual(['working', 'staged', 'branch', 'PR'])
      expect(requestedInput).toBeNull()
      expect(state.target.pullRequest).toMatchObject({
        repositoryUrl: github.repositoryUrl,
        number: String(github.number),
      })
      expect(firstFile.file.path).toBe(github.path)
      expect(
        firstFileMs,
        'first native file renders before slow comment loading can block it',
      ).toBeLessThan(2000)
      expect(state.native_explorer).toBe(true)
      expect(state.file.path).toBe(github.path)
      expect(await editor.text(state.panes.old.buf)).toContain('second')
      expect(await editor.text(state.panes.new.buf)).toContain('changed')
      expect(
        await editor.value(
          'require("codediff.ui.lifecycle").get_panel_view(require("lgtm.context").current.tab) ~= nil',
        ),
      ).toBe(true)
    })

    it('Typing filters branch PRs, arrow keys change selection and Escape cancels without starting a review', async () => {
      // Arrange
      const { editor, github } = fixture()
      const alternate = await github.createBranchComparison()
      await github.waitForSearch(alternate.number)

      // Act
      await editor.command('Lgtm prs')
      await editor.wait(
        'require("lgtm.picker").last and #require("lgtm.picker").last.filtered >= 2',
        'both branch PRs listed',
      )
      const first = await editor.value('require("lgtm.picker").last.index')
      await editor.keys('<Down>')
      const next = await editor.value('require("lgtm.picker").last.index')
      await editor.keys('<Up>')
      const previous = await editor.value('require("lgtm.picker").last.index')
      await editor.keys(`#${alternate.number} `)
      await editor.wait('#require("lgtm.picker").last.filtered == 1', 'typed PR filter')
      const selected = await editor.value('require("lgtm.picker").last.filtered[1].number')
      await editor.keys('<Esc>')

      // Assert
      expect(next).not.toBe(first)
      expect(previous).toBe(first)
      expect(Number(selected)).toBe(alternate.number)
      expect(await editor.value('require("lgtm.picker").last.closed')).toBe(true)
      expect(await editor.state()).toBeNull()
    })
  })

  describe(`When creating draft comments [${profile}]`, { tags: [profile] }, () => {
    const fixture = usePrEditor(profile)

    it('Lowercase c creates a remote draft shown as a full-width card with status and preview text', async () => {
      // Arrange
      const { editor, github } = fixture()
      await selectBranchPr(editor, github)
      await editor.pane('new', 2)

      // Act
      await editor.keys('c')
      await editor.compose('Please explain this line')
      const state = await editor.comments(1)
      const marks = await editor.marks()
      const remote = await github.api(
        `repos/${github.repository}/pulls/${github.number}/reviews/${state!.review.forgeReviewId}/comments`,
      )

      // Assert
      expect(remote).toEqual(
        expect.arrayContaining([
          expect.objectContaining({ body: expect.stringContaining('Please explain this line') }),
        ]),
      )
      expect(marks).toHaveLength(1)
      expect(marks[0][3].virt_lines.length).toBeGreaterThan(0)
      expect(JSON.stringify(marks)).toContain('Please explain this line')
      expect(JSON.stringify(marks)).toContain('GitHub draft')
      expect(
        (await editor.notices()).some((notice) => notice.message === 'Draft saved on GitHub'),
      ).toBe(true)
    })

    it('Old-side, file-level and general feedback retain their scopes in the editor and GitHub', async () => {
      // Arrange
      const { editor, github } = fixture()
      await selectBranchPr(editor, github)
      await editor.pane('old', 2)

      // Act
      await editor.keys('c')
      await editor.compose('Explain removed source')
      await editor.comments(1)
      await editor.command('Lgtm comment file')
      await editor.compose('File-level concern')
      await editor.comments(2)
      await editor.command('Lgtm comment general')
      await editor.compose('Overall review concern')
      const state = await editor.comments(3)
      const remote = await github.api<Array<{ body: string; path: string }>>(
        `repos/${github.repository}/pulls/${github.number}/reviews/${state!.review.forgeReviewId}/comments`,
      )
      await editor.command('Lgtm threads')
      const list = await editor.text()

      // Assert
      expect(state!.conversation.threads).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            anchor: expect.objectContaining({ kind: 'line', side: 'old' }),
          }),
          expect.objectContaining({ anchor: expect.objectContaining({ kind: 'file' }) }),
          expect.objectContaining({ anchor: expect.objectContaining({ kind: 'general' }) }),
        ]),
      )
      for (const body of ['Explain removed source', 'File-level concern'])
        expect(remote.some((comment) => comment.body.includes(body))).toBe(true)
      for (const body of ['Explain removed source', 'File-level concern', 'Overall review concern'])
        expect(list).toContain(body)
    })

    it('Enter opens a centered modal that identifies the selected top-level comment', async () => {
      // Arrange
      const { editor, github } = fixture()
      await selectBranchPr(editor, github)
      await editor.pane('new', 2)
      await editor.keys('c')
      await editor.compose('Top-level concern')
      await editor.comments(1)

      // Act
      await editor.popup('Top-level concern')
      const modal = await editor.value('vim.api.nvim_win_get_config(0)')
      const text = await editor.text()

      // Assert
      expect(modal.relative).toBe('editor')
      expect(Number(modal.row)).toBeGreaterThan(1)
      expect(Number(modal.col)).toBeGreaterThan(1)
      expect(JSON.stringify(modal.footer)).toContain('Top-level comment')
      expect(text).toContain('Top-level concern')
      expect(text).toContain('TOP-LEVEL COMMENT')
    })

    it('Replies show nesting and their own selection label, and can be edited and deleted', async () => {
      // Arrange
      const { editor, github } = fixture()
      await selectBranchPr(editor, github)
      await editor.pane('new', 2)
      await editor.keys('c')
      await editor.compose('Root concern')
      await editor.comments(1)
      await editor.popup('Root concern')

      // Act
      await editor.keys('c')
      await editor.compose('First reply')
      await editor.comments(2)
      await editor.pane('new', 2)
      await editor.popup('First reply')
      const replyModal = await editor.value('vim.api.nvim_win_get_config(0)')
      const discussion = await editor.text()
      await editor.keys('e')
      const title = await editor.value(
        'vim.api.nvim_win_get_config(require("lgtm.composer").last.win).title',
      )
      await editor.compose('Edited reply')
      await editor.pane('new', 2)
      await editor.popup('Edited reply')
      await editor.choose('Delete')
      await editor.keys('dd')
      const remaining = await editor.comments(1)

      // Assert
      expect(JSON.stringify(replyModal.footer)).toContain('Reply')
      expect(discussion).toContain('Root concern')
      expect(discussion).toContain('First reply')
      expect(JSON.stringify(title)).toContain('reply')
      expect(remaining!.conversation.comments[0].body).toBe('Root concern')
      expect(JSON.stringify(await editor.marks())).not.toContain('Edited reply')
    })

    it('Resolve is unavailable for an unpublished draft thread', async () => {
      // Arrange
      const { editor, github } = fixture()
      await selectBranchPr(editor, github)
      await editor.pane('new', 2)
      await editor.keys('c')
      await editor.compose('Unpublished concern')
      await editor.comments(1)
      await editor.popup('Unpublished concern')
      const footer = await editor.value('vim.api.nvim_win_get_config(0).footer')

      // Act
      await editor.keys('x')

      // Assert
      expect(JSON.stringify(footer)).not.toContain('x resolve')
      expect((await editor.state())!.conversation.threads[0].state).toBe('open')
      expect(
        (await editor.notices()).some((notice) => /publish draft comments/i.test(notice.message)),
      ).toBe(true)
    })

    it('Deleting the final comment removes its card and shows request progress', async () => {
      // Arrange
      const { editor, github } = fixture()
      await selectBranchPr(editor, github)
      await editor.pane('new', 2)
      await editor.keys('c')
      await editor.compose('Remove this comment')
      await editor.comments(1)
      await editor.popup('Remove this comment')

      // Act
      await editor.choose('Delete')
      await editor.keys('dd')
      await editor.comments(0)
      const marks = await editor.marks()

      // Assert
      expect(marks).toHaveLength(0)
      expect(
        (await editor.notices()).some((notice) => notice.message === 'Deleting comment…'),
      ).toBe(true)
      expect(JSON.stringify((await editor.state())!.conversation)).not.toContain('comment deleted')
    })
  })

  describe(`When resuming an existing draft review [${profile}]`, { tags: [profile] }, () => {
    const fixture = usePrEditor(profile)

    it('Reopening the branch PR restores its pending cards and allows further comments', async () => {
      // Arrange
      const { editor, runtime, github } = fixture()
      const initial = await selectBranchPr(editor, github)
      await editor.pane('new', 2)
      await editor.keys('c')
      await editor.compose('Saved before restart')
      const saved = await editor.comments(1)
      await editor.command('Lgtm close')
      await editor.close()
      await runtime.restart()
      await editor.start()

      // Act
      const resumed = await selectBranchPr(editor, github, 'Lgtm review')
      const restored = await editor.marks()
      await editor.pane('new', 2)
      await editor.command('Lgtm comment file')
      await editor.compose('Added after restart')
      const current = await editor.comments(2)

      // Assert
      expect(resumed.review).toMatchObject({
        id: initial.review.id,
        forgeReviewId: saved!.review.forgeReviewId,
      })
      expect(JSON.stringify(restored)).toContain('Saved before restart')
      expect(current!.review.forgeReviewId).toBe(saved!.review.forgeReviewId)
      expect((await editor.notices()).some((notice) => notice.message === 'Review ready')).toBe(
        true,
      )
    })

    it('Drafts removed on GitHub disappear from the diff and exported Markdown', async () => {
      // Arrange
      const { editor, github } = fixture()
      await selectBranchPr(editor, github)
      await editor.pane('new', 2)
      await editor.keys('c')
      await editor.compose('Externally removed feedback')
      const saved = await editor.comments(1)
      await github.api(
        `repos/${github.repository}/pulls/comments/${saved!.conversation.comments[0].forgeCommentId}`,
        'DELETE',
      )

      // Act
      await editor.command('Lgtm refresh')
      await editor.comments(0)
      const marks = await editor.marks()
      await editor.choose('Preview')
      await editor.command('Lgtm export review')
      const markdown = await waitFor(
        'review export',
        () => editor.text(),
        (text) => text.includes('#'),
      )

      // Assert
      expect(marks).toHaveLength(0)
      expect(markdown).not.toContain('Externally removed feedback')
      expect(markdown).not.toContain('comment deleted')
    })
  })

  describe(`When the PR changes during review [${profile}]`, { tags: [profile] }, () => {
    const fixture = usePrEditor(profile)

    it('Refresh displays the new commit and retains the pending review and comment card', async () => {
      // Arrange
      const { editor, github } = fixture()
      await selectBranchPr(editor, github)
      await editor.pane('new', 2)
      await editor.keys('c')
      await editor.compose('Keep this concern through pushes')
      const original = await editor.comments(1)
      await github.changeFiles({ [github.path]: github.headContent + 'new committed context\n' })

      // Act
      await editor.command('Lgtm refresh')
      await editor.wait(
        `require("lgtm.context").current.head == "${github.head}" and require("lgtm.context").current.render_ready and not require("lgtm.context").current.initializing`,
        'new PR commit ready',
      )
      const refreshed = (await editor.state())!

      // Assert
      expect(refreshed.review).toMatchObject({
        id: original!.review.id,
        forgeReviewId: original!.review.forgeReviewId,
      })
      expect(await editor.text(refreshed.panes.new.buf)).toContain('new committed context')
      expect(JSON.stringify(await editor.marks())).toContain('Keep this concern through pushes')
    })
  })

  describe(`When refreshing a failed PR load [${profile}]`, { tags: [profile] }, () => {
    const fixture = usePrEditor(profile)

    it('Refresh recovers from authentication failure and re-enables commenting', async () => {
      // Arrange
      const { editor, runtime, github } = fixture()
      await editor.command('Lgtm prs')
      await editor.wait(
        'require("lgtm.picker").last and not require("lgtm.picker").last.closed',
        'PR picker loaded',
      )
      await editor.keys(`#${github.number} `)
      await editor.wait('#require("lgtm.picker").last.filtered == 1', 'branch PR selected')
      const token = runtime.env.GH_TOKEN
      runtime.env.GH_TOKEN = 'invalid-e2e-token'
      await runtime.restart()
      await editor.lua('vim.env.LGTM_RPC_URL = ...', runtime.endpoint)

      // Act
      await editor.keys('<CR>')
      await editor.wait(
        'require("lgtm.context").current and require("lgtm.context").current.load_error ~= nil',
        'failed initial PR load reported',
      )
      runtime.env.GH_TOKEN = token
      await runtime.restart()
      await editor.lua('vim.env.LGTM_RPC_URL = ...', runtime.endpoint)
      await editor.command('Lgtm refresh')
      const recovered = await editor.loaded()
      await editor.pane('new', 2)
      await editor.keys('c')
      await editor.compose('Comment after recovery')
      const saved = await editor.comments(1)

      // Assert
      expect(recovered.load_error).toBeFalsy()
      expect(saved!.conversation.comments[0].postingStatus).toBe('confirmed')
      expect(
        (await editor.notices()).some((notice) => notice.message.includes('still loading')),
      ).toBe(false)
    })
  })

  describe(`When GitHub marks a selection outdated [${profile}]`, { tags: [profile] }, () => {
    const fixture = usePrEditor(profile)

    it('An outdated card floats above the file even if matching text remains elsewhere', async () => {
      // Arrange
      const { editor, github } = fixture()
      await selectBranchPr(editor, github)
      await editor.pane('new', 2)
      await editor.keys('c')
      await editor.compose('Preserve original selection')
      await editor.comments(1)
      await github.changeFiles({
        [github.path]: github.headContent.replace('changed', 'replacement') + 'changed\n',
      })

      // Act
      await editor.command('Lgtm refresh')
      await editor.wait(
        `require("lgtm.context").current.head == "${github.head}" and not require("lgtm.context").current.initializing`,
        'outdated PR refreshed',
      )
      const state = await editor.loaded()
      const marks = await editor.marks()
      await editor.pane('new', 1)
      await editor.popup('Preserve original selection')
      const modal = await editor.text()

      // Assert
      expect(await editor.text(state.panes.new.buf)).toContain('changed')
      expect(state.conversation.threads[0].forgeObservation.outdated).toBe(true)
      expect(marks[0][1]).toBe(0)
      expect(marks[0][3].virt_lines_above).toBe(true)
      expect(JSON.stringify(marks)).toContain('Outdated')
      expect(modal).toContain('OUTDATED')
      expect(modal).toContain('changed')
    })
  })

  describe(`When submitting a draft review [${profile}]`, { tags: [profile] }, () => {
    const fixture = usePrEditor(profile)

    it('The submit shortcut uses the active PR and publishes a final message', async () => {
      // Arrange
      const { editor, github } = fixture()
      const initial = await selectBranchPr(editor, github)
      await editor.command('Lgtm comment general')
      await editor.compose('Overall editor feedback')
      await editor.comments(1)
      await editor.pane('new')

      // Act
      await editor.choose('comment')
      await editor.keys(' rs')
      const composer = await editor.composer()
      const title = await editor.value('vim.api.nvim_win_get_config(...).title', composer.win)
      await editor.compose('Final editor message')
      const submitted = (await editor.state())!
      const remote = await github.api(
        `repos/${github.repository}/pulls/${github.number}/reviews/${submitted.review.forgeReviewId}`,
      )

      // Assert
      expect(JSON.stringify(title)).toContain('Final review body')
      expect(submitted.review.id).toBe(initial.review.id)
      expect(submitted.review.submissionStatus).toBe('confirmed')
      expect(remote).toMatchObject({
        state: 'COMMENTED',
        body: expect.stringContaining('Final editor message'),
      })
      expect((await editor.notices()).some((notice) => notice.message === 'Review submitted')).toBe(
        true,
      )
    })

    it('A rejected self-review leaves the final message editable and allows a corrected outcome', async () => {
      // Arrange
      const { editor, github } = fixture()
      const initial = await selectBranchPr(editor, github)
      await editor.pane('new', 2)
      await editor.keys('c')
      await editor.compose('Retain this draft')
      await editor.comments(initial.conversation.comments.length + 1)

      // Act
      await editor.choose('request_changes')
      await editor.command('Lgtm submit')
      const composer = await editor.composer()
      await editor.body('Retain this final message')
      await editor.keys('<C-s>')
      await waitFor(
        'self-review explanation',
        () => editor.notices(),
        (notices) => notices.some((notice) => notice.message.includes('own PR')),
        40_000,
      )
      const rejected = (await editor.state())!
      const editable = await editor.value('vim.api.nvim_win_is_valid(...)', composer.win)
      const retainedText = await editor.text(composer.buf)
      await editor.keys('q')
      await editor.choose('comment')
      await editor.command('Lgtm submit')
      await editor.compose('Retain this final message')

      // Assert
      expect(editable).toBe(true)
      expect(retainedText).toBe('Retain this final message')
      expect(rejected.review.submissionStatus).toBe('unsubmitted')
      expect((await editor.state())!.review.submissionStatus).toBe('confirmed')
    })
  })

  describe(`When marking PR files as reviewed [${profile}]`, { tags: [profile] }, () => {
    const fixture = usePrEditor(profile)

    it('The mark shortcut toggles the checkmark in the native CodeDiff tree', async () => {
      // Arrange
      const { editor, github } = fixture()
      await selectBranchPr(editor, github)
      await editor.pane('new')

      // Act
      await editor.keys(' rm')
      await expectFileSeen(editor, github.path, true)
      await editor.keys(' rm')

      // Assert
      await expectFileSeen(editor, github.path, false)
    })
  })
  describe(`When addressing published comments [${profile}]`, { tags: [profile] }, () => {
    const fixture = usePrEditor(profile)

    async function openPublishedConcern() {
      const { editor, github } = fixture()
      const account = process.env.LGTM_E2E_GITHUB_SECONDARY_USER ?? 'skapoor81'
      const secondary = await githubCredentials(account)
      await github.api(
        `repos/${github.repository}/pulls/${github.number}/reviews`,
        'POST',
        {
          event: 'REQUEST_CHANGES',
          body: 'Please address this concern',
          comments: [
            {
              path: github.path,
              line: 2,
              side: 'RIGHT',
              body: 'Published concern from another reviewer',
            },
          ],
        },
        secondary,
      )
      const state = await selectBranchPr(editor, github)
      return { editor, github, account, state }
    }

    it('User sees the published author, adds a nested draft reply and submits a comment response', async () => {
      // Arrange
      const { editor, github, account } = await openPublishedConcern()
      await editor.pane('new', 2)

      // Act
      await editor.popup('Published concern from another reviewer')
      const discussion = await editor.text()
      await editor.keys('c')
      await editor.compose('I have addressed this concern')
      await editor.wait(
        'vim.iter(require("lgtm.context").current.conversation.comments):any(function(c) return c.body == "I have addressed this concern" and c.visibility == "draft" end)',
        'draft response displayed',
      )
      const draft = (await editor.state())!
      await editor.choose('comment')
      await editor.command('Lgtm submit')
      await editor.compose('Ready for another look')
      const submitted = (await editor.state())!
      const remote = await github.api(
        `repos/${github.repository}/pulls/${github.number}/reviews/${submitted.review.forgeReviewId}`,
      )

      // Assert
      expect(discussion).toContain(account)
      expect(discussion).toContain('Published concern from another reviewer')
      expect(
        draft.conversation.comments.some(
          (comment: { body: string; visibility: string }) =>
            comment.body === 'I have addressed this concern' && comment.visibility === 'draft',
        ),
      ).toBe(true)
      expect(remote).toMatchObject({
        state: 'COMMENTED',
        body: expect.stringContaining('Ready for another look'),
      })
    })

    it('Published threads expose resolution actions while another author’s comment remains protected', async () => {
      // Arrange
      const { editor, github } = await openPublishedConcern()
      await editor.pane('new', 2)
      await editor.popup('Published concern from another reviewer')
      const footer = await editor.value('vim.api.nvim_win_get_config(0).footer')

      // Act
      await editor.keys('e')
      const notices = await editor.notices()
      await editor.keys('x')
      const resolved = await waitFor(
        'published thread resolved',
        () => editor.state(),
        (state) =>
          state?.conversation.threads.some(
            (thread: { state: string }) => thread.state === 'resolved',
          ),
        40_000,
      )
      const thread = resolved!.conversation.threads.find(
        (thread: { state: string }) => thread.state === 'resolved',
      )
      const remote = await github.api('graphql', 'POST', {
        query: 'query($id:ID!){node(id:$id){... on PullRequestReviewThread{isResolved}}}',
        variables: { id: thread.forgeThreadId },
      })

      // Assert
      expect(JSON.stringify(footer)).toContain('x resolve')
      expect(notices.some((notice) => notice.message.includes('Only your own comments'))).toBe(true)
      expect(remote.data.node.isResolved).toBe(true)
      expect(
        (await editor.notices()).some((notice) => notice.message === 'Resolving thread…'),
      ).toBe(true)
    })
  })
}
