import { describe, it, expect } from 'vitest'
import { LiveGithub } from '../common/live-github.js'
import { Runtime, command, step, waitFor, type Json } from '../common/runtime.js'

// tests -----------------------------------------------------------------------

describe('GitHub · one permanent private repository', () => {
  it.runIf(process.env.LGTM_LIVE_GITHUB_E2E === '1')(
    'keeps one pending review through edits and pushes, submits a Comment and cleans only this run’s PR and refs',
    async () => {
      // Arrange ---------------------------------------------------------------
      const remote = await LiveGithub.start()
      let r: Runtime | undefined,
        failed = true
      try {
        const credentials = await command('gh', ['auth', 'token', '--hostname', 'github.com'])
        if (credentials.code) throw new Error('GitHub credentials unavailable')
        r = await Runtime.create('backend', undefined, undefined, {
          GH_TOKEN: credentials.stdout.trim(),
        })
        r.rpcTimeoutMs = 40_000
        await r.identify({ name: 'LGTM smoke agent', model: 'smoke-test', provider: 'lgtm' })
        const target = {
          pullRequest: { repositoryUrl: remote.repositoryUrl, number: String(remote.number) },
        }
        // Act -----------------------------------------------------------------
        const review = (await r.rpc('StartReview', { target, requestKey: 'start' })).review,
          scope = { reviewId: review.id }
        const conversation = () => r!.rpc('GetConversation', { scope })
        const current = async () => (await r!.rpc('GetReview', { reviewId: review.id })).review
        const add = async (body: string, anchor: Json, key: string) =>
          r!.rpc('AddComment', {
            scope,
            body,
            anchor,
            expectedVersion: (await conversation()).version,
            requestKey: key,
          })
        const selection = async () => {
          // Act ---------------------------------------------------------------
          const descriptor = (
            await r!.rpc('GetDiff', { target, comparison: (await current()).comparison })
          ).diff
          // Assert ------------------------------------------------------------
          expect(
            (await r!.rpc('GetFiles', { diff: descriptor })).files.some(
              (f: Json) => f.path === remote.path,
            ),
          ).toBe(true)
          // Act ---------------------------------------------------------------
          return r!.rpc('GetFile', { diff: descriptor, path: remote.path })
        }
        const selected = await selection()
        // Assert --------------------------------------------------------------
        expect(selected.newSide.content).toBe('first\nchanged\n')
        // Arrange -------------------------------------------------------------
        const anchor = {
          kind: 'line',
          path: remote.path,
          start: '2',
          end: '2',
          side: 'new',
          source: selected.newSide.source,
          evidence: { selectedText: 'changed', contextBefore: 'first' },
        }
        let line: Json, pending: string, original: Json
        await step(
          'Given a remote-only PR, save line, file, general and reply feedback privately before submission',
          async () => {
            // Act -------------------------------------------------------------
            line = (await add('Check the changed line', anchor, 'line')).comment

            // Assert ----------------------------------------------------------
            expect(line.forgeCommentId).toBeTruthy()
            expect(line.visibility).toBe('draft')
            pending = (await current()).forgeReviewId
            expect(
              (
                await remote.api(
                  `repos/${remote.repository}/pulls/${remote.number}/reviews/${pending}`,
                )
              ).state,
            ).toBe('PENDING')

            let snapshot = await conversation()

            // Act -------------------------------------------------------------
            await r!.rpc('RefreshReview', {
              scope,
              expectedVersion: snapshot.version,
              requestKey: 'refresh',
            })

            // Assert ----------------------------------------------------------
            snapshot = await conversation()

            expect(snapshot.comments).toHaveLength(1)
            expect(snapshot.comments[0].visibility).toBe('draft')

            // Act -------------------------------------------------------------
            await r!.rpc('EditComment', {
              scope,
              commentId: line.id,
              body: 'Edited private draft before submission',
              expectedVersion: snapshot.comments[0].version,
              requestKey: 'edit-pending',
            })
            snapshot = await conversation()

            // Arrange ---------------------------------------------------------
            const thread = snapshot.threads.find((t: Json) => t.id === line.threadId)
            original = thread.anchor

            // Act -------------------------------------------------------------
            await r!.rpc('Reply', {
              scope,
              threadId: thread.id,
              body: 'Reply smoke comment',
              expectedVersion: thread.version,
              requestKey: 'reply',
            })
            await add(
              'File-level smoke comment',
              { kind: 'file', path: remote.path, source: selected.newSide.source, evidence: {} },
              'file',
            )
            await add('General smoke comment', { kind: 'general' }, 'general')
          },
        )

        await step(
          'When a push moves an open composer selection, the same pending review posts it at the current line',
          async () => {
            // Act -------------------------------------------------------------
            await remote.push('inserted\nfirst\nchanged\n')

            // Assert ----------------------------------------------------------
            await waitFor(
              'GitHub exposes pushed head',
              () => remote.api(`repos/${remote.repository}/pulls/${remote.number}`),
              (pr) => pr.head.sha === remote.head,
              15_000,
            )

            // Act -------------------------------------------------------------
            const moved = await add('Selected before a push; follows its line', anchor, 'push-line')

            // Assert ----------------------------------------------------------
            expect(moved.syncError).toBeFalsy()
            expect(moved.comment.forgeCommentId).toBeTruthy()
            expect(await current()).toMatchObject({
              id: review.id,
              forgeReviewId: pending,
              comparison: { pullRequest: { headCommit: remote.head } },
            })
            const thread = (await remote.threads()).find((t) =>
              t.comments.nodes.some(
                (c: Json) => String(c.databaseId) === moved.comment.forgeCommentId,
              ),
            )!
            expect(thread).toMatchObject({ line: 3, subjectType: 'LINE' })
            expect(
              (await conversation()).threads.find((t: Json) => t.id === line.threadId).anchor,
            ).toEqual(original)

            // Arrange ---------------------------------------------------------
            const source = (await selection()).newSide.source

            // Act -------------------------------------------------------------
            await remote.push('inserted\nfirst\nreplacement\n')

            // Assert ----------------------------------------------------------
            await waitFor(
              'GitHub exposes replacement',
              () => remote.api(`repos/${remote.repository}/pulls/${remote.number}`),
              (pr) => pr.head.sha === remote.head,
              15_000,
            )

            // Act -------------------------------------------------------------
            const lost = await add(
              'Retain feedback when selection is gone',
              {
                ...anchor,
                start: '3',
                end: '3',
                source,
                evidence: { selectedText: 'changed', contextBefore: 'inserted\nfirst' },
              },
              'lost-line',
            )

            // Assert ----------------------------------------------------------
            expect(lost.syncError).toBeFalsy()
            const fallback = (await remote.threads()).find((t) =>
              t.comments.nodes.some(
                (c: Json) => String(c.databaseId) === lost.comment.forgeCommentId,
              ),
            )!
            expect(fallback.subjectType).toBe('FILE')
            expect(fallback.comments.nodes[0].body).toContain('Original inline location')
          },
        )

        await step(
          'Then invalid self-review outcomes leave drafts pending, and Comment submits the existing review with a final body',
          async () => {
            // Assert ----------------------------------------------------------
            for (const outcome of ['approve', 'request_changes']) {
              // Arrange -------------------------------------------------------
              const request = {
                reviewId: review.id,
                outcome,
                body: 'Self-review final body',
                requestKey: `self-${outcome}`,
                expectedHead: remote.head,
                expectedBase: remote.base,
                expectedVersion: (await current()).version,
              }
              const response = await r!.response('SubmitReview', request)

              // Assert --------------------------------------------------------
              expect(response.status).toBeGreaterThanOrEqual(400)
              expect(response.body.message).toContain('own PR')
              expect(await current()).toMatchObject({
                state: 'active',
                submissionStatus: 'unsubmitted',
                forgeReviewId: pending,
              })
            }
            for (const event of ['APPROVE', 'REQUEST_CHANGES']) {
              // Arrange -------------------------------------------------------
              const result = await command(
                'gh',
                [
                  'api',
                  '--method',
                  'POST',
                  `repos/${remote.repository}/pulls/${remote.number}/reviews/${pending}/events`,
                  '--input',
                  '-',
                ],
                { input: JSON.stringify({ event, body: 'Invalid self-review fixture' }) },
              )

              // Assert --------------------------------------------------------
              expect(result.code).not.toBe(0)
              expect(result.stderr).toContain('HTTP 422')
              expect(
                (
                  await remote.api(
                    `repos/${remote.repository}/pulls/${remote.number}/reviews/${pending}`,
                  )
                ).state,
              ).toBe('PENDING')
            }

            const result = await r!.rpc('SubmitReview', {
              reviewId: review.id,
              outcome: 'comment',
              body: 'Final smoke review body',
              requestKey: 'submit',
              expectedHead: remote.head,
              expectedBase: remote.base,
              expectedVersion: (await current()).version,
            })

            expect(result.review).toMatchObject({
              forgeReviewId: pending,
              submissionStatus: 'confirmed',
            })
          },
        )

        await step(
          'And posted feedback supports resolve, reopen, edit and deletion without placeholders; export remains read-only',
          async () => {
            // Arrange ---------------------------------------------------------
            for (const resolved of [true, false]) {
              // Arrange -------------------------------------------------------
              const thread = (await conversation()).threads.find(
                (t: Json) => t.id === line.threadId,
              )

              // Act -----------------------------------------------------------
              await r!.rpc('ResolveThread', {
                scope,
                threadId: thread.id,
                resolved,
                expectedVersion: thread.version,
                requestKey: `resolve-${resolved}`,
              })
            }
            let root = (await conversation()).comments.find((c: Json) => c.id === line.id)

            // Act -------------------------------------------------------------
            await r!.rpc('EditComment', {
              scope,
              commentId: line.id,
              body: 'Edited smoke comment',
              expectedVersion: root.version,
              requestKey: 'edit-posted',
            })
            root = (await conversation()).comments.find((c: Json) => c.id === line.id)
            await r!.rpc('DeleteComment', {
              scope,
              commentId: line.id,
              expectedVersion: root.version,
              requestKey: 'delete',
            })
            await r!.rpc('RefreshReview', {
              scope,
              expectedVersion: (await conversation()).version,
              requestKey: 'refresh-after-delete',
            })

            // Assert ----------------------------------------------------------
            const before = await conversation()

            expect(before.comments.some((c: Json) => c.id === line.id)).toBe(false)
            expect(before.comments.some((c: Json) => c.body === 'Reply smoke comment')).toBe(true)
            const markdown = (await r!.rpc('Export', { scope })).markdown
            expect(markdown).toContain('Final smoke review body')
            expect(markdown).toContain('Reply smoke comment')
            expect(markdown).not.toContain('Edited smoke comment')
            expect(markdown).not.toContain('comment deleted')
            expect(await conversation()).toEqual(before)
          },
        )
        failed = false
      } finally {
        await r?.dispose(failed)
        await remote.dispose()
      }
    },
    180_000,
  )
})
