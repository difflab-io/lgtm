/**
 * @module-tag e2e
 * @module-tag backend
 * @module-tag local-review
 */
import { describe, it, expect } from 'vitest'
import { randomUUID } from 'node:crypto'
import { mkdir, rename, rm } from 'node:fs/promises'
import { join } from 'node:path'
import { useTestHarness } from '../common/fixtures.js'
import { hash, type Json } from '../common/test-utils.js'
import { line } from './anchors.js'
import {
  prepareDirectory,
  prepareDirectoryDiscussion,
  prepareStagedReview,
  prepareBranchDiscussion,
} from './local-review.fixtures.js'

describe('When starting a local directory review', () => {
  const fixture = useTestHarness('backend')

  it(
    'User can review ignored files and immediately persist local draft comments',
    { tags: ['smoke'] },
    async () => {
      // Arrange
      const { runtime } = fixture()
      const { scope, bytes, review } = await prepareDirectory(runtime)

      // Act
      const root = (
        await runtime.rpc('AddComment', {
          scope,
          anchor: line(bytes, 'PLAN.md'),
          body: 'Handle this case',
          expectedVersion: review.version,
          requestKey: randomUUID(),
        })
      ).comment
      const conversation = await runtime.rpc('GetConversation', { scope })

      // Assert
      expect(review.state).toBe('active')
      expect(conversation.comments).toEqual(
        expect.arrayContaining([
          expect.objectContaining({ id: root.id, visibility: 'draft', body: 'Handle this case' }),
        ]),
      )
      expect(conversation.threads[0].anchor.path).toBe('PLAN.md')
    },
  )

  it('Consumers can page discussions and filter them by the selected files', async () => {
    // Arrange
    const { runtime } = fixture()
    const { scope } = await prepareDirectoryDiscussion(runtime)
    const comments: Json[] = []
    let pageToken = ''

    // Act
    do {
      const page = await runtime.rpc('GetConversation', { scope, pageSize: 1, pageToken })
      comments.push(...(page.comments ?? []))
      pageToken = page.nextPageToken ?? ''
    } while (pageToken)
    const selected = await runtime.rpc('GetConversation', { scope, filePaths: ['PLAN.md'] })

    // Assert
    expect(comments).toHaveLength(3)
    expect(new Set(comments.map((comment) => comment.id)).size).toBe(3)
    expect(selected.comments).toHaveLength(2)
    expect(selected.threads[0].orphanReason).toBeFalsy()
  })
})

