import { describe, expect } from 'vitest'
import { mkdir, readFile, rename, rm, access } from 'node:fs/promises'
import { join } from 'node:path'
import { suite } from '../common/fixtures.js'
import { binary, command, isolatedEnv, step, type Json } from '../common/runtime.js'

// fixtures --------------------------------------------------------------------

const it = suite('cli')

// tests -----------------------------------------------------------------------

describe('CLI · a reader can keep feedback through the entire local review lifecycle', () => {
  it('takes guarded notes, reviews ignored plans, promotes feedback, moves sources and resumes after restart', async ({
    runtime: r,
  }) => {
    // Arrange -----------------------------------------------------------------
    await step(
      'Given no running backend, JSON commands report a useful error without starting one',
      async () => {
        // Arrange -------------------------------------------------------------
        const home = join(r.directory, 'offline')

        // Act -----------------------------------------------------------------
        await mkdir(home)

        // Arrange -------------------------------------------------------------
        const result = await command(binary('lgtm'), ['sessions', 'list', '--json'], {
          env: isolatedEnv(home),
        })

        // Assert --------------------------------------------------------------
        expect(result.code).not.toBe(0)
        expect(JSON.parse(result.stdout).error.message).toContain('backend')
        await expect(access(join(home, 'server.port'))).rejects.toThrow()
      },
    )

    await r.git('init', '-q', '-b', 'main')
    const file = await r.write('notes ü.md', 'before\r\nselected\r\nafter')
    await step(
      'When a reader adds a file note twice with one request key, it saves one exact source selection and creates no review',
      async () => {
        // Arrange -------------------------------------------------------------
        const args = [
          'comments',
          'add',
          '--notes',
          file,
          '--file',
          'notes ü.md',
          '--line',
          '2',
          '--body',
          'Explain the fallback',
          '--expected-version',
          '0',
          '--request-key',
          'note-once',
        ]

        // Act -----------------------------------------------------------------
        const saved = await r.json(args)
        const replayedComment = await r.json(args)

        // Assert --------------------------------------------------------------
        expect(replayedComment.comment.id).toBe(saved.comment.id)
        const notes = await r.json(['comments', 'list', '--notes', file])
        expect(notes.comments).toHaveLength(1)
        expect(notes.threads[0].anchor.evidence.selectedText).toBe('selected')
        expect(notes.threads[0].anchor.source.localFile.contentHash).toMatch(/^sha256:/)
        expect((await r.json(['sessions', 'list', '--all'])).reviews).toEqual([])
        expect((await r.json(['session', 'export', '--notes', file])).markdown).toContain(
          '1. **[NOTE]**',
        )

        // Act -----------------------------------------------------------------
        await rm(file)

        // Arrange -------------------------------------------------------------
        const deletion = [
          'comments',
          'delete',
          saved.comment.id,
          '--notes',
          file,
          '--expected-version',
          notes.comments[0].version,
          '--request-key',
          'delete-once',
        ]

        // Act -----------------------------------------------------------------
        const acceptedDeletion = await r.json(deletion)
        const replayedDeletion = await r.json(deletion)

        // Assert --------------------------------------------------------------
        expect(acceptedDeletion).toEqual(replayedDeletion)
        expect((await r.json(['comments', 'list', '--notes', file])).comments).toEqual([])
      },
    )

    const plans = join(r.workspace, '.difflab/plans/private-plan')
    await r.write('.gitignore', '.difflab/\n')
    await r.write('.difflab/plans/private-plan/PLAN.md', '# Plan\nFirst task\n')
    const review = (await r.json(['session', 'start', '--notes', plans])).review
    let note: Json
    await step(
      'When ignored plan feedback is edited and promoted, exports are readable and the source feedback remains intact',
      async () => {
        // Act -----------------------------------------------------------------
        note = (
          await r.json([
            'comments',
            'add',
            '--notes',
            plans,
            '--file',
            'PLAN.md',
            '--line',
            '2',
            '--body',
            'Clarify the task',
          ])
        ).comment
        await r.json([
          'comments',
          'edit',
          note.id,
          '--notes',
          plans,
          '--body',
          'Clarify the acceptance test',
        ])

        // Assert --------------------------------------------------------------
        expect(JSON.stringify(await r.json(['comments', 'list', '--notes', plans]))).toContain(
          'Clarify the acceptance test',
        )
        expect((await r.json(['session', 'export', '--notes', plans])).markdown).toContain(
          'Clarify the acceptance test',
        )

        // Arrange -------------------------------------------------------------
        const target = join(r.workspace, 'review-target')

        // Act -----------------------------------------------------------------
        await r.write('review-target/PLAN.md', '# Target review\n')
        const targetReview = (await r.json(['session', 'start', '--notes', target])).review
        const promotion = await r.json([
          'comments',
          'promote',
          '--notes',
          plans,
          '--target-session',
          targetReview.id,
          '--comment',
          note.id,
          '--file',
          'PLAN.md',
        ])

        // Assert --------------------------------------------------------------
        expect(promotion.comments[0].body).toBe('Clarify the acceptance test')

        const markdown = (await r.json(['session', 'export', '--notes', target])).markdown

        for (const text of ['1. **[COMMENT]**', '`PLAN.md`', 'Clarify the acceptance test'])
          expect(markdown).toContain(text)
        expect(markdown).not.toContain(note.id)
        expect((await r.json(['comments', 'list', '--notes', plans])).comments).toEqual(
          expect.arrayContaining([
            expect.objectContaining({ id: note.id, body: 'Clarify the acceptance test' }),
          ]),
        )
      },
    )

    await step(
      'Then restart resumes the same attempt, completion creates a successor only when requested, and conversation feedback survives',
      async () => {
        // Act -----------------------------------------------------------------
        await r.restart()
        const resumedReview = await r.json(['session', 'start', '--notes', plans])

        // Assert --------------------------------------------------------------
        expect(resumedReview.review.id).toBe(review.id)

        // Act -----------------------------------------------------------------
        const completedReview = await r.json(['session', 'complete', '--notes', plans])

        // Assert --------------------------------------------------------------
        expect(completedReview.review.state).toBe('completed')

        // Act -----------------------------------------------------------------
        const successor = await r.json(['session', 'start', '--notes', plans])

        // Assert --------------------------------------------------------------
        expect(successor.review.id).not.toBe(review.id)
        expect(JSON.stringify(await r.json(['comments', 'list', '--notes', plans]))).toContain(
          'Clarify the acceptance test',
        )
      },
    )

    await step(
      'When a saved note moves, the guarded move can be retried and preserves its discussion identity',
      async () => {
        // Act -----------------------------------------------------------------
        const original = await r.write('before.md', 'before\nselected\nafter\n'),
          destination = join(r.workspace, 'after.md')
        const saved = (
          await r.json([
            'comments',
            'add',
            '--notes',
            original,
            '--file',
            'before.md',
            '--line',
            '2',
            '--body',
            'Move this note',
          ])
        ).comment
        const before = await r.json(['comments', 'list', '--notes', original])
        await rename(original, destination)

        // Arrange -------------------------------------------------------------
        const move = [
          'session',
          'refresh',
          '--notes',
          destination,
          '--moved-from',
          original,
          '--expected-version',
          before.version,
          '--request-key',
          'file-move',
        ]

        // Act -----------------------------------------------------------------
        const accepted = await r.json(move)
        const replayedMove = await r.json(move)

        // Assert --------------------------------------------------------------
        expect(replayedMove).toEqual(accepted)
        expect((await r.json(['comments', 'list', '--notes', destination])).comments[0].id).toBe(
          saved.id,
        )
      },
    )

    await step(
      'And moving the ignored review root preserves its review ID, feedback and read-only export',
      async () => {
        // Arrange -------------------------------------------------------------
        const before = await r.json(['comments', 'list', '--notes', plans]),
          active = (await r.json(['session', 'show', '--notes', plans])).review
        const destination = join(r.workspace, '.difflab/plans/renamed')

        // Act -----------------------------------------------------------------
        await rename(plans, destination)
        await r.json([
          'session',
          'refresh',
          '--notes',
          destination,
          '--moved-from',
          plans,
          '--expected-version',
          before.version,
          '--request-key',
          'directory-move',
        ])
        const resumedMovedReview = await r.json(['session', 'start', '--notes', destination])

        // Assert --------------------------------------------------------------
        expect(resumedMovedReview.review.id).toBe(active.id)
        const moved = await r.json(['comments', 'list', '--notes', destination])
        expect(moved.comments[0].id).toBe(note.id)
        expect((await r.json(['session', 'export', '--notes', destination])).markdown).toContain(
          'Clarify the acceptance test',
        )
        expect(await r.json(['comments', 'list', '--notes', destination])).toEqual(moved)
      },
    )
  })

  it('reviews old-side working changes, exports final feedback, and carries exact staged sources into a branch review', async ({
    runtime: r,
  }) => {
    // Arrange -----------------------------------------------------------------
    await r.git('init', '-q', '-b', 'main')

    await r.write('review.md', 'before\nold selection\nafter\n')
    await r.commit('base')
    await r.write('review.md', 'before\nnew selection\nafter\n')
    await step(
      'Given working changes, old-side comments, Markdown and replies survive completion and export without mutation',
      async () => {
        // Act -----------------------------------------------------------------
        await r.json(['session', 'start', '--mode', 'working'])
        const root = (
          await r.json([
            'comments',
            'add',
            '--mode',
            'working',
            '--file',
            'review.md',
            '--line',
            '2',
            '--side',
            'old',
            '--body',
            'Explain the fallback.\n\n- Keep **Markdown** and `code`.',
          ])
        ).comment
        await r.json([
          'comments',
          'reply',
          root.threadId,
          '--mode',
          'working',
          '--body',
          'Use the documented alternative.',
        ])
        await r.json([
          'session',
          'complete',
          '--mode',
          'working',
          '--body',
          'Please address these cases before merging.',
        ])
        const before = await r.json(['comments', 'list', '--mode', 'working']),
          output = join(r.workspace, 'feedback.md')

        // Assert --------------------------------------------------------------
        const markdown = (
          await r.json(['session', 'export', '--mode', 'working', '--output', output])
        ).markdown

        expect(await readFile(output, 'utf8')).toBe(markdown)
        for (const text of [
          '1. **[COMMENT]**',
          '`review.md:~2`',
          'old (left) side',
          'Explain the fallback.',
          '- Keep **Markdown** and `code`.',
          '**Reply',
          'Use the documented alternative.',
          '## Overall feedback',
          'Please address these cases before merging.',
        ])
          expect(markdown).toContain(text)
        for (const comment of before.comments) expect(markdown).not.toContain(comment.id)
        for (const field of ['sha256:', 'originSessionId', 'postingStatus', 'contextId'])
          expect(markdown).not.toContain(field)
        expect(await r.json(['comments', 'list', '--mode', 'working'])).toEqual(before)
      },
    )

    await r.git('reset', '--hard', 'HEAD')
    await rm(join(r.workspace, 'feedback.md'))
    await r.write('source [literal] ü.txt', 'before\noriginal\nafter\n')
    const base = await r.commit('source base')
    await r.git('checkout', '-qb', 'feature')
    const path = 'source [literal] ü.txt'
    await r.write(path, 'before\r\nselected\r\nafter')
    await r.git('add', '--', path)
    const blob = await r.git('rev-parse', `:${path}`)
    await r.write(path, 'before\nunsaved to index\nafter\n')
    await step(
      'When the index differs from the working tree, comments capture stage-zero bytes rather than HEAD or working content',
      async () => {
        // Act -----------------------------------------------------------------
        await r.json(['session', 'start', '--mode', 'staged'])
        const saved = (
          await r.json([
            'comments',
            'add',
            '--file',
            path,
            '--line',
            '2',
            '--body',
            'Keep this staged behavior',
          ])
        ).comment

        // Assert --------------------------------------------------------------
        const original = await r.json(['comments', 'list']),
          anchor = original.threads.find((t: Json) => t.id === saved.threadId).anchor

        expect(anchor.source.indexFile.blobOid).toBe(blob)
        expect(anchor.evidence.selectedText).toBe('selected')
        expect(anchor.source.commit).toBeUndefined()

        // Act -----------------------------------------------------------------
        await r.git(
          '-c',
          'user.name=E2E',
          '-c',
          'user.email=e2e@example.test',
          'commit',
          '-qm',
          'reviewed staged change',
        )
        const head = await r.git('rev-parse', 'HEAD')
        await r.json(['session', 'complete'])
        const branch = (await r.json(['session', 'start', '--mode', 'branch', '--base', base]))
          .review

        // Assert --------------------------------------------------------------
        expect(branch.comparison.branch).toEqual({ baseCommit: base, headCommit: head })
        const carried = await r.json(['comments', 'list'])
        expect(carried.comments.map((c: Json) => c.id)).toContain(saved.id)
        expect(carried.threads.find((t: Json) => t.id === saved.threadId).anchor).toEqual(anchor)
        const markdown = (await r.json(['session', 'export'])).markdown
        expect(markdown).toContain('Keep this staged behavior')
        expect(markdown).not.toContain(blob)
      },
    )
  })
})

