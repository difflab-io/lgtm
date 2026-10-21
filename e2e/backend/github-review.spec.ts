import { describe, expect } from 'vitest'
import { access, rm } from 'node:fs/promises'
import { join } from 'node:path'
import { suite } from '../common/fixtures.js'
import { hash, step, type Json, type Runtime } from '../common/runtime.js'

// fixtures --------------------------------------------------------------------

const it = suite('backend')
const sha = (character: string) => character.repeat(40)
const target = (number: number) => ({
  pullRequest: { repositoryUrl: 'https://github.com/acme/repo', number: String(number) },
})
const conversation = (r: Runtime, id: string) =>
  r.rpc('GetConversation', { scope: { reviewId: id } })
const current = async (r: Runtime, id: string) =>
  (await r.rpc('GetReview', { reviewId: id })).review
async function add(r: Runtime, id: string, body: string, anchor: Json, key: string) {
  const before = await conversation(r, id)
  return r.rpc('AddComment', {
    scope: { reviewId: id },
    body,
    anchor,
    expectedVersion: before.version,
    requestKey: key,
  })
}
async function refresh(r: Runtime, id: string, key: string) {
  return r.rpc('RefreshReview', {
    scope: { reviewId: id },
    expectedVersion: (await conversation(r, id)).version,
    requestKey: key,
  })
}
async function submission(
  r: Runtime,
  id: string,
  key: string,
  outcome = 'approve',
  body = 'Final E2E body',
) {
  const review = await current(r, id),
    comparison = review.comparison.pullRequest
  return {
    reviewId: id,
    outcome,
    body,
    requestKey: key,
    expectedVersion: review.version,
    expectedBase: comparison.baseCommit,
    expectedHead: comparison.headCommit,
  }
}

// tests -----------------------------------------------------------------------

