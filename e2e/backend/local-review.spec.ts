/**
 * @module-tag e2e
 * @module-tag backend
 * @module-tag local-review
 */
import { describe, it, expect } from 'vitest'
import { randomUUID } from 'node:crypto'
import { rename, rm } from 'node:fs/promises'
import { join } from 'node:path'
import { useTestHarness } from '../common/fixtures.js'
import { type Json } from '../common/test-utils.js'
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
      const { scope, review } = await prepareDirectory(runtime)

      // Act
      const root = (
        await runtime.rpc('AddComment', {
          scope,
          anchor: line(review.snapshot.id, 'PLAN.md'),
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

  it.each(['rename', 'delete'] as const)(
    'Saved source and discussions survive a file %s',
    async (change) => {
      // Arrange
      const { runtime } = fixture()
      const { scope, plans, review, root } = await prepareDirectoryDiscussion(runtime)
      if (change === 'rename') await rename(join(plans, 'PLAN.md'), join(plans, 'MOVED.md'))
      else await rm(join(plans, 'PLAN.md'))

      // Act
      const saved = (await runtime.rpc('GetReview', { reviewId: review.id })).review
      const diff = (await runtime.rpc('GetDiff', { reviewId: review.id })).diff
      const file = await runtime.rpc('GetFile', { diff, path: 'PLAN.md' })
      const before = await runtime.rpc('GetConversation', { scope })
      const refreshed = (
        await runtime.rpc('RefreshReview', {
          scope,
          expectedVersion: before.version,
          requestKey: randomUUID(),
        })
      ).review
      const historical = (
        await runtime.rpc('GetDiff', { reviewId: review.id, revisionId: review.snapshot.id })
      ).diff
      const discussion = await runtime.rpc('GetConversation', {
        scope,
        snapshotId: review.snapshot.id,
      })

      // Assert
      expect(saved.snapshot.id).toBe(review.snapshot.id)
      expect(file.newSide.content).toContain('selected')
      expect(refreshed.id).toBe(review.id)
      expect(refreshed.snapshot.id).not.toBe(review.snapshot.id)
      expect(
        (await runtime.rpc('GetFile', { diff: historical, path: 'PLAN.md' })).newSide.content,
      ).toContain('selected')
      expect(discussion.comments).toHaveLength(3)
      expect(discussion.threads.find((thread: Json) => thread.id === root.threadId)).toMatchObject({
        anchor: { path: 'PLAN.md', snapshotId: review.snapshot.id },
        displayLocation: { path: 'PLAN.md', start: '2' },
      })
    },
  )

  it('Moving the original directory cannot break resume, replies or export', async () => {
    // Arrange
    const { runtime } = fixture()
    const { scope, plans, review, root } = await prepareDirectoryDiscussion(runtime)
    await rename(plans, join(runtime.workspace, '.difflab/plan/renamed'))
    await runtime.restart()

    // Act
    const saved = (await runtime.rpc('GetReview', { reviewId: review.id })).review
    const conversation = await runtime.rpc('GetConversation', { scope })
    const thread = conversation.threads.find((thread: Json) => thread.id === root.threadId)
    await runtime.rpc('Reply', {
      scope,
      threadId: thread.id,
      body: 'Reply after move',
      expectedVersion: thread.version,
      requestKey: randomUUID(),
    })
    const exported = await runtime.rpc('Export', { scope })

    // Assert
    expect(saved.snapshot).toEqual(review.snapshot)
    expect(exported.markdown).toContain('Reply after move')
    expect((await runtime.rpc('GetConversation', { scope })).comments).toHaveLength(4)
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

  it('An unrelated checkout cannot change saved source or comment locations', async () => {
    // Arrange
    const { runtime } = fixture()
    const { scope, path, root } = await prepareBranchDiscussion(runtime)
    const before = await runtime.rpc('GetConversation', { scope })
    await runtime.git('checkout', '-qb', 'unrelated-checkout')
    await runtime.git('rm', '--', path)

    // Act
    const saved = (await runtime.rpc('GetReview', { reviewId: scope.reviewId })).review
    const diff = (await runtime.rpc('GetDiff', { reviewId: scope.reviewId })).diff
    const source = await runtime.rpc('GetFile', { diff, path })

    // Assert
    expect(source.oldSide.content).toContain('selected old')
    expect(saved.id).toBe(scope.reviewId)
    expect(
      (await runtime.rpc('GetConversation', { scope })).threads.find(
        (thread: Json) => thread.id === root.threadId,
      ).anchor,
    ).toEqual(before.threads.find((thread: Json) => thread.id === root.threadId).anchor)
  })
})

describe('When exporting local review feedback', () => {
  const fixture = useTestHarness('backend')

  it('Export preserves original source, replies and final feedback after workspace changes', async () => {
    // Arrange
    const { runtime } = fixture()
    const { scope, path, root, fileComment, general, review, base } =
      await prepareBranchDiscussion(runtime)
    await rename(join(runtime.workspace, path), join(runtime.workspace, 'src/moved.md'))
    await rm(join(runtime.workspace, 'other.md'))
    const current = await runtime.rpc('GetConversation', { scope })
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
      '``src/[x]`Ω.rs:~2``',
      'old (left) side',
      'Add a regression test for the timeout case.',
      'return recover(error);',
      '> selected old',
      '## Overall feedback\n\nPlease address these points before merging.',
    ])
      expect(exported.markdown).toContain(text)
    for (const text of ['Session:', 'Anchor:', 'sha256:', '<!-- lgtm-', review.id, root.id, base])
      expect(exported.markdown).not.toContain(text)
    expect(await runtime.rpc('GetConversation', { scope })).toEqual(before)
  })
})