describe('CLI · remote review recovery is explicit and never duplicates publication', () => {
  it('selects and resumes PRs, saves pending feedback, reports provider errors and recovers accepted writes', async ({
    runtime: r,
    github: gh,
  }) => {
    // Arrange -----------------------------------------------------------------
    gh.control({ base: 'a'.repeat(40), head: 'b'.repeat(40) })
    let number = 1
    for (const mode of ['normal', 'lost', 'stale']) {
      await step(
        `Given ${mode} final-response behavior, start, refresh and reopen select the same PR review`,
        async () => {
          // Arrange -----------------------------------------------------------
          gh.control({ mode: 'normal', head: 'b'.repeat(40) })
          const pr = `https://github.com/acme/repo/pull/${number++}`

          // Act ---------------------------------------------------------------
          const started = (await r.json(['session', 'start', '--pr', pr])).review

          // Assert ------------------------------------------------------------
          expect(started.state).toBe('active')

          // Act ---------------------------------------------------------------
          const refreshed = await r.json(['session', 'refresh', '--pr', pr])

          // Assert ------------------------------------------------------------
          expect(refreshed.review.observedComparison.pullRequest.headCommit).toBe('b'.repeat(40))

          // Act ---------------------------------------------------------------
          const reopenedPrReview = await r.json(['session', 'start', '--pr', pr])

          // Assert ------------------------------------------------------------
          expect(reopenedPrReview.review.id).toBe(started.id)

          // Act ---------------------------------------------------------------
          await r.json(['comments', 'add', '--pr', pr, '--body', 'General CLI comment'])
          await r.json([
            'comments',
            'add',
            '--pr',
            pr,
            '--file',
            'review.txt',
            '--line',
            '2',
            '--body',
            'Inline CLI comment',
          ])
          await r.json([
            'comments',
            'add',
            '--pr',
            pr,
            '--file',
            'review.txt',
            '--body',
            'File CLI comment',
          ])

          // Assert ------------------------------------------------------------
          expect(gh.reviews[number - 1].state).toBe('PENDING')

          // Arrange -----------------------------------------------------------
          const before = gh.writes

          gh.control({ mode })
          const submit = [
            'session',
            'submit',
            '--pr',
            pr,
            '--outcome',
            'approve',
            '--body',
            'Final CLI body',
            '--request-key',
            `cli-once-${number}`,
          ]
          if (mode === 'stale') {
            // Act -------------------------------------------------------------
            const output = await r.cli([...submit, '--json'])

            // Assert ----------------------------------------------------------
            expect(output.code).not.toBe(0)
            expect(JSON.parse(output.stdout).error.message).toContain('head')
            expect(gh.writes).toBe(before)
            expect((await r.json(['comments', 'list', '--pr', pr])).comments).toHaveLength(3)
            expect((await r.json(['session', 'show', '--pr', pr])).review.state).toBe('active')
            return
          }

          // Act ---------------------------------------------------------------
          const submission = await r.json(submit)

          // Assert ------------------------------------------------------------
          expect(submission.review.submissionStatus).toBe(
            mode === 'lost' ? 'uncertain' : 'confirmed',
          )
          expect(gh.reviews[number - 1].state).toBe('APPROVED')
          expect(gh.reviews[number - 1].body).toContain('Final CLI body')

          // Arrange -----------------------------------------------------------
          const writes = gh.writes

          gh.control({ mode: 'normal' })
          if (mode === 'lost') await r.restart()

          // Act ---------------------------------------------------------------
          const confirmedSubmission = await r.json(
            mode === 'lost' ? ['session', 'reconcile', '--pr', pr] : submit,
          )

          // Assert ------------------------------------------------------------
          expect(confirmedSubmission.review.submissionStatus).toBe('confirmed')
          expect(gh.writes).toBe(writes)
          const markdown = (await r.json(['session', 'export', '--pr', pr])).markdown
          expect(markdown).toContain('Inline CLI comment')
          expect(markdown).toContain('Final CLI body')
        },
      )
    }

    await step(
      'When refresh is denied, JSON reports the error and preserves unsent general feedback for recovery',
      async () => {
        // Arrange -------------------------------------------------------------
        gh.control({ mode: 'normal', head: 'b'.repeat(40) })
        const pr = `https://github.com/acme/repo/pull/${number++}`

        // Act -----------------------------------------------------------------
        await r.json(['session', 'start', '--pr', pr])
        await r.json(['session', 'refresh', '--pr', pr])
        await r.json(['comments', 'add', '--pr', pr, '--body', 'Retain denied CLI draft'])

        // Arrange -------------------------------------------------------------
        const writes = gh.writes
        gh.control({ mode: 'denied' })

        // Act -----------------------------------------------------------------
        const output = await r.cli(['session', 'refresh', '--pr', pr, '--json'])

        // Assert --------------------------------------------------------------
        expect(output.code).not.toBe(0)
        expect(JSON.parse(output.stdout).error.message).toEqual(expect.any(String))
        expect((await r.json(['comments', 'list', '--pr', pr])).comments[0].body).toBe(
          'Retain denied CLI draft',
        )
        expect(gh.writes).toBe(writes)

        // Act -----------------------------------------------------------------
        gh.control({ mode: 'normal' })
        await r.json(['session', 'refresh', '--pr', pr])
      },
    )

    await step(
      'Then an accepted draft with a lost response remains the same pending comment after a push and sync',
      async () => {
        // Arrange -------------------------------------------------------------
        const pr = `https://github.com/acme/repo/pull/${number++}`

        // Act -----------------------------------------------------------------
        await r.json(['session', 'start', '--pr', pr])
        await r.json(['session', 'refresh', '--pr', pr])
        gh.control({ mode: 'draft_lost' })
        const saved = await r.json([
          'comments',
          'add',
          '--pr',
          pr,
          '--file',
          'review.txt',
          '--line',
          '2',
          '--body',
          'Recover this private draft',
        ])

        // Assert --------------------------------------------------------------
        expect(saved.syncError).toBeTruthy()
        expect(saved.comment.postingStatus).toBe('uncertain')

        // Arrange -------------------------------------------------------------
        const writes = gh.writes

        gh.control({ head: 'c'.repeat(40) })

        // Act -----------------------------------------------------------------
        const recovered = await r.json(['comments', 'sync', saved.comment.id, '--pr', pr])

        // Assert --------------------------------------------------------------
        expect(recovered.comment).toMatchObject({ postingStatus: 'confirmed', visibility: 'draft' })
        expect(gh.writes).toBe(writes)
        expect(gh.reviews[number - 1].state).toBe('PENDING')
      },
    )
  })
})
