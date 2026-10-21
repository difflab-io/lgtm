import { describe, expect } from 'vitest'
import { copyFile, mkdir, readFile, rename, rm } from 'node:fs/promises'
import { join } from 'node:path'
import { suite } from '../common/fixtures.js'
import { hash, step, type Json } from '../common/runtime.js'

// fixtures --------------------------------------------------------------------

const it = suite('backend')
const line = (bytes: string, path = '') => ({
  kind: 'line',
  path,
  side: 'new',
  start: '2',
  end: '2',
  source: { localFile: { contentHash: hash(bytes) } },
  evidence: { selectedText: 'selected', contextBefore: 'before', contextAfter: 'after' },
})

// tests -----------------------------------------------------------------------

describe('Review API · local feedback belongs to its context throughout its lifecycle', () => {
  it('lets a reader take notes without setup, reject stale edits, recover retries, move files, and export discussions', async ({
    runtime: r,
  }) => {
    // Arrange -----------------------------------------------------------------
    await step(
      'Given the target-based API, consumers need no context or session setup',
      async () => {
        // Act -----------------------------------------------------------------
        const capabilities = await r.rpc('GetCapabilities')

        // Assert --------------------------------------------------------------
        expect(capabilities.capabilities).toEqual(
          expect.arrayContaining([
            'review_targets',
            'feedback_scopes',
            'anchor_evidence',
            'diff_service',
            'rejected_submission_recovery',
          ]),
        )
        for (const method of [
          'EnsureContext',
          'StartSession',
          'GetSession',
          'ListSessions',
          'CompleteSession',
          'RefreshContext',
          'GetWorkspace',
        ]) {
          await r.reject(method, {}, 'unimplemented')
        }
        expect((await r.rpc('ListReviews')).reviews ?? []).toEqual([])

        // Act -----------------------------------------------------------------
        await r.identify()
      },
    )

    // Arrange -----------------------------------------------------------------
    const bytes = 'before\r\nselected\r\nafter',
      file = await r.write('notes/one.md', bytes)
    let scope: Json = { notes: { path: file } }
    let root: Json, saved: Json
    await step(
      'When a reader adds a range note, a lost response can be retried even after its file disappears',
      async () => {
        // Assert --------------------------------------------------------------
        expect(await r.rpc('GetConversation', { scope })).toMatchObject({ version: '0' })

        // Arrange -------------------------------------------------------------
        const request = {
          scope,
          anchor: line(bytes),
          body: 'Explain this case',
          expectedVersion: '0',
          requestKey: 'note-create',
        }

        // Act -----------------------------------------------------------------
        root = (await r.rpc('AddComment', request)).comment
        await rm(file)
        const replayedSave = await r.rpc('AddComment', request)

        // Assert --------------------------------------------------------------
        expect(replayedSave.comment.id).toBe(root.id)
        saved = await r.rpc('GetConversation', { scope })
        expect(saved.comments).toHaveLength(1)
        expect(saved.threads[0].anchor).toMatchObject({
          start: '2',
          source: { localFile: { contentHash: hash(bytes) } },
        })
        await r.reject('AddComment', { ...request, body: 'Different request' })
        expect((await r.rpc('ListReviews')).reviews ?? []).toEqual([])

        // Act -----------------------------------------------------------------
        await r.write('notes/one.md', bytes)
      },
    )

    await step(
      'Then guarded edits reject another editor’s stale body, and a retry returns the accepted result',
      async () => {
        // Arrange -------------------------------------------------------------
        const token = root.version
        const edit = {
          scope,
          commentId: root.id,
          body: 'Accepted edit',
          expectedVersion: token,
          requestKey: 'edit',
        }

        // Act -----------------------------------------------------------------
        const acceptedEdit = await r.rpc('EditComment', edit)

        // Assert --------------------------------------------------------------
        expect(acceptedEdit.comment.body).toBe('Accepted edit')
        await r.reject('EditComment', { ...edit, body: 'Stale edit', requestKey: 'other-editor' })

        const current = await r.rpc('GetConversation', { scope })

        // Arrange -------------------------------------------------------------
        const combined = token.split(':')
        combined[1] = current.version

        // Assert --------------------------------------------------------------
        await r.reject('EditComment', {
          ...edit,
          expectedVersion: combined.join(':'),
          requestKey: 'mixed-token',
        })

        // Act -----------------------------------------------------------------
        const replayedEdit = await r.rpc('EditComment', edit)

        // Assert --------------------------------------------------------------
        expect(replayedEdit.comment.body).toBe('Accepted edit')
      },
    )

    await step(
      'When the reader replies and resolves the thread, numbered Markdown retains the discussion without exposing IDs',
      async () => {
        // Arrange -------------------------------------------------------------
        let current = await r.rpc('GetConversation', { scope })

        // Act -----------------------------------------------------------------
        await r.rpc('Reply', {
          scope,
          threadId: root.threadId,
          body: 'Checked',
          expectedVersion: current.threads[0].version,
          requestKey: 'reply',
        })
        current = await r.rpc('GetConversation', { scope })
        await r.rpc('ResolveThread', {
          scope,
          threadId: root.threadId,
          resolved: true,
          expectedVersion: current.threads[0].version,
          requestKey: 'resolve',
        })
        const before = await r.rpc('GetConversation', { scope })
        const markdown = (await r.rpc('Export', { scope })).markdown

        // Assert --------------------------------------------------------------
        for (const text of ['1. **[NOTE]**', 'Accepted edit', 'Checked', 'resolved'])
          expect(markdown).toContain(text)

        expect(markdown).not.toContain(root.threadId)
        expect(await r.rpc('GetConversation', { scope })).toEqual(before)
      },
    )

    const destination = join(r.workspace, 'moved-notes/two.md')
    await step(
      'When a file moves between parents, copies are rejected and a confirmed move preserves IDs and original evidence',
      async () => {
        // Act -----------------------------------------------------------------
        await mkdir(join(r.workspace, 'moved-notes'))
        await copyFile(file, destination)
        const before = await r.rpc('GetConversation', { scope })

        // Arrange -------------------------------------------------------------
        const event = { locationMoved: { previousPath: file, currentPath: destination } }

        // Assert --------------------------------------------------------------
        await r.reject('RefreshReview', {
          scope,
          expectedVersion: before.version,
          sourceChanges: [event],
          requestKey: 'copy-is-not-move',
        })
        expect(await r.rpc('GetConversation', { scope })).toEqual(before)

        // Act -----------------------------------------------------------------
        await rm(destination)
        await rename(file, destination)

        // Arrange -------------------------------------------------------------
        const request = {
          scope,
          expectedVersion: before.version,
          sourceChanges: [event],
          requestKey: 'move',
        }

        // Act -----------------------------------------------------------------
        const accepted = await r.rpc('RefreshReview', request)
        const replayedMove = await r.rpc('RefreshReview', request)

        // Assert --------------------------------------------------------------
        expect(replayedMove).toEqual(accepted)
        scope = { notes: { path: destination } }
        const moved = await r.rpc('GetConversation', { scope })
        expect(moved.comments[0].id).toBe(root.id)
        expect(moved.threads[0]).toMatchObject({
          sourcePath: file,
          currentPath: destination,
          anchor: saved.threads[0].anchor,
        })

        // Act -----------------------------------------------------------------
        await r.rpc('AddComment', {
          scope,
          anchor: line(bytes),
          body: 'New note at destination',
          expectedVersion: moved.version,
          requestKey: 'new-location',
        })
      },
    )

    await step(
      'Then backend restart preserves the moved discussion and repeated deletion cannot recreate feedback',
      async () => {
        // Act -----------------------------------------------------------------
        await r.restart()

        // Assert --------------------------------------------------------------
        const current = await r.rpc('GetConversation', { scope })

        expect(current.comments).toHaveLength(3)
        await r.reject('DeleteComment', {
          scope,
          commentId: root.id,
          expectedVersion: current.comments.find((c: Json) => c.id === root.id).version,
          requestKey: 'root-with-replies',
        })

        // Arrange -------------------------------------------------------------
        const reply = current.comments.find(
          (c: Json) => c.threadId === root.threadId && c.id !== root.id,
        )

        // Act -----------------------------------------------------------------
        await r.rpc('DeleteComment', {
          scope,
          commentId: reply.id,
          expectedVersion: reply.version,
          requestKey: 'delete-reply',
        })
        const afterReply = await r.rpc('GetConversation', { scope })

        // Arrange -------------------------------------------------------------
        const deletion = {
          scope,
          commentId: root.id,
          expectedVersion: afterReply.comments.find((c: Json) => c.id === root.id).version,
          requestKey: 'delete-once',
        }

        // Act -----------------------------------------------------------------
        const accepted = await r.rpc('DeleteComment', deletion)
        const replayedDeletion = await r.rpc('DeleteComment', deletion)

        // Assert --------------------------------------------------------------
        expect(replayedDeletion).toEqual(accepted)
        expect(
          (await r.rpc('GetConversation', { scope })).comments.map((c: Json) => c.id),
        ).not.toContain(root.id)
      },
    )
  })

  it('reviews ignored directories, pages and filters feedback, promotes notes, and preserves moved or deleted sources', async ({
    runtime: r,
  }) => {
    // Arrange -----------------------------------------------------------------
    await r.identify()
    const bytes = 'before\nselected\nafter\n',
      plans = join(r.workspace, '.difflab/plan/example')

    await r.write('.gitignore', '.difflab/\n')
    await r.write('.difflab/plan/example/PLAN.md', bytes)
    let diff: Json
    await step(
      'Given an ignored directory, DiffService reads exact bytes without creating a review',
      async () => {
        // Assert --------------------------------------------------------------
        diff = (await r.rpc('GetDiff', { target: { directory: { path: plans } } })).diff

        expect((await r.rpc('GetFiles', { diff, pageSize: 1 })).files[0].path).toBe('PLAN.md')
        const file = await r.rpc('GetFile', {
          diff,
          path: 'PLAN.md',
          anchors: [{ original: line(bytes, 'PLAN.md') }],
        })
        expect(file.newSide.content).toBe(bytes)
        expect(file.placements[0].exact.location.start).toBe('2')
        expect((await r.rpc('ListReviews')).reviews ?? []).toEqual([])
      },
    )

    await step(
      'When content shifts or vanishes, fresh descriptors relocate uniquely or report an outdated selection',
      async () => {
        // Act -----------------------------------------------------------------
        await copyFile(join(plans, 'PLAN.md'), join(plans, 'MOVED.md'))
        const movedDiff = (await r.rpc('GetDiff', { target: { directory: { path: plans } } })).diff

        // Assert --------------------------------------------------------------
        const moved = await r.rpc('GetFile', {
          diff: movedDiff,
          path: 'MOVED.md',
          anchors: [{ original: line(bytes, 'PLAN.md'), currentPath: join(plans, 'MOVED.md') }],
        })

        expect(moved.placements[0].relocated.location.start).toBe('2')
        const reused = await r.rpc('GetFile', {
          diff: movedDiff,
          path: 'PLAN.md',
          anchors: [{ original: line(bytes, 'PLAN.md'), currentPath: 'MOVED.md' }],
        })
        expect(reused.placements[0].unverified).toBeDefined()

        // Act -----------------------------------------------------------------
        await rm(join(plans, 'MOVED.md'))
        await r.write('.difflab/plan/example/PLAN.md', 'inserted\n' + bytes)

        // Assert --------------------------------------------------------------
        await r.reject('GetFile', { diff, path: 'PLAN.md' })
        const fresh = (await r.rpc('GetDiff', { target: { directory: { path: plans } } })).diff
        expect(
          (
            await r.rpc('GetFile', {
              diff: fresh,
              path: 'PLAN.md',
              anchors: [{ original: line(bytes, 'PLAN.md') }],
            })
          ).placements[0].relocated.location.start,
        ).toBe('3')

        // Act -----------------------------------------------------------------
        await r.write('.difflab/plan/example/PLAN.md', 'unrelated replacement\n')

        // Assert --------------------------------------------------------------
        const lost = (await r.rpc('GetDiff', { target: { directory: { path: plans } } })).diff

        expect(
          (
            await r.rpc('GetFile', {
              diff: lost,
              path: 'PLAN.md',
              anchors: [{ original: line(bytes, 'PLAN.md') }],
            })
          ).placements[0].outdated.reason,
        ).toEqual(expect.any(String))

        // Act -----------------------------------------------------------------
        await r.write('.difflab/plan/example/PLAN.md', bytes)
      },
    )

    const review = (
      await r.rpc('StartReview', {
        target: { directory: { path: plans } },
        requestKey: 'directory',
      })
    ).review
    const scope = { reviewId: review.id }
    let root: Json
    await step(
      'When a review has line feedback, a reply and file feedback, pages contain each comment once and filters preserve discussions',
      async () => {
        // Act -----------------------------------------------------------------
        root = (
          await r.rpc('AddComment', {
            scope,
            anchor: line(bytes, 'PLAN.md'),
            body: 'Handle this case',
            expectedVersion: review.version,
            requestKey: 'root',
          })
        ).comment
        let current = await r.rpc('GetConversation', { scope })
        await r.rpc('Reply', {
          scope,
          threadId: root.threadId,
          body: 'Keep the fallback',
          expectedVersion: current.threads[0].version,
          requestKey: 'reply',
        })
        await r.write('.difflab/plan/example/two.md', 'second file\n')
        current = await r.rpc('GetConversation', { scope })
        await r.rpc('AddComment', {
          scope,
          anchor: { kind: 'file', path: 'two.md' },
          body: 'Explain this file',
          expectedVersion: current.version,
          requestKey: 'file',
        })

        // Arrange -------------------------------------------------------------
        const comments: Json[] = []
        let token = ''
        do {
          // Arrange -----------------------------------------------------------
          const page = await r.rpc('GetConversation', { scope, pageSize: 1, pageToken: token })
          comments.push(...(page.comments ?? []))
          token = page.nextPageToken ?? ''
        } while (token)

        // Assert --------------------------------------------------------------
        expect(comments).toHaveLength(3)
        expect(new Set(comments.map((c) => c.id)).size).toBe(3)
        const visible = await r.rpc('GetConversation', { scope, filePaths: ['PLAN.md'] })
        expect(visible.comments).toHaveLength(2)
        expect(visible.threads[0].orphanReason).toBeFalsy()
      },
    )

    await step(
      'Then confirmed file moves and deletion preserve root/reply identity and immutable anchors',
      async () => {
        // Arrange -------------------------------------------------------------
        const before = await r.rpc('GetConversation', { scope })

        // Act -----------------------------------------------------------------
        await mkdir(join(plans, 'nested'))
        await rename(join(plans, 'PLAN.md'), join(plans, 'nested/PLAN.md'))
        const moved = await r.rpc('RefreshReview', {
          scope,
          expectedVersion: before.version,
          requestKey: 'move-file',
          sourceChanges: [
            {
              moved: {
                threadId: root.threadId,
                previousPath: 'PLAN.md',
                currentPath: 'nested/PLAN.md',
                source: { localFile: { contentHash: hash(bytes) } },
              },
            },
          ],
        })

        // Assert --------------------------------------------------------------
        const visible = await r.rpc('GetConversation', { scope, filePaths: ['nested/PLAN.md'] })

        expect(visible.comments).toHaveLength(2)
        expect(visible.threads[0]).toMatchObject({
          id: root.threadId,
          currentPath: 'nested/PLAN.md',
          anchor: { path: 'PLAN.md' },
        })

        // Act -----------------------------------------------------------------
        await rm(join(plans, 'nested/PLAN.md'))
        await r.rpc('RefreshReview', {
          scope,
          expectedVersion: moved.version,
          requestKey: 'delete-file',
          sourceChanges: [{ deleted: { threadId: root.threadId, path: 'nested/PLAN.md' } }],
        })

        // Assert --------------------------------------------------------------
        const retained = await r.rpc('GetConversation', { scope })

        expect(retained.comments).toHaveLength(3)
        expect(retained.threads.find((t: Json) => t.id === root.threadId)).toMatchObject({
          orphanReason: 'file_deleted',
          anchor: { evidence: { selectedText: 'selected' } },
        })
      },
    )

    await step(
      'When a standalone note is promoted, the source remains intact and repeated promotion creates one target comment',
      async () => {
        // Act -----------------------------------------------------------------
        const path = await r.write('standalone.md', 'saved\n'),
          source = { notes: { path } }
        const note = (
          await r.rpc('AddComment', {
            scope: source,
            body: 'Promote this feedback',
            expectedVersion: '0',
            requestKey: 'note',
          })
        ).comment
        const before = await r.rpc('GetConversation', { scope: source }),
          current = await r.rpc('GetConversation', { scope })

        // Arrange -------------------------------------------------------------
        const request = {
          source,
          targetReviewId: review.id,
          commentIds: [note.id],
          anchors: [{ kind: 'file', path: 'two.md' }],
          expectedSourceVersion: before.version,
          expectedReviewVersion: current.version,
          requestKey: 'promote',
        }

        // Act -----------------------------------------------------------------
        const promoted = await r.rpc('PromoteNotes', request)

        // Assert --------------------------------------------------------------
        expect(promoted.comments[0].id).not.toBe(note.id)
        expect(promoted.comments[0].author).toEqual(note.author)
        expect(promoted.acceptedVersion).toBeTruthy()

        // Act -----------------------------------------------------------------
        const replayedPromotion = await r.rpc('PromoteNotes', request)

        // Assert --------------------------------------------------------------
        expect(replayedPromotion.comments[0].id).toBe(promoted.comments[0].id)
        const after = await r.rpc('GetConversation', { scope: source })
        expect(after.comments[0]).toMatchObject({ id: note.id, body: note.body })
        const targetBefore = await r.rpc('GetConversation', { scope })
        expect(
          (await r.rpc('Export', { scope, commentIds: [promoted.comments[0].id] })).markdown,
        ).toContain('Promote this feedback')
        expect(await r.rpc('GetConversation', { scope })).toEqual(targetBefore)
      },
    )

    const destination = join(r.workspace, '.difflab/plan/renamed')
    await step(
      'Then moving the review root retains the review, and completion is idempotent across restart',
      async () => {
        // Arrange -------------------------------------------------------------
        const before = await r.rpc('GetConversation', { scope })

        // Act -----------------------------------------------------------------
        await rename(plans, destination)
        await r.rpc('RefreshReview', {
          scope,
          expectedVersion: before.version,
          requestKey: 'root-move',
          sourceChanges: [{ locationMoved: { previousPath: plans, currentPath: destination } }],
        })

        // Assert --------------------------------------------------------------
        expect(
          (await r.rpc('GetReview', { reviewId: review.id })).review.target.directory.path,
        ).toBe(destination)

        // Act -----------------------------------------------------------------
        const resumedReview = await r.rpc('StartReview', {
          target: { directory: { path: destination } },
          requestKey: 'resume-moved',
        })

        // Assert --------------------------------------------------------------
        expect(resumedReview.review.id).toBe(review.id)

        const current = await r.rpc('GetConversation', { scope })

        // Arrange -------------------------------------------------------------
        const request = {
          reviewId: review.id,
          expectedVersion: current.version,
          requestKey: 'complete',
          body: 'Overall feedback',
        }

        // Act -----------------------------------------------------------------
        const completionResult = await r.rpc('CompleteReview', request)

        // Assert --------------------------------------------------------------
        expect(completionResult.review.state).toBe('completed')

        // Act -----------------------------------------------------------------
        const replayedCompletion = await r.rpc('CompleteReview', request)

        // Assert --------------------------------------------------------------
        expect(replayedCompletion.review.id).toBe(review.id)

        // Act -----------------------------------------------------------------
        await r.restart()

        // Assert --------------------------------------------------------------
        expect((await r.rpc('GetReview', { reviewId: review.id })).review.state).toBe('completed')
        expect((await r.rpc('GetConversation', { scope })).comments).toHaveLength(4)
      },
    )
  })

  it('shares staged and branch feedback, preserves source evidence, and exports actionable Markdown', async ({
    runtime: r,
  }) => {
    // Arrange -----------------------------------------------------------------
    await r.identify()

    await r.git('init', '-q', '-b', 'main')
    const path = 'src/[x]`Ω.rs',
      old = 'before\nselected old\n',
      next = 'before\nselected new\n'
    await r.write(path, old)
    await r.write('other.md', 'file feedback target\n')
    const base = await r.commit('base')
    await r.git('checkout', '-qb', 'feature')
    await r.write(path, next)
    await r.git('add', '.')
    const target = { localGit: { path: r.workspace } }
    const review = (
      await r.rpc('StartReview', {
        target,
        comparison: { staged: { baseCommit: base } },
        requestKey: 'staged',
      })
    ).review
    const scope = { reviewId: review.id }

    // Assert ------------------------------------------------------------------
    expect(review.target.localGit.branch).toMatch(/^refs\/heads\//)

    // Act ---------------------------------------------------------------------
    const root = (
      await r.rpc('AddComment', {
        scope,
        body: 'Handle this failure before returning.\n\n```rust\nreturn recover(error);\n```',
        anchor: {
          kind: 'line',
          path,
          side: 'old',
          start: '2',
          end: '2',
          source: { commit: { sha: base, contentHash: hash(old) } },
          evidence: { selectedText: 'selected old', contextBefore: 'before' },
        },
        expectedVersion: review.version,
        requestKey: 'line',
      })
    ).comment

    await step(
      'Given an active staged review, switching views requires an explicit guarded replacement',
      async () => {
        // Assert --------------------------------------------------------------
        await r.reject('StartReview', {
          target,
          comparison: { working: { baseCommit: base } },
          requestKey: 'implicit-switch',
        })

        const current = await r.rpc('GetConversation', { scope })

        // Act -----------------------------------------------------------------
        const head = await r.commit('reviewed change')

        // Arrange -------------------------------------------------------------
        const replacement = {
          target,
          comparison: { branch: { baseCommit: base, headCommit: head } },
          resumePolicy: 'RESUME_POLICY_REPLACE_LOCAL',
          expectedReviewId: review.id,
          expectedVersion: current.version,
          requestKey: 'branch',
        }

        // Act -----------------------------------------------------------------
        const branch = (await r.rpc('StartReview', replacement)).review

        // Assert --------------------------------------------------------------
        expect(branch.id).not.toBe(review.id)
        expect((await r.rpc('GetReview', { reviewId: review.id })).review.state).toBe('completed')

        // Act -----------------------------------------------------------------
        const replayedReplacement = await r.rpc('StartReview', replacement)

        // Assert --------------------------------------------------------------
        expect(replayedReplacement.review.id).toBe(branch.id)
        scope.reviewId = branch.id
        const shared = await r.rpc('GetConversation', { scope })
        expect(shared.comments[0].id).toBe(root.id)

        const before = shared.version

        await r.reject('StartReview', {
          ...replacement,
          expectedVersion: before,
          comparison: { working: { baseCommit: head } },
          requestKey: 'stale-attempt',
        })
        expect((await r.rpc('GetConversation', { scope })).version).toBe(before)
      },
    )

    // Arrange -----------------------------------------------------------------
    let fileComment: Json, general: Json
    await step(
      'When the reviewer adds replies, file feedback and general feedback, all remain in the branch context',
      async () => {
        // Arrange -------------------------------------------------------------
        let current = await r.rpc('GetConversation', { scope })

        // Act -----------------------------------------------------------------
        await r.rpc('Reply', {
          scope,
          threadId: root.threadId,
          body: 'Add a regression test for the timeout case.',
          expectedVersion: current.threads[0].version,
          requestKey: 'reply',
        })
        current = await r.rpc('GetConversation', { scope })
        const head = await r.git('rev-parse', 'HEAD')
        fileComment = (
          await r.rpc('AddComment', {
            scope,
            body: 'Keep this explanation in the replacement.',
            anchor: {
              kind: 'file',
              path: 'other.md',
              source: { commit: { sha: head, contentHash: hash('file feedback target\n') } },
              evidence: {},
            },
            expectedVersion: current.version,
            requestKey: 'file',
          })
        ).comment
        current = await r.rpc('GetConversation', { scope })
        general = (
          await r.rpc('AddComment', {
            scope,
            body: 'Check the whole failure path.',
            expectedVersion: current.version,
            requestKey: 'general',
          })
        ).comment
      },
    )

    await step(
      'Then observations from an unrelated checkout branch cannot orphan this branch’s discussions',
      async () => {
        // Arrange -------------------------------------------------------------
        const before = await r.rpc('GetConversation', { scope })

        // Act -----------------------------------------------------------------
        await r.git('checkout', '-qb', 'unrelated-checkout')
        await r.git('rm', '--', path)

        // Assert --------------------------------------------------------------
        await r.reject('RefreshReview', {
          scope,
          expectedVersion: before.version,
          requestKey: 'wrong-branch',
          sourceChanges: [{ deleted: { threadId: root.threadId, path } }],
        })
        expect(await r.rpc('GetConversation', { scope })).toEqual(before)

        // Act -----------------------------------------------------------------
        await r.git('reset', '--hard', 'HEAD')
        await r.git('checkout', 'feature')
      },
    )

    await step(
      'When sources move or disappear, exports show original locations, replies and final feedback without database metadata',
      async () => {
        // Act -----------------------------------------------------------------
        await rename(join(r.workspace, path), join(r.workspace, 'src/moved.md'))
        await rm(join(r.workspace, 'other.md'))
        let current = await r.rpc('GetConversation', { scope })
        await r.rpc('RefreshReview', {
          scope,
          expectedVersion: current.version,
          requestKey: 'source-events',
          sourceChanges: [
            {
              moved: {
                threadId: root.threadId,
                previousPath: path,
                currentPath: 'src/moved.md',
                source: { workingFile: { contentHash: hash(next) } },
              },
            },
            { deleted: { threadId: fileComment.threadId, path: 'other.md' } },
          ],
        })
        current = await r.rpc('GetConversation', { scope })
        await r.rpc('CompleteReview', {
          reviewId: scope.reviewId,
          expectedVersion: current.version,
          requestKey: 'complete',
          body: 'Please address these points before merging.',
        })
        const before = await r.rpc('GetConversation', { scope })

        // Assert --------------------------------------------------------------
        const markdown = (
          await r.rpc('Export', { scope, commentIds: [root.id, fileComment.id, general.id] })
        ).markdown

        expect(markdown).toMatch(/^# Review feedback\n\n/)
        expect(markdown.match(/\*\*\[COMMENT\]\*\*/g)).toHaveLength(3)
        for (const expected of [
          '`src/moved.md`',
          '``src/[x]`Ω.rs:~2``',
          'old (left) side',
          'Add a regression test for the timeout case.',
          'return recover(error);',
          'File deleted.',
          '> selected old',
          '## Overall feedback\n\nPlease address these points before merging.',
        ])
          expect(markdown).toContain(expected)
        for (const forbidden of [
          'Session:',
          'Anchor:',
          'sha256:',
          '<!-- lgtm-',
          review.id,
          root.id,
          base,
        ])
          expect(markdown).not.toContain(forbidden)
        expect(await r.rpc('GetConversation', { scope })).toEqual(before)
      },
    )

    await step('And file-level notes reconnect only to the matching restored source', async () => {
      // Act -------------------------------------------------------------------
      const bytes = 'original file\n',
        file = await r.write('file-note.md', bytes),
        noteScope = { notes: { path: file } }
      const saved = (
        await r.rpc('AddComment', {
          scope: noteScope,
          body: 'Feedback about the file',
          anchor: {
            kind: 'file',
            source: { localFile: { contentHash: hash(bytes) } },
            evidence: {},
          },
          expectedVersion: '0',
          requestKey: 'file-note',
        })
      ).comment

      // Assert ----------------------------------------------------------------
      const original = await r.rpc('GetConversation', { scope: noteScope }),
        thread = original.threads[0]

      expect(thread.anchor.kind).toBe('file')
      expect(thread.anchor.start).toBeFalsy()
      expect(thread.anchor.source.localFile.contentHash).toBe(hash(bytes))

      // Act -------------------------------------------------------------------
      await rm(file)
      const deleted = await r.rpc('RefreshReview', {
        scope: noteScope,
        expectedVersion: original.version,
        requestKey: 'deleted',
        sourceChanges: [{ deleted: { threadId: thread.id, path: 'file-note.md' } }],
      })
      await r.write('file-note.md', 'unrelated replacement\n')

      // Arrange ---------------------------------------------------------------
      const restore = (content: string, key: string) => ({
        scope: noteScope,
        expectedVersion: deleted.version,
        requestKey: key,
        sourceChanges: [
          {
            restored: {
              threadId: thread.id,
              path: 'file-note.md',
              source: { localFile: { contentHash: hash(content) } },
            },
          },
        ],
      })

      // Assert ----------------------------------------------------------------
      await r.reject('RefreshReview', restore('unrelated replacement\n', 'wrong-restore'))
      expect((await r.rpc('GetConversation', { scope: noteScope })).threads[0].orphanReason).toBe(
        'file_deleted',
      )

      // Act -------------------------------------------------------------------
      await r.write('file-note.md', bytes)
      await r.rpc('RefreshReview', restore(bytes, 'matching-restore'))

      // Assert ----------------------------------------------------------------
      const restored = await r.rpc('GetConversation', { scope: noteScope })

      expect(restored.comments[0].id).toBe(saved.id)
      expect(restored.threads[0].anchor).toEqual(thread.anchor)
      expect(restored.threads[0].orphanReason).toBeFalsy()
      expect(await readFile(file, 'utf8')).toBe(bytes)
    })
  })
})
