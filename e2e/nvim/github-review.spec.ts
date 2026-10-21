import { describe, expect } from 'vitest'
import { profiles, suite } from './fixtures.js'
import { step, waitFor, type Json } from '../common/runtime.js'

// tests -----------------------------------------------------------------------

for (const profile of profiles) {
  const it = suite(profile)
  describe(`Neovim · GitHub comment and submission lifecycle · ${profile}`, () => {
    it('selects a branch PR, comments across scopes, preserves failed final text and reconciles uncertain publication', async ({
      editor: e,
      runtime: r,
      github: gh,
    }) => {
      // Arrange ---------------------------------------------------------------
      await r.git('init', '-q', '-b', 'main')

      await r.git('remote', 'add', 'origin', 'git@github.com:acme/repo.git')
      await r.write('local.txt', 'checkout remains unchanged\n')
      await r.commit('base')

      await step(
        'Given inferred origin and branch, Review offers working, staged, branch and PR; the searchable PR picker needs no repository prompt',
        async () => {
          // Act ---------------------------------------------------------------
          await e.choose('PR')
          await e.command('Lgtm review')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.picker").last and not require("lgtm.picker").last.closed',
            'PR search',
          )
          expect(await e.value('_G.e2e.last_select.items')).toEqual([
            'working',
            'staged',
            'branch',
            'PR',
          ])
          expect(await e.value('_G.e2e.last_input')).toBeNull()
          expect(await e.value('vim.api.nvim_win_get_width(require("lgtm.picker").last.win)')).toBe(
            76,
          )
          expect(
            await e.value('vim.api.nvim_win_get_height(require("lgtm.picker").last.list_win)'),
          ).toBe(10)

          // Act ---------------------------------------------------------------
          await e.keys('E2E pull 1')

          // Assert ------------------------------------------------------------
          await e.wait('#require("lgtm.picker").last.filtered==1', 'typed PR filter')

          // Act ---------------------------------------------------------------
          await e.keys('<CR>')

          // Arrange -----------------------------------------------------------
          const state = await e.loaded()

          // Assert ------------------------------------------------------------
          expect(Number(state.target.pullRequest.number)).toBe(1)
          expect(await e.text(state.panes.old.buf)).toContain('old')
          expect(await e.text(state.panes.new.buf)).toContain('new')
          for (const pane of Object.values(state.panes) as Json[]) {
            // Arrange ---------------------------------------------------------
            const name = await e.value<string>('vim.api.nvim_buf_get_name(...)', pane.buf)
            const kind = await e.value<string>('vim.bo[...].buftype', pane.buf)

            // Assert ----------------------------------------------------------
            expect(name.startsWith('codediff://') || kind === 'nofile').toBe(true)
          }
        },
      )

      await step(
        'When both diff sides, a file and the whole review receive feedback, inline Enter opens all threads at the selected line',
        async () => {
          // Arrange -----------------------------------------------------------
          for (const [side, body] of [
            ['old', 'Old-side inline feedback'],
            ['new', 'New-side inline feedback'],
            ['new', 'Second thread on the new line'],
          ]) {
            // Act -------------------------------------------------------------
            await e.pane(side, 2)
            await e.command('Lgtm comment line')
            await e.compose(body)
          }

          // Act ---------------------------------------------------------------
          await e.popup()

          // Assert ------------------------------------------------------------
          expect(await e.text()).toContain('New-side inline feedback')
          expect(await e.text()).toContain('Second thread on the new line')

          // Act ---------------------------------------------------------------
          await e.keys('q')
          await e.command('Lgtm comment file')
          await e.compose('Whole-file feedback')
          await e.command('Lgtm comment general')
          await e.compose('General feedback')

          // Assert ------------------------------------------------------------
          await e.comments(5)
          expect((await e.marks('old')).length).toBeGreaterThan(0)
          expect((await e.marks()).length).toBeGreaterThan(0)
          expect(gh.reviews[1].state).toBe('PENDING')

          // Act ---------------------------------------------------------------
          await e.choose('approve')
          await e.keys(' rs')
          await e.compose('Final approval body')

          // Assert ------------------------------------------------------------
          expect((await e.state())!.review.submissionStatus).toBe('confirmed')
          expect(gh.writes).toBe(7)
          expect((await e.notices()).some((n) => n.message === 'Review submitted')).toBe(true)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
        },
      )

      await step(
        'Then permission and stale-head errors retain the draft and final composer text until refresh permits an explicit retry',
        async () => {
          // Act ---------------------------------------------------------------
          await e.openPr(2)
          await e.command('Lgtm comment general')
          await e.compose('Retained draft on refresh failure')
          const before = (await e.notices()).length
          gh.control({ mode: 'denied' })
          await e.command('Lgtm refresh')

          // Assert ------------------------------------------------------------
          await waitFor(
            'permission notice',
            () => e.notices(),
            (notices) => notices.length > before + 1,
          )
          expect((await e.state())!.conversation.comments[0].body).toBe(
            'Retained draft on refresh failure',
          )

          // Act ---------------------------------------------------------------
          gh.control({ mode: 'stale' })
          await e.pane()
          await e.choose('request_changes')
          await e.keys(' rs')
          const composer = await e.composer()
          await e.body('Retained final body after stale head')
          const notices = (await e.notices()).length
          await e.keys('<C-s>')

          // Assert ------------------------------------------------------------
          await waitFor(
            'stale-head notice',
            () => e.notices(),
            (ns) => ns.length > notices + 1,
          )
          expect(await e.value('vim.api.nvim_win_is_valid(...)', composer.win)).toBe(true)
          expect(await e.text(composer.buf)).toBe('Retained final body after stale head')

          // Act ---------------------------------------------------------------
          await e.keys('q')

          // Assert ------------------------------------------------------------
          expect(gh.writes).toBe(7)

          // Act ---------------------------------------------------------------
          gh.control({ mode: 'normal' })
          const version = (await e.state())!.version
          await e.command('Lgtm refresh')

          // Assert ------------------------------------------------------------
          await waitFor(
            'recovered PR state',
            () => e.state(),
            (s) => Boolean(s?.version !== version && s?.render_ready),
          )
          expect(
            await e.value(
              'require("lgtm.review").submission_drafts[require("lgtm.context").current.review.id].body',
            ),
          ).toBe('Retained final body after stale head')

          // Act ---------------------------------------------------------------
          await e.pane()
          await e.choose('request_changes')
          await e.keys(' rs')
          await e.compose('Final request changes after refreshing the reviewed head')

          // Assert ------------------------------------------------------------
          expect((await e.state())!.review.submissionStatus).toBe('confirmed')
          expect(gh.reviews[2].state).toBe('CHANGES_REQUESTED')

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
        },
      )

      await step(
        'When GitHub accepts submission but loses its response, reconciliation is read-only and export includes the final body',
        async () => {
          // Act ---------------------------------------------------------------
          await e.openPr(3)
          gh.control({ mode: 'lost' })
          await e.choose('comment')
          await e.keys(' rs')
          const composer = await e.composer()
          await e.body('Uncertain final body')
          await e.keys('<C-s>')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.review.submissionStatus=="uncertain"',
            'durable uncertain submission',
          )
          expect(await e.value('vim.api.nvim_win_is_valid(...)', composer.win)).toBe(true)

          // Act ---------------------------------------------------------------
          await e.keys('q')
          gh.control({ mode: 'normal' })

          // Arrange -----------------------------------------------------------
          const writes = gh.writes

          // Act ---------------------------------------------------------------
          await e.command('Lgtm reconcile')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.review.submissionStatus=="confirmed"',
            'accepted submission confirmed',
          )
          expect(gh.writes).toBe(writes)
          const state = (await e.state())!
          expect((await r.rpc('Export', { scope: state.scope }, '')).markdown).toContain(
            'Uncertain final body',
          )
        },
      )
    })

    it('saves pending drafts remotely, shows progress immediately, recovers lost writes, removes deleted comments and corrects rejected outcomes', async ({
      editor: e,
      runtime: r,
      github: gh,
    }) => {
      // Arrange ---------------------------------------------------------------
      let pendingId: string, savedReviewId: string
      await step(
        'Given a PR, saving, editing and replying use the same private GitHub pending review',
        async () => {
          // Act ---------------------------------------------------------------
          await e.openPr(4)
          await e.keys('c')
          await e.compose('Pending draft before editor restart')

          // Arrange -----------------------------------------------------------
          const state = (await e.comments(1))!,
            saved = state.conversation.comments[0]

          // Assert ------------------------------------------------------------
          expect(saved).toMatchObject({ visibility: 'draft', postingStatus: 'confirmed' })
          expect(saved.forgeCommentId).toBeTruthy()
          expect(gh.reviews[4].state).toBe('PENDING')
          expect(gh.threads[4]).toHaveLength(1)
          expect(gh.comments[4] ?? []).toHaveLength(0)
          pendingId = state.review.forgeReviewId
          savedReviewId = state.review.id

          // Act ---------------------------------------------------------------
          await e.popup(saved.body)
          await e.keys('e')
          await e.compose('Edited pending draft')
          await e.popup('Edited pending draft')
          await e.keys('c')
          await e.compose('Pending reply to remove')

          // Assert ------------------------------------------------------------
          await e.comments(2)
        },
      )

      await step(
        'When resolve and delete wait for GitHub, progress appears before confirmation and only scoped requests are sent',
        async () => {
          // Arrange -----------------------------------------------------------
          gh.control({ reset_requests: true, pause_mutations: true })

          // Act ---------------------------------------------------------------
          await e.popup('Pending reply to remove')
          await e.keys('x')

          // Assert ------------------------------------------------------------
          await waitFor(
            'resolution progress',
            () => e.notices(),
            (notices) => notices.at(-1)?.message === 'Resolving thread…',
          )
          expect((await e.state())!.conversation.threads[0].state).toBe('open')

          // Act ---------------------------------------------------------------
          gh.control({ pause_mutations: false })

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.conversation.threads[1].state=="resolved"',
            'provider confirmed resolution',
          )
          expect(gh.requests).toHaveLength(1)
          expect(gh.requests[0].query).toContain('resolveReviewThread')

          // Act ---------------------------------------------------------------
          gh.control({ reset_requests: true, pause_mutations: true })
          await e.popup('Pending reply to remove')
          await e.choose('Delete')
          await e.keys('dd')

          // Assert ------------------------------------------------------------
          await waitFor(
            'deletion progress',
            () => e.notices(),
            (notices) => notices.at(-1)?.message === 'Deleting comment…',
          )
          expect((await e.state())!.conversation.comments).toHaveLength(2)

          // Act ---------------------------------------------------------------
          gh.control({ pause_mutations: false })

          // Assert ------------------------------------------------------------
          await e.comments(1)
          expect(gh.threads[4][0].comments.nodes).toHaveLength(1)
          expect(gh.requests).toHaveLength(2)
          expect(gh.requests[0].query).toContain('node(id:')
          expect(gh.requests[1].query).toContain('deletePullRequestReviewComment')

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
        },
      )

      await step(
        'Then restart resumes the same pending review and submission includes final text without reposting its saved draft',
        async () => {
          // Act ---------------------------------------------------------------
          await e.close()
          await r.restart()
          await e.start()
          const resumed = await e.openPr(4)

          // Assert ------------------------------------------------------------
          expect(resumed.review).toMatchObject({ id: savedReviewId, forgeReviewId: pendingId })
          expect(resumed.conversation.comments).toHaveLength(1)
          expect(resumed.conversation.comments[0]).toMatchObject({
            body: 'Edited pending draft',
            visibility: 'draft',
          })

          // Act ---------------------------------------------------------------
          await e.choose('comment')
          await e.keys(' rs')
          await e.compose('Final body after restart')

          // Assert ------------------------------------------------------------
          expect((await e.state())!.review).toMatchObject({
            forgeReviewId: pendingId,
            submissionStatus: 'confirmed',
          })
          expect(gh.reviews[4].state).toBe('COMMENTED')
          expect(gh.reviews[4].body).toContain('Final body after restart')
          expect(gh.threads[4]).toHaveLength(1)

          const before = (await e.state())!.conversation

          // Act ---------------------------------------------------------------
          await e.choose('Preview')
          await e.command('Lgtm export review')

          // Assert ------------------------------------------------------------
          await waitFor(
            'submitted preview',
            () => e.text(),
            (text) => text.startsWith('# Review feedback'),
          )
          for (const text of [
            '## Overall feedback',
            'Final body after restart',
            'Edited pending draft',
          ])
            expect(await e.text()).toContain(text)
          expect(await e.text()).not.toContain(savedReviewId)

          // Act ---------------------------------------------------------------
          await e.keys('q')

          // Assert ------------------------------------------------------------
          expect((await e.state())!.conversation).toEqual(before)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
        },
      )

      await step(
        'When a draft or pending-review creation response is lost, sync discovers the accepted object instead of duplicating it',
        async () => {
          // Act ---------------------------------------------------------------
          await e.openPr(5)
          gh.control({ mode: 'draft_lost' })
          await e.keys('c')
          await e.compose('Draft whose response was lost')

          // Assert ------------------------------------------------------------
          expect((await e.comments(1))!.conversation.comments[0].postingStatus).toBe('uncertain')

          // Act ---------------------------------------------------------------
          gh.control({ mode: 'normal' })

          // Arrange -----------------------------------------------------------
          let writes = gh.writes

          // Act ---------------------------------------------------------------
          await e.command('Lgtm sync')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.conversation.comments[1].postingStatus=="confirmed"',
            'draft receipt recovered',
          )
          expect(gh.writes).toBe(writes)

          // Act ---------------------------------------------------------------
          gh.control({ mode: 'denied' })
          await e.keys('c')
          await e.compose('Draft retained after permission failure')

          // Assert ------------------------------------------------------------
          await e.comments(2)

          // Act ---------------------------------------------------------------
          gh.control({ mode: 'normal' })
          await e.command('Lgtm sync')

          // Assert ------------------------------------------------------------
          await e.wait(
            'vim.iter(require("lgtm.context").current.conversation.comments):all(function(c) return c.postingStatus=="confirmed" end)',
            'denied draft retried explicitly',
          )
          expect(gh.threads[5]).toHaveLength(2)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
          await e.openPr(6)
          gh.control({ mode: 'review_lost' })
          await e.keys('c')
          await e.compose('Draft after pending review creation response was lost')

          // Assert ------------------------------------------------------------
          await e.comments(1)

          // Act ---------------------------------------------------------------
          gh.control({ mode: 'normal' })
          writes = gh.writes
          await e.command('Lgtm sync')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.conversation.comments[1].postingStatus=="confirmed"',
            'pending review receipt recovered',
          )
          expect(gh.writes).toBe(writes + 1)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
        },
      )

      await step(
        'Then externally discarded drafts disappear from cards, export and reopen without being recreated',
        async () => {
          // Act ---------------------------------------------------------------
          await e.openPr(7)
          await e.keys('c')
          await e.compose('Saved feedback after external discard')

          // Assert ------------------------------------------------------------
          await e.comments(1)

          // Act ---------------------------------------------------------------
          gh.control({ discard_pending: true, discard_pr: 7 })

          // Arrange -----------------------------------------------------------
          const writes = gh.writes

          // Act ---------------------------------------------------------------
          await e.command('Lgtm refresh')

          // Assert ------------------------------------------------------------
          await e.comments(0)
          expect((await e.state())!.conversation.threads).toHaveLength(0)
          expect(await e.marks()).toHaveLength(0)

          const before = (await e.state())!.conversation

          // Act ---------------------------------------------------------------
          await e.choose('Preview')
          await e.command('Lgtm export review')

          // Assert ------------------------------------------------------------
          await waitFor(
            'empty export',
            () => e.text(),
            (text) => text.startsWith('# Review feedback'),
          )
          expect(await e.text()).not.toContain('Saved feedback after external discard')

          // Act ---------------------------------------------------------------
          await e.keys('q')

          // Assert ------------------------------------------------------------
          expect((await e.state())!.conversation).toEqual(before)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
          await e.openPr(7)

          // Assert ------------------------------------------------------------
          expect((await e.state())!.conversation.comments).toHaveLength(0)
          expect((await e.state())!.conversation.threads).toHaveLength(0)
          expect(gh.writes).toBe(writes)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
        },
      )

      await step(
        'When a remote root is deleted, surviving replies keep their role and accept more replies until the final deletion removes the card',
        async () => {
          // Act ---------------------------------------------------------------
          await e.openPr(8)
          await e.keys('c')
          await e.compose('Original comment to delete')

          // Assert ------------------------------------------------------------
          await e.comments(1)

          // Act ---------------------------------------------------------------
          await e.popup('Original comment to delete')
          await e.keys('c')
          await e.compose('Surviving reply')

          // Assert ------------------------------------------------------------
          await e.comments(2)

          // Act ---------------------------------------------------------------
          await e.popup('Original comment to delete')
          await e.choose('Delete')
          await e.keys('dd')

          // Assert ------------------------------------------------------------
          await e.comments(1)
          for (let reopen = 0; reopen < 2; reopen++) {
            // Assert ----------------------------------------------------------
            expect((await e.state())!.conversation.comments[0].body).toBe('Surviving reply')
            const marks = JSON.stringify(await e.marks())
            expect(marks).toContain('Surviving reply')
            expect(marks).not.toContain('Original comment to delete')
            expect(marks).not.toContain('comment deleted')
            if (!reopen) {
              // Act -----------------------------------------------------------
              await e.command('Lgtm close')
              await e.openPr(8)
            }
          }

          // Act ---------------------------------------------------------------
          await e.popup('Surviving reply')
          await e.keys('c')
          await e.compose('Reply after original deletion')

          // Assert ------------------------------------------------------------
          await e.comments(2)
          const newest = (await e.state())!.conversation.comments[1]
          expect(newest.postingStatus).toBe('confirmed')
          expect(newest.forgeCommentId).toBeTruthy()

          // Act ---------------------------------------------------------------
          await e.popup('Reply after original deletion')
          await e.choose('Delete')
          await e.keys('dd')

          // Assert ------------------------------------------------------------
          await e.comments(1)

          // Act ---------------------------------------------------------------
          await e.popup('Surviving reply')

          // Assert ------------------------------------------------------------
          const text = await e.text()

          expect(text).toContain('REPLY 1')
          expect(text).not.toContain('REPLY 0')
          expect(text).not.toContain('Top-level comment deleted')

          // Act ---------------------------------------------------------------
          await e.choose('Delete')
          await e.keys('dd')

          // Assert ------------------------------------------------------------
          await e.comments(0)
          expect((await e.state())!.conversation.threads).toHaveLength(0)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
        },
      )

      await step(
        'Then invalid self-reviews and definite final-event rejection retain editable drafts and final text for a corrected Comment submission',
        async () => {
          // Act ---------------------------------------------------------------
          await e.openPr(9)
          await e.keys('c')
          await e.compose('Pending feedback during rejected review')
          const original = (await e.state())!
          gh.control({ self_pr: 9 })

          // Arrange -----------------------------------------------------------
          const writes = gh.writes

          // Act ---------------------------------------------------------------
          await e.choose('request_changes')
          await e.keys(' rs')
          let composer = await e.composer()

          // Arrange -----------------------------------------------------------
          const body = 'Keep the final comment after rejection'

          // Act ---------------------------------------------------------------
          await e.body(body)
          await e.keys('<C-s>')

          // Assert ------------------------------------------------------------
          await waitFor(
            'self-review explanation',
            () => e.notices(),
            (notices) => notices.some((n) => n.message.includes('own PR')),
          )
          expect(await e.value('vim.api.nvim_win_is_valid(...)', composer.win)).toBe(true)
          expect((await e.state())!.review).toMatchObject({
            state: 'active',
            submissionStatus: 'unsubmitted',
          })
          expect(gh.writes).toBe(writes)

          // Act ---------------------------------------------------------------
          await e.keys('q')
          gh.control({ self_pr: 0, reject_next_event: 422 })
          await e.choose('approve')
          await e.keys(' rs')

          // Assert ------------------------------------------------------------
          composer = await e.composer()

          expect(await e.text(composer.buf)).toBe(body)

          // Act ---------------------------------------------------------------
          await e.keys('<C-s>')

          // Assert ------------------------------------------------------------
          await waitFor(
            'editable definite rejection',
            () => e.state(),
            (s) => Boolean(s?.review.body === body && s.review.submissionStatus === 'unsubmitted'),
          )
          expect(await e.value('vim.api.nvim_win_is_valid(...)', composer.win)).toBe(true)
          expect((await e.state())!.review).toMatchObject({
            id: original.review.id,
            forgeReviewId: original.review.forgeReviewId,
            state: 'active',
          })
          expect((await e.state())!.conversation.comments[0].visibility).toBe('draft')

          // Act ---------------------------------------------------------------
          await e.keys('q')
          await e.command('Lgtm close')
          const reopened = await e.openPr(9)

          // Assert ------------------------------------------------------------
          expect(reopened.review.body).toBe(body)

          // Act ---------------------------------------------------------------
          await e.choose('comment')
          await e.keys(' rs')

          // Assert ------------------------------------------------------------
          composer = await e.composer()

          expect(await e.text(composer.buf)).toBe(body)

          // Act ---------------------------------------------------------------
          await e.compose(body)

          // Assert ------------------------------------------------------------
          expect((await e.state())!.review).toMatchObject({
            submissionStatus: 'confirmed',
            forgeReviewId: original.review.forgeReviewId,
          })
          expect(gh.threads[9]).toHaveLength(1)
        },
      )
    })
  })
}
