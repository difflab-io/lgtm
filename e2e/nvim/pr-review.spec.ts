/**
 * @module-tag e2e
 * @module-tag nvim
 * @module-tag github
 * @module-tag pr-review
 */
import { describe, it, expect, afterEach } from 'vitest'
import { profiles } from './fixtures.js'
import { usePrEditor, selectBranchPr } from './pr-review.fixtures.js'
import type { GithubFixture } from '../common/github.js'
import { waitFor, type Json } from '../common/test-utils.js'
import { expectFileSeen } from './assertions.js'

async function publishConcerns(
  github: GithubFixture,
  bodies = ['Published concern from another reviewer'],
) {
  const account = process.env.LGTM_E2E_GITHUB_SECONDARY_USER ?? 'skapoor81'
  await github.api(
    `repos/${github.repository}/pulls/${github.number}/reviews`,
    'POST',
    {
      event: 'REQUEST_CHANGES',
      body: 'Please address this concern',
      comments: bodies.map((body, index) => ({
        path: github.path,
        line: 2 + index,
        side: 'RIGHT',
        body,
      })),
    },
    await github.credentials(account),
  )
  return account
}

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
        'require("lgtm.ui.search").last and not require("lgtm.ui.search").last.closed and vim.api.nvim_get_current_buf() == require("lgtm.ui.search").last.buf and vim.fn.mode():sub(1,1) == "i"',
        'PR picker displayed',
      )
      const options = await editor.value('_G.e2e.last_select.items')
      const requestedInput = await editor.value('_G.e2e.last_input')
      await editor.keys(`#${github.number} `)
      await editor.wait('#require("lgtm.ui.search").last.filtered == 1', 'branch PR listed')
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

      // Act
      await editor.command('Lgtm prs')
      await editor.wait(
        'require("lgtm.ui.search").last and #require("lgtm.ui.search").last.filtered >= 2 and vim.fn.mode():sub(1,1) == "i"',
        'both branch PRs listed',
      )
      const first = await editor.value('require("lgtm.ui.search").last.index')
      await editor.keys('<Down>')
      const next = await editor.value('require("lgtm.ui.search").last.index')
      await editor.keys('<Up>')
      const previous = await editor.value('require("lgtm.ui.search").last.index')
      await editor.keys(`#${alternate.number} `)
      await editor.wait('#require("lgtm.ui.search").last.filtered == 1', 'typed PR filter')
      const selected = await editor.value('require("lgtm.ui.search").last.filtered[1].label')
      await editor.keys('<Esc>')

      // Assert
      expect(next).not.toBe(first)
      expect(previous).toBe(first)
      expect(selected).toContain(`#${alternate.number} `)
      expect(await editor.value('require("lgtm.ui.search").last.closed')).toBe(true)
      expect(await editor.state()).toBeNull()
    })
  })

  describe(`When creating draft comments [${profile}]`, { tags: [profile] }, () => {
    const fixture = usePrEditor(profile)

    it(
      'Lowercase c creates a remote draft shown as a full-width card with status and preview text',
      { tags: profile === 'required' ? ['smoke'] : [] },
      async () => {
        // Arrange
        const { editor, github } = fixture()
        await selectBranchPr(editor, github)
        await editor.pane('new', 2)

        // Act
        await editor.keys('c')
        await editor.composer()
        await editor.body('Please explain this line')
        const savedAt = performance.now()
        await editor.keys('<C-s>')
        const focus = await editor.value('vim.api.nvim_get_current_win()')
        const focusMs = performance.now() - savedAt

        // Assert
        expect(focus).toBe((await editor.state())!.panes.new.win)
        expect(focusMs, 'Ctrl-S returns to the diff without waiting for GitHub').toBeLessThan(500)
        await editor.keys('l')
        expect(await editor.value('vim.api.nvim_win_get_cursor(0)[2]')).toBe(1)

        // Act
        await editor.pane('old', 2)

        // Assert
        const state = await editor.comments(1)
        await editor.wait('require("lgtm.ui.composer").last.closed', 'draft save confirmed')
        expect(await editor.value('vim.api.nvim_get_current_win()')).toBe(state!.panes.old.win)
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
      },
    )

    it('User can save a file-level draft with a file card and no fabricated line number', async () => {
      // Arrange
      const { editor, github } = fixture()
      await selectBranchPr(editor, github)
      await editor.pane('new', 2)

      // Act
      await editor.command('Lgtm comment file')
      await editor.compose('Concern about the whole file')
      const state = await editor.comments(1)
      const marks = await editor.marks()
      const remote = await github.api('graphql', 'POST', {
        query:
          'query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){reviewThreads(first:100){nodes{subjectType line path comments(first:100){nodes{body state}}}}}}}',
        variables: {
          owner: github.repository.split('/')[0],
          name: github.repository.split('/')[1],
          number: github.number,
        },
      })
      await editor.pane('new', 1)
      await editor.popup('Concern about the whole file')
      const modal = await editor.text()

      // Assert
      expect(state!.conversation.threads[0].anchor).toMatchObject({
        kind: 'file',
        path: github.path,
      })
      expect(state!.conversation.threads[0].anchor.start ?? '0').toBe('0')
      expect(marks).toHaveLength(1)
      expect(marks[0][3].virt_lines_above).toBe(true)
      expect(JSON.stringify(marks)).toContain('File · GitHub draft')
      expect(JSON.stringify(marks)).toContain('Concern about the whole file')
      expect(modal).toContain('Concern about the whole file')
      expect(remote.data.repository.pullRequest.reviewThreads.nodes).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            subjectType: 'FILE',
            path: github.path,
            comments: {
              nodes: [
                expect.objectContaining({
                  body: expect.stringContaining('Concern about the whole file'),
                  state: 'PENDING',
                }),
              ],
            },
          }),
        ]),
      )
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
      const selected = await editor.value('require("lgtm.threads").last.selected')
      const text = await editor.text()

      // Assert
      expect(selected.role).toBe('top-level comment')
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
      const replySelection = await editor.value('require("lgtm.threads").last.selected')
      const discussion = await editor.text()
      await editor.keys('e')
      await editor.composer()
      const title = await editor.value('require("lgtm.ui.composer").last.title')
      await editor.compose('Edited reply')
      await editor.pane('new', 2)
      await editor.popup('Edited reply')
      await editor.choose('Delete')
      await editor.keys('dd')
      const remaining = await editor.comments(1)

      // Assert
      expect(replySelection.role).toMatch(/^reply /)
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
      const canResolve = await editor.value('require("lgtm.threads").last.selected.can_resolve')

      // Act
      await editor.keys('x')

      // Assert
      expect(canResolve).toBe(false)
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
    it('Resuming shows earlier published and resolved comments with their visible states', async () => {
      // Arrange
      const { editor, github, runtime } = fixture()
      await publishConcerns(github, ['Earlier resolved concern', 'Earlier open concern'])
      const initial = await selectBranchPr(editor, github)
      await editor.pane('new', 2)
      await editor.popup('Earlier resolved concern')
      await editor.keys('x')
      const resolved = await waitFor(
        'resolved concern',
        () => editor.state(),
        (state) =>
          state?.conversation.threads.some(
            (thread: { state: string }) => thread.state === 'resolved',
          ),
        40_000,
      )
      const threadId = resolved!.conversation.threads.find(
        (thread: { state: string }) => thread.state === 'resolved',
      ).id
      await editor.close()
      await runtime.restart()
      await editor.start()

      // Act
      const resumed = await selectBranchPr(editor, github)
      const cards = JSON.stringify(await editor.marks())
      await editor.pane('new', 2)
      await editor.popup('Earlier resolved concern')
      const modal = await editor.text()

      // Assert
      expect(resumed.review.id).toBe(initial.review.id)
      expect(resumed.conversation.threads).toEqual(
        expect.arrayContaining([
          expect.objectContaining({ id: threadId, state: 'resolved' }),
          expect.objectContaining({ state: 'open' }),
        ]),
      )
      for (const text of ['Earlier open concern', 'Published', 'Resolved'])
        expect(cards).toContain(text)
      expect(cards).not.toContain('Earlier resolved concern')
      expect(cards).toContain('✓')
      expect(modal.toUpperCase()).toContain('RESOLVED')
      expect(modal).toContain('Earlier resolved concern')
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
        'require("lgtm.ui.search").last and not require("lgtm.ui.search").last.closed and vim.api.nvim_get_current_buf() == require("lgtm.ui.search").last.buf and vim.fn.mode():sub(1,1) == "i"',
        'PR picker loaded',
      )
      await editor.keys(`#${github.number} `)
      await editor.wait('#require("lgtm.ui.search").last.filtered == 1', 'branch PR selected')
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

    it('Publishing keeps the diff open and further comments and replies start new pending reviews', async () => {
      // Arrange
      const { editor, github } = fixture()
      const initial = await selectBranchPr(editor, github)
      await editor.pane('new', 2)
      await editor.keys('c')
      await editor.compose('Inline editor feedback')
      await editor.comments(1)
      expect(JSON.stringify(await editor.marks())).toContain('GitHub draft')
      await editor.command('Lgtm comment general')
      await editor.compose('Overall editor feedback')
      await editor.comments(2)
      await editor.pane('new')

      // Act
      await editor.choose('comment')
      await editor.keys(' rs')
      await editor.composer()
      const title = await editor.value('require("lgtm.ui.composer").last.title')
      await editor.compose('Final editor message')
      const submitted = (await editor.state())!
      const remote = await github.api(
        `repos/${github.repository}/pulls/${github.number}/reviews/${submitted.review.forgeReviewId}`,
      )

      // Assert
      expect(JSON.stringify(title)).toContain('Final review body')
      expect(submitted.review.id).toBe(initial.review.id)
      expect(submitted.panes.new.buf).toBe(initial.panes.new.buf)
      expect(submitted.review.submissionStatus).toBe('confirmed')
      const cards = JSON.stringify(await editor.marks())
      expect(cards).toContain('Inline editor feedback')
      expect(cards).toContain('Published')
      expect(cards).not.toContain('GitHub draft')
      expect(
        submitted.conversation.comments.find(
          (comment: { body: string }) => comment.body === 'Inline editor feedback',
        ).visibility,
      ).toBe('published')
      expect(remote).toMatchObject({
        state: 'COMMENTED',
        body: expect.stringContaining('Final editor message'),
      })
      expect((await editor.notices()).some((notice) => notice.message === 'Review submitted')).toBe(
        true,
      )

      // Act
      await editor.pane('new', 2)
      await editor.keys('c')
      await editor.compose('More feedback after submission')
      const continued = await waitFor(
        'a new remote draft in the same PR diff',
        () => editor.state(),
        (state) =>
          state?.conversation.comments.some(
            (comment: Json) =>
              comment.body === 'More feedback after submission' && comment.forgeCommentId,
          ),
        40_000,
      )
      const pending = await github.api(
        `repos/${github.repository}/pulls/${github.number}/reviews/${continued!.review.forgeReviewId}`,
      )

      // Assert
      expect(continued!.review.id).not.toBe(submitted.review.id)
      expect(continued!.review.state).toBe('active')
      expect(continued!.scope.reviewId).toBe(continued!.review.id)
      expect(continued!.review.forgeReviewId).not.toBe(submitted.review.forgeReviewId)
      expect(continued!.panes.new.buf).toBe(initial.panes.new.buf)
      expect(pending.state).toBe('PENDING')
      expect(JSON.stringify(await editor.marks())).toContain('Published')
      expect(JSON.stringify(await editor.marks())).toContain('GitHub draft')
      expect(continued!.conversation.comments).toEqual(
        expect.arrayContaining([
          expect.objectContaining({ body: 'Inline editor feedback', visibility: 'published' }),
          expect.objectContaining({ body: 'More feedback after submission', visibility: 'draft' }),
        ]),
      )

      // Act
      await editor.choose('comment')
      await editor.keys(' rs')
      await editor.compose('Second published review')
      const second = (await editor.state())!
      await editor.pane('new', 2)
      await editor.popup('Inline editor feedback')
      await editor.keys('c')
      await editor.compose('Follow-up reply after publishing')
      const replied = await waitFor(
        'a new pending reply after the second submission',
        () => editor.state(),
        (state) =>
          state?.conversation.comments.some(
            (comment: Json) =>
              comment.body === 'Follow-up reply after publishing' && comment.forgeCommentId,
          ),
        40_000,
      )
      const previous = await github.api(
        `repos/${github.repository}/pulls/${github.number}/reviews/${submitted.review.forgeReviewId}`,
      )
      const replyDrafts = await github.api<Array<{ body: string; in_reply_to_id?: number }>>(
        `repos/${github.repository}/pulls/${github.number}/reviews/${replied!.review.forgeReviewId}/comments`,
      )

      // Assert
      expect(second.review.submissionStatus).toBe('confirmed')
      expect(replied!.review.id).not.toBe(second.review.id)
      expect(replied!.review.state).toBe('active')
      expect(replied!.panes.new.buf).toBe(initial.panes.new.buf)
      expect(previous).toMatchObject({
        state: 'COMMENTED',
        body: expect.stringContaining('Final editor message'),
      })
      expect(replyDrafts).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            body: expect.stringContaining('Follow-up reply after publishing'),
            in_reply_to_id: expect.any(Number),
          }),
        ]),
      )
      expect(
        (await editor.notices()).some((notice) =>
          notice.message.includes('Completed session metadata'),
        ),
      ).toBe(false)
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
      await editor.composer()
      await editor.body('Retain this final message')
      await editor.keys('<C-s>')
      await waitFor(
        'self-review explanation',
        () => editor.notices(),
        (notices) => notices.some((notice) => notice.message.includes('own PR')),
        40_000,
      )
      const rejected = (await editor.state())!
      const restored = await editor.composer()
      const editable = await editor.value('vim.api.nvim_win_is_valid(...)', restored.win)
      const retainedText = await editor.text(restored.buf)
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

    afterEach(async () => {
      const { github } = fixture()
      const response = await github.api('graphql', 'POST', {
        query:
          'query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){id files(first:100){nodes{path viewerViewedState}}}}}',
        variables: {
          owner: github.repository.split('/')[0],
          name: github.repository.split('/')[1],
          number: github.number,
        },
      })
      const pr = response.data.repository.pullRequest
      for (const file of pr.files.nodes)
        if (file.viewerViewedState === 'VIEWED') {
          await github.api('graphql', 'POST', {
            query:
              'mutation($input:UnmarkFileAsViewedInput!){unmarkFileAsViewed(input:$input){clientMutationId}}',
            variables: { input: { pullRequestId: pr.id, path: file.path } },
          })
        }
    })

    it('The mark shortcut silently toggles the checkmark in the native CodeDiff tree', async () => {
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
      expect((await editor.notices()).some((notice) => /File marked/.test(notice.message))).toBe(
        false,
      )
    })
    it('Viewed files survive editor and backend restarts and only changed files lose their marks', async () => {
      // Arrange
      const { editor, github, runtime } = fixture()
      const unchanged = `${github.run}-unchanged.txt`
      await github.changeFiles({ [unchanged]: 'unchanged reviewed file\n' })
      await github.fetch(runtime, github.head)
      const initial = await selectBranchPr(editor, github)
      for (const path of [github.path, unchanged]) {
        await editor.show(path)
        await editor.pane('new')
        await editor.keys(' rm')
        await expectFileSeen(editor, path, true)
      }
      await waitFor(
        'GitHub viewed-file state',
        () =>
          github.api('graphql', 'POST', {
            query:
              'query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){files(first:100){nodes{path viewerViewedState}}}}}',
            variables: {
              owner: github.repository.split('/')[0],
              name: github.repository.split('/')[1],
              number: github.number,
            },
          }),
        (response) =>
          [github.path, unchanged].every((path) =>
            response.data.repository.pullRequest.files.nodes.some(
              (file: { path: string; viewerViewedState: string }) =>
                file.path === path && file.viewerViewedState === 'VIEWED',
            ),
          ),
      )
      await editor.close()
      await runtime.restart()
      await editor.start()

      // Act
      const resumed = await selectBranchPr(editor, github)
      await waitFor(
        'resumed file checkmark',
        () => editor.state(),
        (state) => Boolean(state?.reviewed?.[github.path]),
      )

      // Assert
      expect(resumed.review.id).toBe(initial.review.id)
      await expectFileSeen(editor, github.path, true)

      // Act
      await github.changeFiles({
        [github.path]: github.headContent + 'changed after marking viewed\n',
      })
      await github.fetch(runtime, github.head)
      await editor.command('Lgtm refresh')
      await editor.wait(
        `require("lgtm.context").current.head == "${github.head}" and require("lgtm.context").current.render_ready`,
        'updated review comparison',
      )
      await waitFor(
        'changed file checkmark cleared',
        () => editor.state(),
        (state) => !state?.reviewed?.[github.path],
      )

      // Assert
      await expectFileSeen(editor, github.path, false)
      await expectFileSeen(editor, unchanged, true)
      expect((await editor.notices()).some((notice) => /File marked/.test(notice.message))).toBe(
        false,
      )
    })
  })

  describe(`When addressing published comments [${profile}]`, { tags: [profile] }, () => {
    const fixture = usePrEditor(profile)

    async function openPublishedConcern() {
      const { editor, github } = fixture()
      const account = await publishConcerns(github)
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
      const canResolve = await editor.value('require("lgtm.threads").last.selected.can_resolve')

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
      expect(canResolve).toBe(true)
      expect(notices.some((notice) => notice.message.includes('Only your own comments'))).toBe(true)
      expect(remote.data.node.isResolved).toBe(true)
      const cards = JSON.stringify(await editor.marks())
      expect(cards).not.toContain('Published concern')
      expect(cards).toContain('✓')
      expect(cards).toContain('Resolved')
      expect(cards).toContain('LgtmCommentResolved')
      await editor.pane('new', 2)
      await editor.popup('Published concern from another reviewer')
      const modal = await editor.text()
      expect(modal).toContain('Published concern from another reviewer')
      expect(modal.toUpperCase()).toContain('RESOLVED')
      expect(
        (await editor.notices()).some((notice) => notice.message === 'Resolving thread…'),
      ).toBe(true)
    })
  })
}