describe('Review API · GitHub is the durable home of PR drafts', () => {
  it('browses without checkout, manages pending discussions, and submits once despite permission errors or lost responses', async ({
    runtime: r,
    github: gh,
  }) => {
    // Arrange -----------------------------------------------------------------
    gh.control({ base: sha('a'), head: sha('b') })

    await r.identify()

    // Assert ------------------------------------------------------------------
    expect(
      (
        await r.rpc('ListPullRequests', {
          repositoryUrl: 'https://github.com/acme/repo',
          query: 'E2E',
        })
      ).pullRequests,
    ).toHaveLength(3)

    // Arrange -----------------------------------------------------------------
    const start = { target: target(1), requestKey: 'start-pr' }

    const review = (await r.rpc('StartReview', start)).review,
      id = review.id,
      scope = { reviewId: id }

    await step(
      'Given a PR selection, start reads conversation metadata before downloading any file bodies',
      async () => {
        // Assert --------------------------------------------------------------
        expect(review.comparison.pullRequest).toEqual({
          baseCommit: sha('a'),
          headCommit: sha('b'),
        })
        expect(review.forgeState).toBe('open')
        expect(gh.contentReads).toBe(0)
        await expect(access(join(r.home, '.git'))).rejects.toThrow()

        // Act -----------------------------------------------------------------
        gh.control({ mode: 'denied' })
        const resumedReview = await r.rpc('StartReview', start)

        // Assert --------------------------------------------------------------
        expect(resumedReview.review.id).toBe(id)

        // Act -----------------------------------------------------------------
        gh.control({ mode: 'normal' })
      },
    )

    await step(
      'When selected remote content is requested, line, file and general comments retain their intended scope',
      async () => {
        // Assert --------------------------------------------------------------
        const diff = (await r.rpc('GetDiff', { target: target(1), comparison: review.comparison }))
          .diff

        expect((await r.rpc('GetFiles', { diff })).files.length).toBeGreaterThan(0)
        const file = await r.rpc('GetFile', { diff, path: 'review.txt' })
        expect(file.oldSide.content).toContain('old')
        expect(file.newSide.content).toContain('new')

        // Arrange -------------------------------------------------------------
        const anchor = {
          kind: 'line',
          path: 'review.txt',
          side: 'new',
          start: '2',
          end: '2',
          source: file.newSide.source,
          evidence: { selectedText: 'new', contextBefore: 'one', contextAfter: 'three' },
        }

        // Act -----------------------------------------------------------------
        await add(r, id, 'General remote concern', { kind: 'general' }, 'general')
        await add(r, id, 'Inline remote concern', anchor, 'line')
        await add(
          r,
          id,
          'File remote concern',
          { kind: 'file', path: 'review.txt', source: file.newSide.source, evidence: {} },
          'file',
        )

        // Assert --------------------------------------------------------------
        expect(gh.reviews[1].state).toBe('PENDING')
        expect((await conversation(r, id)).comments).toHaveLength(3)
      },
    )

    await step(
      'When GitHub denies refresh or rate-limits it, saved feedback remains readable and no writes are replayed',
      async () => {
        // Arrange -------------------------------------------------------------
        const writes = gh.writes
        for (const mode of ['denied', 'limited']) {
          // Arrange -----------------------------------------------------------
          gh.control({ mode })

          // Assert ------------------------------------------------------------
          const result = await r.response('RefreshReview', {
            scope,
            expectedVersion: (await conversation(r, id)).version,
            requestKey: `refresh-${mode}`,
          })

          expect(result.status).toBeGreaterThanOrEqual(400)
          expect(result.body.message).toEqual(expect.any(String))
          expect((await conversation(r, id)).comments).toHaveLength(3)
          expect(gh.writes).toBe(writes)
        }

        gh.control({ mode: 'normal' })

        // Act -----------------------------------------------------------------
        await refresh(r, id, 'refresh-recovered')
      },
    )

    await step(
      'Then edits and deletions are idempotent, resolution takes one provider request, and deleting a root leaves its replies usable',
      async () => {
        // Arrange -------------------------------------------------------------
        let snapshot = await conversation(r, id)
        const root = snapshot.comments.find((c: Json) => c.body === 'File remote concern')
        const edit = {
          scope,
          commentId: root.id,
          body: 'Edited remote feedback',
          expectedVersion: root.version,
          requestKey: 'edit',
        }

        // Act -----------------------------------------------------------------
        await r.rpc('EditComment', edit)

        // Arrange -------------------------------------------------------------
        let writes = gh.writes

        // Act -----------------------------------------------------------------
        await r.rpc('EditComment', edit)

        // Assert --------------------------------------------------------------
        expect(gh.writes).toBe(writes)

        snapshot = await conversation(r, id)

        // Arrange -------------------------------------------------------------
        const thread = snapshot.threads.find((t: Json) => t.id === root.threadId)

        // Act -----------------------------------------------------------------
        const reply = (
          await r.rpc('Reply', {
            scope,
            threadId: thread.id,
            body: 'Surviving remote reply',
            expectedVersion: thread.version,
            requestKey: 'reply',
          })
        ).comment
        snapshot = await conversation(r, id)
        gh.control({ reset_requests: true })
        await r.rpc('ResolveThread', {
          scope,
          threadId: thread.id,
          resolved: true,
          expectedVersion: snapshot.threads.find((t: Json) => t.id === thread.id).version,
          requestKey: 'resolve',
        })

        // Assert --------------------------------------------------------------
        expect(gh.requests).toHaveLength(1)
        expect(gh.requests[0].query).toContain('resolveReviewThread(')

        snapshot = await conversation(r, id)

        // Act -----------------------------------------------------------------
        gh.control({ reset_requests: true })

        // Arrange -------------------------------------------------------------
        const deletion = {
          scope,
          commentId: root.id,
          expectedVersion: snapshot.comments.find((c: Json) => c.id === root.id).version,
          requestKey: 'delete-root',
        }

        // Act -----------------------------------------------------------------
        await r.rpc('DeleteComment', deletion)

        // Assert --------------------------------------------------------------
        expect(gh.requests).toHaveLength(2)
        expect(gh.requests[0].query).toContain('node(id:')
        expect(gh.requests[1].query).toContain('deletePullRequestReviewComment(')
        writes = gh.writes

        // Act -----------------------------------------------------------------
        await r.restart()
        await r.rpc('DeleteComment', deletion)

        // Assert --------------------------------------------------------------
        expect(gh.requests).toHaveLength(2)
        expect(gh.writes).toBe(writes)

        // Act -----------------------------------------------------------------
        await refresh(r, id, 'refresh-after-delete')

        // Assert --------------------------------------------------------------
        snapshot = await conversation(r, id)

        expect(snapshot.comments.map((c: Json) => c.id)).not.toContain(root.id)
        expect(snapshot.comments.find((c: Json) => c.id === reply.id).body).toBe(
          'Surviving remote reply',
        )

        // Act -----------------------------------------------------------------
        const continued = await r.rpc('Reply', {
          scope,
          threadId: thread.id,
          body: 'Continue the surviving thread',
          expectedVersion: snapshot.threads.find((t: Json) => t.id === thread.id).version,
          requestKey: 'reply-after-delete',
        })

        // Assert --------------------------------------------------------------
        expect(continued.syncError).toBeFalsy()
        expect(continued.comment.forgeCommentId).toBeTruthy()
        const markdown = (await r.rpc('Export', { scope, commentIds: [reply.id] })).markdown
        expect(markdown).toContain('Surviving remote reply')
        expect(markdown).not.toContain('Edited remote feedback')
        expect(markdown).not.toContain('comment deleted')

        snapshot = await conversation(r, id)

        // Arrange -------------------------------------------------------------
        const removable = snapshot.comments.find(
          (c: Json) => c.body === 'Continue the surviving thread',
        )
        const removeReply = {
          scope,
          commentId: removable.id,
          expectedVersion: removable.version,
          requestKey: 'delete-reply',
        }

        // Act -----------------------------------------------------------------
        await r.rpc('DeleteComment', removeReply)
        writes = gh.writes
        await r.rpc('DeleteComment', removeReply)

        // Assert --------------------------------------------------------------
        expect(gh.writes).toBe(writes)
      },
    )

    await step(
      'When a final response is lost, restart and reconciliation confirm the same submission without another write',
      async () => {
        // Assert --------------------------------------------------------------
        const stale = { ...(await submission(r, id, 'stale')), expectedVersion: '0' },
          before = gh.writes

        await r.reject('SubmitReview', stale)
        expect(gh.writes).toBe(before)
        expect((await current(r, id)).state).toBe('active')

        const request = await submission(r, id, 'submit-once')

        // Act -----------------------------------------------------------------
        gh.control({ mode: 'lost' })

        const uncertainSubmission = await r.rpc('SubmitReview', request)

        // Assert --------------------------------------------------------------
        expect(uncertainSubmission.review.submissionStatus).toBe('uncertain')
        expect(gh.reviews[1]).toMatchObject({ state: 'APPROVED' })
        expect(gh.reviews[1].body).toContain('Final E2E body')

        // Arrange -------------------------------------------------------------
        const writes = gh.writes

        gh.control({ mode: 'normal' })

        // Act -----------------------------------------------------------------
        await r.restart()
        const reconciliation = await r.rpc('ReconcileSubmission', { reviewId: id })

        // Assert --------------------------------------------------------------
        expect(reconciliation.review.submissionStatus).toBe('confirmed')

        // Act -----------------------------------------------------------------
        await r.rpc('SubmitReview', request)

        // Assert --------------------------------------------------------------
        expect(gh.writes).toBe(writes)
        const markdown = (await r.rpc('Export', { scope })).markdown
        expect(markdown).toContain('Final E2E body')
        expect(markdown).toContain('Inline remote concern')
      },
    )

    await step('And a normal final response confirms directly and remains idempotent', async () => {
      // Act -------------------------------------------------------------------
      const next = (await r.rpc('StartReview', { target: target(2), requestKey: 'normal-start' }))
        .review
      await add(
        r,
        next.id,
        'Normal feedback',
        { kind: 'file', path: 'review.txt' },
        'normal-feedback',
      )
      const request = await submission(r, next.id, 'normal-submit')
      const normalSubmission = await r.rpc('SubmitReview', request)

      // Assert ----------------------------------------------------------------
      expect(normalSubmission.review.submissionStatus).toBe('confirmed')

      // Arrange ---------------------------------------------------------------
      const writes = gh.writes

      // Act -------------------------------------------------------------------
      await r.rpc('SubmitReview', request)

      // Assert ----------------------------------------------------------------
      expect(gh.writes).toBe(writes)
    })
  })

  it('follows PR pushes and lifecycle changes, safely saves old selections, and recovers rejected outcomes', async ({
    runtime: r,
    github: gh,
  }) => {
    // Arrange -----------------------------------------------------------------
    gh.control({ base: sha('a'), head: sha('b') })

    await r.identify()
    const review = (await r.rpc('StartReview', { target: target(1), requestKey: 'start' })).review,
      id = review.id
    const saved = (
      await add(r, id, 'Pending feedback to preserve', { kind: 'file', path: 'review.txt' }, 'file')
    ).comment
    const pending = (await current(r, id)).forgeReviewId

    await step(
      'Given pending feedback, refresh, reopen and later edits follow pushes without replacing the review or pending drafts',
      async () => {
        // Assert --------------------------------------------------------------
        expect(saved.forgeCommentId).toBeTruthy()

        // Arrange -------------------------------------------------------------
        const writes = gh.writes

        gh.control({ head: sha('c') })

        // Act -----------------------------------------------------------------
        await refresh(r, id, 'push')

        // Assert --------------------------------------------------------------
        expect(await current(r, id)).toMatchObject({
          id,
          forgeReviewId: pending,
          comparison: { pullRequest: { headCommit: sha('c') } },
        })

        // Act -----------------------------------------------------------------
        gh.control({ base: sha('d'), head: sha('e') })
        const reopened = (await r.rpc('StartReview', { target: target(1), requestKey: 'reopen' }))
          .review

        // Assert --------------------------------------------------------------
        expect(reopened).toMatchObject({
          id,
          forgeReviewId: pending,
          comparison: { pullRequest: { baseCommit: sha('d'), headCommit: sha('e') } },
        })
        expect(gh.writes).toBe(writes)
        expect((await conversation(r, id)).comments[0]).toMatchObject({
          id: saved.id,
          visibility: 'draft',
        })

        // Act -----------------------------------------------------------------
        await add(r, id, 'Feedback after a push', { kind: 'file', path: 'review.txt' }, 'next')

        // Assert --------------------------------------------------------------
        expect((await current(r, id)).forgeReviewId).toBe(pending)
        expect(Object.values(gh.reviews)).toHaveLength(1)

        const root = (await conversation(r, id)).comments.find((c: Json) => c.id === saved.id)

        // Act -----------------------------------------------------------------
        gh.control({ head: sha('f') })

        await r.rpc('EditComment', {
          scope: { reviewId: id },
          commentId: saved.id,
          body: 'Edited after another push',
          expectedVersion: root.version,
          requestKey: 'edit-after-push',
        })

        // Assert --------------------------------------------------------------
        expect((await current(r, id)).comparison.pullRequest.headCommit).toBe(sha('f'))

        const request = await submission(r, id, 'unseen-head')

        // Act -----------------------------------------------------------------
        gh.control({ head: sha('1') })

        // Arrange -------------------------------------------------------------
        const before = gh.writes

        // Assert --------------------------------------------------------------
        await r.reject('SubmitReview', request)
        expect(gh.writes).toBe(before)
      },
    )

    await step(
      'When the PR closes, merges or reopens, its feedback remains readable and its local attempt stays active',
      async () => {
        // Arrange -------------------------------------------------------------
        for (const mode of ['closed', 'merged', 'normal']) {
          // Arrange -----------------------------------------------------------
          gh.control({ mode })

          // Act ---------------------------------------------------------------
          await refresh(r, id, `lifecycle-${mode}`)

          // Assert ------------------------------------------------------------
          expect(await current(r, id)).toMatchObject({
            id,
            state: 'active',
            forgeState: mode === 'normal' ? 'open' : mode,
          })

          // Act ---------------------------------------------------------------
          const reopenedReview = await r.rpc('StartReview', {
            target: target(1),
            requestKey: 'start',
          })

          // Assert ------------------------------------------------------------
          expect(reopenedReview.review.id).toBe(id)
          expect((await r.rpc('Export', { scope: { reviewId: id } })).markdown).toContain(
            'Edited after another push',
          )
        }
      },
    )

    await step(
      'Then self-approval and self-request-changes fail before writes, while an explicit final rejection preserves editable drafts and text',
      async () => {
        // Arrange -------------------------------------------------------------
        gh.control({ self_pr: 1 })
        const writes = gh.writes
        for (const outcome of ['approve', 'request_changes']) {
          // Assert ------------------------------------------------------------
          const result = await r.response(
            'SubmitReview',
            await submission(r, id, outcome, outcome, 'Keep final text'),
          )

          expect(result.status).toBeGreaterThanOrEqual(400)
          expect(result.body.message).toContain('own PR')
          expect((await current(r, id)).state).toBe('active')
          expect(gh.writes).toBe(writes)
        }
        gh.control({ self_pr: 0, reject_next_event: 422 })
        const request = await submission(r, id, 'rejected', 'request_changes', 'Keep final text')

        // Act -----------------------------------------------------------------
        const rejectedSubmission = await r.rpc('SubmitReview', request)

        // Assert --------------------------------------------------------------
        expect(rejectedSubmission.review).toMatchObject({
          state: 'active',
          submissionStatus: 'unsubmitted',
          body: 'Keep final text',
          forgeReviewId: pending,
        })
        expect((await conversation(r, id)).comments[0].visibility).toBe('draft')

        // Act -----------------------------------------------------------------
        await r.restart()

        // Assert --------------------------------------------------------------
        expect((await current(r, id)).body).toBe('Keep final text')

        const before = gh.writes

        await r.reject('SubmitReview', request)

        // Act -----------------------------------------------------------------
        await r.rpc('ReconcileSubmission', { reviewId: id })

        // Assert --------------------------------------------------------------
        expect(gh.writes).toBe(before)

        // Act -----------------------------------------------------------------
        await add(
          r,
          id,
          'More feedback after rejection',
          { kind: 'file', path: 'review.txt' },
          'after-rejection',
        )

        // Assert --------------------------------------------------------------
        const corrected = (
          await r.rpc(
            'SubmitReview',
            await submission(r, id, 'corrected', 'comment', 'Keep final text'),
          )
        ).review

        expect(corrected).toMatchObject({ submissionStatus: 'confirmed', forgeReviewId: pending })
        expect(gh.reviews[1].state).toBe('COMMENTED')
      },
    )

    await step(
      'When a push lands during draft preflight, the bounded observation retry creates exactly one pending review and comment',
      async () => {
        // Arrange -------------------------------------------------------------
        gh.control({ base: sha('a'), head: sha('b') })

        // Act -----------------------------------------------------------------
        const next = (await r.rpc('StartReview', { target: target(2), requestKey: 'racing-start' }))
          .review
        gh.control({ advance_on_pr_read: 2, next_head: sha('c') })

        // Arrange -------------------------------------------------------------
        const writes = gh.writes

        // Act -----------------------------------------------------------------
        const saved = await add(
          r,
          next.id,
          'Feedback during a push',
          { kind: 'file', path: 'review.txt' },
          'racing-preflight',
        )

        // Assert --------------------------------------------------------------
        expect(saved.syncError).toBeFalsy()
        expect(saved.comment.postingStatus).toBe('confirmed')
        expect((await current(r, next.id)).comparison.pullRequest.headCommit).toBe(sha('c'))
        expect(gh.writes - writes).toBe(2)
      },
    )

    await step(
      'Then a lost pending-creation response survives a push and backend restart without creating a second review',
      async () => {
        // Act -----------------------------------------------------------------
        const next = (
          await r.rpc('StartReview', { target: target(3), requestKey: 'lost-pending-start' })
        ).review
        gh.control({ mode: 'review_lost' })

        // Arrange -------------------------------------------------------------
        const writes = gh.writes

        // Act -----------------------------------------------------------------
        const saved = await add(
          r,
          next.id,
          'Recover this private draft',
          { kind: 'file', path: 'review.txt' },
          'lost-pending',
        )

        // Assert --------------------------------------------------------------
        expect(saved.syncError).toBeTruthy()

        // Arrange -------------------------------------------------------------
        const pending = gh.reviews[3].id

        gh.control({ head: sha('d') })

        // Act -----------------------------------------------------------------
        await r.restart()

        // Arrange -------------------------------------------------------------
        const request = { reviewId: next.id, commentId: saved.comment.id }

        // Act -----------------------------------------------------------------
        const synced = await r.rpc('SyncDraftComment', request)

        // Assert --------------------------------------------------------------
        expect(synced.comment.postingStatus).toBe('confirmed')
        expect((await current(r, next.id)).comparison.pullRequest.headCommit).toBe(sha('d'))
        expect(gh.writes - writes).toBe(2)
        expect(gh.reviews[3].id).toBe(pending)

        // Arrange -------------------------------------------------------------
        const before = gh.writes

        // Act -----------------------------------------------------------------
        const replayedSync = await r.rpc('SyncDraftComment', request)

        // Assert --------------------------------------------------------------
        expect(replayedSync.comment.forgeCommentId).toBe(synced.comment.forgeCommentId)
        expect(gh.writes).toBe(before)
      },
    )
  })

  it('validates exact selected bytes, relocates old composer selections, and promotes agent feedback with human ownership', async ({
    runtime: r,
    github: gh,
  }) => {
    // Arrange -----------------------------------------------------------------
    await r.identify()
    let number = 10
    await step(
      'Given local or remote PR sources, a wrong hash changes nothing and accepted retries need no surviving source',
      async () => {
        // Arrange -------------------------------------------------------------
        for (const local of [false, true]) {
          // Arrange -----------------------------------------------------------
          let base = sha('a'),
            head = sha('b')
          if (local) {
            // Act -------------------------------------------------------------
            await r.git('init', '-q')
            await r.write('review.txt', 'one\nold\nthree\n')
            base = await r.commit('base')
            await r.write('review.txt', 'one\nnew\nthree\n')
            head = await r.commit('head')
            await r.git('checkout', '--detach', '-q', head)
          }

          gh.control({ base, head })
          const n = number++

          // Act ---------------------------------------------------------------
          const review = (
            await r.rpc('StartReview', {
              target: target(n),
              requestKey: `start-${n}`,
              ...(local ? { localGitPath: r.workspace } : {}),
            })
          ).review

          // Arrange -----------------------------------------------------------
          const scope = { reviewId: review.id },
            reads = gh.contentReads
          const anchor = {
            kind: 'line',
            path: 'review.txt',
            side: 'new',
            start: '2',
            end: '2',
            source: { commit: { sha: head, contentHash: hash('one\nnew\nthree\n') } },
            evidence: { selectedText: 'new', contextBefore: 'one', contextAfter: 'three' },
          }
          const bad = structuredClone(anchor)
          bad.source.commit.contentHash = `sha256:${'0'.repeat(64)}`

          // Act ---------------------------------------------------------------
          const rejected = await r.response('AddComment', {
            scope,
            anchor: bad,
            body: 'Wrong source',
            expectedVersion: review.version,
            requestKey: `bad-${n}`,
          })

          // Assert ------------------------------------------------------------
          expect(rejected.status).toBeGreaterThanOrEqual(400)
          expect(await conversation(r, review.id)).toMatchObject({ version: review.version })
          expect((await conversation(r, review.id)).comments ?? []).toHaveLength(0)

          // Arrange -----------------------------------------------------------
          const request = {
            scope,
            anchor,
            body: 'Exact selected source',
            expectedVersion: review.version,
            requestKey: `exact-${n}`,
          }

          // Act ---------------------------------------------------------------
          const saved = await r.rpc('AddComment', request)

          // Assert ------------------------------------------------------------
          expect(saved.syncError).toBeFalsy()
          expect(saved.comment.postingStatus).toBe('confirmed')
          expect(gh.contentReads - reads).toBe(local ? 0 : 2)
          if (local) await rm(join(r.workspace, '.git'), { recursive: true })

          // Act ---------------------------------------------------------------
          gh.control({ mode: 'denied' })

          // Arrange -----------------------------------------------------------
          const writes = gh.writes,
            before = gh.contentReads

          // Act ---------------------------------------------------------------
          const replayedSave = await r.rpc('AddComment', request)

          // Assert ------------------------------------------------------------
          expect(replayedSave.comment.id).toBe(saved.comment.id)
          expect(gh.writes).toBe(writes)
          expect(gh.contentReads).toBe(before)

          // Act ---------------------------------------------------------------
          gh.control({ mode: 'normal' })
        }
      },
    )

    await step(
      'When an old composer selection meets inserted, renamed, missing or ambiguous text, posting relocates safely or floats to file feedback',
      async () => {
        // Arrange -------------------------------------------------------------
        for (const [text, path, expectedLine] of [
          ['inserted\none\nnew\nthree\n', 'review.txt', 3],
          ['inserted\none\nnew\nthree\n', 'renamed.txt', 3],
          ['one\nreplacement\nthree\n', 'review.txt', null],
          ['new\nnew\n', 'review.txt', null],
        ] as const) {
          // Arrange -----------------------------------------------------------
          gh.control({ base: sha('a'), head: sha('b'), files: null })
          delete gh.contents[sha('b')]

          // Act ---------------------------------------------------------------
          const n = number++,
            review = (await r.rpc('StartReview', { target: target(n), requestKey: `start-${n}` }))
              .review
          const diff = (
            await r.rpc('GetDiff', { target: target(n), comparison: review.comparison })
          ).diff
          const selected = await r.rpc('GetFile', { diff, path: 'review.txt' })

          // Arrange -----------------------------------------------------------
          const anchor = {
            kind: 'line',
            path: 'review.txt',
            side: 'new',
            start: '2',
            end: '2',
            source: selected.newSide.source,
            evidence: { selectedText: 'new', contextBefore: 'one', contextAfter: 'three' },
          }
          gh.control({
            head: sha('c'),
            contents: { [sha('c')]: { [path]: text } },
            files: [
              {
                filename: path,
                previous_filename: 'review.txt',
                status: path === 'review.txt' ? 'modified' : 'renamed',
              },
            ],
          })
          if (path === 'renamed.txt') await refresh(r, review.id, `rename-${n}`)

          // Act ---------------------------------------------------------------
          const saved = await add(
            r,
            review.id,
            'Feedback from the open composer',
            anchor,
            `old-selection-${n}`,
          )

          // Assert ------------------------------------------------------------
          expect(saved.syncError).toBeFalsy()
          expect(saved.comment.postingStatus).toBe('confirmed')
          expect((await current(r, review.id)).comparison.pullRequest.headCommit).toBe(sha('c'))

          const thread = gh.threads[n][0]

          expect(thread.path).toBe(path)
          if (expectedLine)
            expect(thread).toMatchObject({ line: expectedLine, subjectType: 'LINE' })
          else {
            // Assert ----------------------------------------------------------
            expect(thread.subjectType).toBe('FILE')
            expect(thread.comments.nodes[0].body).toContain('Original inline location')
            expect(thread.comments.nodes[0].body).toContain('\nnew\n')
          }
          const original = (await conversation(r, review.id)).threads.find(
            (t: Json) => t.id === saved.comment.threadId,
          )
          expect(original.anchor).toMatchObject({
            path: 'review.txt',
            start: '2',
            source: { commit: { sha: sha('b') } },
          })
        }
      },
    )

    await step(
      'Then promoting agent notes preserves authorship, assigns the promoter ownership and saves one attributed pending draft',
      async () => {
        // Act -----------------------------------------------------------------
        const author = await r.identify({
          name: 'Review agent',
          model: 'test-model',
          provider: 'test-provider',
        })
        const file = await r.write('agent-notes.md', 'Saved notes\n'),
          source = { notes: { path: file } }
        const note = (
          await r.rpc('AddComment', {
            scope: source,
            body: 'Agent feedback to review',
            expectedVersion: '0',
            requestKey: 'agent-note',
          })
        ).comment
        const promoter = await r.identify({ name: 'Promoter', email: 'promoter@example.test' })
        gh.control({ base: sha('a'), head: sha('b'), files: null })
        const n = number++,
          review = (await r.rpc('StartReview', { target: target(n), requestKey: 'agent-review' }))
            .review
        const before = await r.rpc('GetConversation', { scope: source })

        // Arrange -------------------------------------------------------------
        const request = {
          source,
          targetReviewId: review.id,
          commentIds: [note.id],
          anchors: [{ kind: 'file', path: 'review.txt' }],
          expectedSourceVersion: before.version,
          expectedReviewVersion: review.version,
          requestKey: 'promote-agent',
        }

        // Act -----------------------------------------------------------------
        const result = await r.rpc('PromoteNotes', request)

        // Assert --------------------------------------------------------------
        expect(result.syncErrors ?? []).toEqual([])
        expect(result.comments[0]).toMatchObject({
          author: { id: author },
          ownerId: promoter,
          postingStatus: 'confirmed',
        })
        expect(gh.threads[n][0].comments.nodes[0].body).toContain('Review agent')

        // Arrange -------------------------------------------------------------
        const writes = gh.writes

        // Act -----------------------------------------------------------------
        await r.rpc('PromoteNotes', request)

        // Assert --------------------------------------------------------------
        expect(gh.writes).toBe(writes)
        expect((await r.rpc('GetConversation', { scope: source })).comments[0].body).toBe(
          'Agent feedback to review',
        )
      },
    )
  })
})