describe('When files change during a local review', () => {
  const fixture = useTestHarness('backend')

  it('User feedback follows a confirmed file move while preserving its original anchor', async () => {
    // Arrange
    const { runtime } = fixture()
    const { scope, plans, bytes, root } = await prepareDirectoryDiscussion(runtime)
    const before = await runtime.rpc('GetConversation', { scope })
    await mkdir(join(plans, 'nested'))
    await rename(join(plans, 'PLAN.md'), join(plans, 'nested/PLAN.md'))

    // Act
    await runtime.rpc('RefreshReview', {
      scope,
      expectedVersion: before.version,
      requestKey: randomUUID(),
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
    const selected = await runtime.rpc('GetConversation', { scope, filePaths: ['nested/PLAN.md'] })

    // Assert
    expect(selected.comments).toHaveLength(2)
    expect(selected.threads[0]).toMatchObject({
      id: root.threadId,
      currentPath: 'nested/PLAN.md',
      anchor: { path: 'PLAN.md' },
    })
  })

  it('User feedback remains orphaned with its original selection when a file is deleted', async () => {
    // Arrange
    const { runtime } = fixture()
    const { scope, plans, root } = await prepareDirectoryDiscussion(runtime)
    const before = await runtime.rpc('GetConversation', { scope })
    await rm(join(plans, 'PLAN.md'))

    // Act
    await runtime.rpc('RefreshReview', {
      scope,
      expectedVersion: before.version,
      requestKey: randomUUID(),
      sourceChanges: [{ deleted: { threadId: root.threadId, path: 'PLAN.md' } }],
    })
    const retained = await runtime.rpc('GetConversation', { scope })

    // Assert
    expect(retained.comments).toHaveLength(3)
    expect(retained.threads.find((thread: Json) => thread.id === root.threadId)).toMatchObject({
      orphanReason: 'file_deleted',
      anchor: { evidence: { selectedText: 'selected' } },
    })
  })

  it('Moving the review directory preserves its review identity and discussions', async () => {
    // Arrange
    const { runtime } = fixture()
    const { scope, plans, review } = await prepareDirectoryDiscussion(runtime)
    const before = await runtime.rpc('GetConversation', { scope })
    const destination = join(runtime.workspace, '.difflab/plan/renamed')
    await rename(plans, destination)

    // Act
    await runtime.rpc('RefreshReview', {
      scope,
      expectedVersion: before.version,
      requestKey: randomUUID(),
      sourceChanges: [{ locationMoved: { previousPath: plans, currentPath: destination } }],
    })
    await runtime.restart()
    const resumed = await runtime.rpc('StartReview', {
      target: { directory: { path: destination } },
      requestKey: randomUUID(),
    })

    // Assert
    expect(resumed.review.id).toBe(review.id)
    expect(resumed.review.target.directory.path).toBe(destination)
    expect((await runtime.rpc('GetConversation', { scope })).comments).toHaveLength(3)
  })
})

describe('When promoting notes into a local review', () => {
  const fixture = useTestHarness('backend')

  it('User can copy selected notes into a review without losing their author or modifying the source', async () => {
    // Arrange
    const { runtime } = fixture()
    const { scope, review } = await prepareDirectory(runtime)
    const path = await runtime.write('standalone.md', 'saved\n')
    const source = { notes: { path } }
    const note = (
      await runtime.rpc('AddComment', {
        scope: source,
        body: 'Promote this feedback',
        expectedVersion: '0',
        requestKey: randomUUID(),
      })
    ).comment
    const before = await runtime.rpc('GetConversation', { scope: source })
    const current = await runtime.rpc('GetConversation', { scope })
    const request = {
      source,
      targetReviewId: review.id,
      commentIds: [note.id],
      anchors: [{ kind: 'file', path: 'two.md' }],
      expectedSourceVersion: before.version,
      expectedReviewVersion: current.version,
      requestKey: randomUUID(),
    }

    // Act
    const promoted = await runtime.rpc('PromoteNotes', request)
    const replay = await runtime.rpc('PromoteNotes', request)
    const target = await runtime.rpc('GetConversation', { scope })
    const exported = await runtime.rpc('Export', { scope, commentIds: [promoted.comments[0].id] })

    // Assert
    expect(promoted.comments[0].id).not.toBe(note.id)
    expect(promoted.comments[0].author).toEqual(note.author)
    expect(promoted.acceptedVersion).toBeTruthy()
    expect(replay.comments[0].id).toBe(promoted.comments[0].id)
    expect((await runtime.rpc('GetConversation', { scope: source })).comments[0]).toMatchObject({
      id: note.id,
      body: note.body,
    })
    expect(exported.markdown).toContain('Promote this feedback')
    expect(await runtime.rpc('GetConversation', { scope })).toEqual(target)
  })
})

describe('When completing and resuming local reviews', () => {
  const fixture = useTestHarness('backend')

  it('User can complete once, retain the final message and read the completed review after restart', async () => {
    // Arrange
    const { runtime } = fixture()
    const { scope, review } = await prepareDirectoryDiscussion(runtime)
    const current = await runtime.rpc('GetConversation', { scope })
    const request = {
      reviewId: review.id,
      expectedVersion: current.version,
      requestKey: randomUUID(),
      body: 'Overall feedback',
    }

    // Act
    const completed = await runtime.rpc('CompleteReview', request)
    const replay = await runtime.rpc('CompleteReview', request)
    await runtime.restart()
    const saved = await runtime.rpc('GetReview', { reviewId: review.id })
    const exported = await runtime.rpc('Export', { scope })

    // Assert
    expect(completed.review.state).toBe('completed')
    expect(replay.review.id).toBe(review.id)
    expect(saved.review.state).toBe('completed')
    expect(exported.markdown).toContain('Overall feedback')
    expect((await runtime.rpc('GetConversation', { scope })).comments).toHaveLength(3)
  })
})

describe('When staged changes become branch commits', () => {
  const fixture = useTestHarness('backend')

  it('User explicitly replaces the comparison while retaining feedback in the shared local context', async () => {
    // Arrange
    const { runtime } = fixture()
    const { target, base, review, scope, root } = await prepareStagedReview(runtime)
    const implicit = await runtime.response('StartReview', {
      target,
      comparison: { working: { baseCommit: base } },
      requestKey: randomUUID(),
    })
    const current = await runtime.rpc('GetConversation', { scope })
    const head = await runtime.commit('reviewed change')
    const request = {
      target,
      comparison: { branch: { baseCommit: base, headCommit: head } },
      resumePolicy: 'RESUME_POLICY_REPLACE_LOCAL',
      expectedReviewId: review.id,
      expectedVersion: current.version,
      requestKey: randomUUID(),
    }

    // Act
    const branch = (await runtime.rpc('StartReview', request)).review
    const replay = await runtime.rpc('StartReview', request)
    const shared = await runtime.rpc('GetConversation', { scope: { reviewId: branch.id } })
    const stale = await runtime.response('StartReview', {
      ...request,
      comparison: { working: { baseCommit: head } },
      expectedVersion: shared.version,
      requestKey: randomUUID(),
    })

    // Assert
    expect(review.target.localGit.branch).toMatch(/^refs\/heads\//)
    expect(implicit.status).toBeGreaterThanOrEqual(400)
    expect(branch.id).not.toBe(review.id)
    expect(replay.review.id).toBe(branch.id)
    expect((await runtime.rpc('GetReview', { reviewId: review.id })).review.state).toBe('completed')
    expect(shared.comments[0].id).toBe(root.id)
    expect(stale.status).toBeGreaterThanOrEqual(400)
    expect((await runtime.rpc('GetConversation', { scope: { reviewId: branch.id } })).version).toBe(
      shared.version,
    )
  })

  it('An unrelated checkout cannot update a saved branch review’s anchors', async () => {
    // Arrange
    const { runtime } = fixture()
    const { scope, path, root } = await prepareBranchDiscussion(runtime)
    const before = await runtime.rpc('GetConversation', { scope })
    await runtime.git('checkout', '-qb', 'unrelated-checkout')
    await runtime.git('rm', '--', path)

    // Act
    const rejected = await runtime.response('RefreshReview', {
      scope,
      expectedVersion: before.version,
      requestKey: randomUUID(),
      sourceChanges: [{ deleted: { threadId: root.threadId, path } }],
    })

    // Assert
    expect(rejected.status).toBeGreaterThanOrEqual(400)
    expect(await runtime.rpc('GetConversation', { scope })).toEqual(before)
  })
})

describe('When exporting local review feedback', () => {
  const fixture = useTestHarness('backend')

  it('Export describes moved and deleted files, replies and final feedback without internal state', async () => {
    // Arrange
    const { runtime } = fixture()
    const { scope, path, next, root, fileComment, general, review, base } =
      await prepareBranchDiscussion(runtime)
    await rename(join(runtime.workspace, path), join(runtime.workspace, 'src/moved.md'))
    await rm(join(runtime.workspace, 'other.md'))
    let current = await runtime.rpc('GetConversation', { scope })
    await runtime.rpc('RefreshReview', {
      scope,
      expectedVersion: current.version,
      requestKey: randomUUID(),
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
    current = await runtime.rpc('GetConversation', { scope })
    await runtime.rpc('CompleteReview', {
      reviewId: scope.reviewId,
      expectedVersion: current.version,
      requestKey: randomUUID(),
      body: 'Please address these points before merging.',
    })
    const before = await runtime.rpc('GetConversation', { scope })

    // Act
    const exported = await runtime.rpc('Export', {
      scope,
      commentIds: [root.id, fileComment.id, general.id],
    })

    // Assert
    expect(exported.markdown).toMatch(/^# Review feedback\n\n/)
    expect(exported.markdown.match(/\*\*\[COMMENT\]\*\*/g)).toHaveLength(3)
    for (const text of [
      '`src/moved.md`',
      '``src/[x]`Ω.rs:~2``',
      'old (left) side',
      'Add a regression test for the timeout case.',
      'return recover(error);',
      'File deleted.',
      '> selected old',
      '## Overall feedback\n\nPlease address these points before merging.',
    ])
      expect(exported.markdown).toContain(text)
    for (const text of ['Session:', 'Anchor:', 'sha256:', '<!-- lgtm-', review.id, root.id, base])
      expect(exported.markdown).not.toContain(text)
    expect(await runtime.rpc('GetConversation', { scope })).toEqual(before)
  })
})
