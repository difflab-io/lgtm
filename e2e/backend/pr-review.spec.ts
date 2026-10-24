/**
 * @module-tag e2e
 * @module-tag backend
 * @module-tag github
 * @module-tag pr-review
 */
import { describe, it, expect } from 'vitest'
import { randomUUID } from 'node:crypto'
import { usePrFixture } from '../common/pr.fixtures.js'
import { githubCredentials } from '../common/github.js'
import type { Json } from '../common/test-utils.js'
import {
  prepareReview,
  saveDraft,
  refreshReview,
  submitReview,
  readPendingComment,
} from './pr-review.fixtures.js'

const secondaryUser = process.env.LGTM_E2E_GITHUB_SECONDARY_USER ?? 'skapoor81'

describe('When starting a fresh draft review', () => {
  const fixture = usePrFixture('backend')

  it('User can start a review for the selected PR', async () => {
    // Arrange
    const { runtime, github } = fixture()
    await runtime.identify()
    const target = {
      pullRequest: { repositoryUrl: github.repositoryUrl, number: String(github.number) },
    }

    // Act
    const result = await runtime.rpc('StartReview', { target, requestKey: randomUUID() })

    // Assert
    expect(result.review).toMatchObject({
      state: 'active',
      target,
      submissionStatus: 'unsubmitted',
    })
    expect(
      (await runtime.rpc('GetConversation', { scope: { reviewId: result.review.id } })).comments ??
        [],
    ).toHaveLength(0)
  })

  it('Reopening the same PR resumes the existing review', { tags: ['extended'] }, async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { target, review } = await prepareReview(runtime, github)

    // Act
    const resumed = await runtime.rpc('StartReview', { target, requestKey: randomUUID() })

    // Assert
    expect(resumed.review.id).toBe(review.id)
  })

  it.each(['old', 'new'])(
    'User can immediately persist an inline draft on the %s diff side',
    async (side) => {
      // Arrange
      const { runtime, github } = fixture()
      const { scope, file, anchor } = await prepareReview(runtime, github)
      const selection =
        side === 'old'
          ? {
              ...anchor,
              side,
              source: file.oldSide.source,
              evidence: { ...anchor.evidence, selectedText: 'second' },
            }
          : anchor

      // Act
      const saved = await saveDraft(runtime, scope, `Explain the ${side} side`, selection)
      const remote = await readPendingComment(
        runtime,
        github,
        scope.reviewId,
        saved.comment.forgeCommentId,
      )

      // Assert
      expect(saved.syncError).toBeFalsy()
      expect(saved.comment).toMatchObject({ visibility: 'draft', postingStatus: 'confirmed' })
      expect(remote).toMatchObject({
        path: github.path,
        thread: { diffSide: side === 'old' ? 'LEFT' : 'RIGHT' },
        body: expect.stringContaining(`Explain the ${side} side`),
      })
    },
  )

  it('User can immediately persist a file-level draft comment', { tags: ['smoke'] }, async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope } = await prepareReview(runtime, github)

    // Act
    const saved = await saveDraft(runtime, scope, 'Explain this file', {
      kind: 'file',
      path: github.path,
    })
    const remote = await readPendingComment(
      runtime,
      github,
      scope.reviewId,
      saved.comment.forgeCommentId,
    )

    // Assert
    expect(saved.syncError).toBeFalsy()
    expect(remote).toMatchObject({
      path: github.path,
      thread: { subjectType: 'FILE' },
      body: expect.stringContaining('Explain this file'),
    })
  })

  it(
    'User can save general feedback without inventing a file anchor',
    { tags: ['extended'] },
    async () => {
      // Arrange
      const { runtime, github } = fixture()
      const { scope } = await prepareReview(runtime, github)

      // Act
      const saved = await saveDraft(runtime, scope, 'Overall feedback', { kind: 'general' })
      await runtime.restart()
      const conversation = await runtime.rpc('GetConversation', { scope })

      // Assert
      expect(conversation.comments).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            id: saved.comment.id,
            body: 'Overall feedback',
            visibility: 'draft',
          }),
        ]),
      )
      const thread = conversation.threads.find(
        (thread: Json) => thread.id === saved.comment.threadId,
      )
      expect(thread).toMatchObject({ id: saved.comment.threadId, anchor: { kind: 'general' } })
    },
  )

  it('Agent can immediately persist draft comments with an identity notice', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor } = await prepareReview(runtime, github, {
      name: 'Review agent',
      model: 'test-model',
      provider: 'test-provider',
    })

    // Act
    const saved = await saveDraft(runtime, scope, 'Agent feedback', anchor)
    const remote = await readPendingComment(
      runtime,
      github,
      scope.reviewId,
      saved.comment.forgeCommentId,
    )

    // Assert
    expect(saved.comment.author).toMatchObject({
      name: 'Review agent',
      model: 'test-model',
      provider: 'test-provider',
    })
    for (const text of ['Agent feedback', 'Review agent', 'test-model', 'test-provider'])
      expect(remote.body).toContain(text)
  })

  it('Agent notes can be promoted as remote drafts without changing the original note', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, review, anchor } = await prepareReview(runtime, github)
    const promoter = runtime.actor
    await runtime.identify({
      name: 'Planning agent',
      model: 'test-model',
      provider: 'test-provider',
    })
    const path = await runtime.write('agent-note.md', 'agent source\n')
    const source = { notes: { path } }
    const note = (
      await runtime.rpc('AddComment', {
        scope: source,
        body: 'Promote agent concern',
        expectedVersion: '0',
        requestKey: randomUUID(),
      })
    ).comment
    await runtime.identify({ name: 'E2E user', email: 'e2e@example.test' })
    const before = await runtime.rpc('GetConversation', { scope: source })
    const current = await runtime.rpc('GetConversation', { scope })

    // Act
    const promoted = await runtime.rpc('PromoteNotes', {
      source,
      targetReviewId: review.id,
      commentIds: [note.id],
      anchors: [anchor],
      expectedSourceVersion: before.version,
      expectedReviewVersion: current.version,
      requestKey: randomUUID(),
    })
    const remote = await readPendingComment(
      runtime,
      github,
      review.id,
      promoted.comments[0].forgeCommentId,
    )

    // Assert
    expect(promoted.comments[0].author).toEqual(note.author)
    expect(promoted.comments[0].ownerId).toBe(promoter)
    expect(promoted.comments[0].author.id).not.toBe(promoter)
    for (const text of ['Promote agent concern', 'Planning agent', 'test-model', 'test-provider'])
      expect(remote.body).toContain(text)
    expect((await runtime.rpc('GetConversation', { scope: source })).comments[0]).toMatchObject({
      id: note.id,
      body: note.body,
    })
  })

  it('Retrying an accepted draft after a push and restart does not create another remote comment', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor, review } = await prepareReview(runtime, github)
    const current = await runtime.rpc('GetConversation', { scope })
    const request = {
      scope,
      anchor,
      body: 'Save exactly once',
      expectedVersion: current.version,
      requestKey: randomUUID(),
    }
    const accepted = await runtime.rpc('AddComment', request)
    const pending = (await runtime.rpc('GetReview', { reviewId: review.id })).review.forgeReviewId
    await github.changeFiles({ [github.path]: 'inserted\n' + github.headContent })
    await runtime.restart()

    // Act
    const replay = await runtime.rpc('AddComment', request)
    await refreshReview(runtime, scope)
    const remote = await github.api<Json[]>(
      `repos/${github.repository}/pulls/${github.number}/reviews/${pending}/comments`,
    )

    // Assert
    expect(replay.comment.id).toBe(accepted.comment.id)
    expect(remote).toHaveLength(1)
    expect(String(remote[0].id)).toBe(accepted.comment.forgeCommentId)
  })

  it('User can reply to a draft comment', { tags: ['extended'] }, async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor } = await prepareReview(runtime, github)
    const root = (await saveDraft(runtime, scope, 'Root draft', anchor)).comment
    const conversation = await runtime.rpc('GetConversation', { scope })
    const thread = conversation.threads.find((thread: Json) => thread.id === root.threadId)

    // Act
    const reply = await runtime.rpc('Reply', {
      scope,
      threadId: thread.id,
      body: 'Draft reply',
      expectedVersion: thread.version,
      requestKey: randomUUID(),
    })
    const remote = await readPendingComment(
      runtime,
      github,
      scope.reviewId,
      reply.comment.forgeCommentId,
    )

    // Assert
    expect(reply.syncError).toBeFalsy()
    expect(reply.comment).toMatchObject({ threadId: root.threadId, visibility: 'draft' })
    expect(remote.body).toContain('Draft reply')
  })

  it('User can edit their own draft comment', { tags: ['extended'] }, async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor } = await prepareReview(runtime, github)
    const root = (await saveDraft(runtime, scope, 'Original draft', anchor)).comment
    const current = await runtime.rpc('GetConversation', { scope })
    root.version = current.comments.find((comment: Json) => comment.id === root.id).version

    // Act
    await runtime.rpc('EditComment', {
      scope,
      commentId: root.id,
      body: 'Edited draft',
      expectedVersion: root.version,
      requestKey: randomUUID(),
    })
    const remote = await readPendingComment(runtime, github, scope.reviewId, root.forgeCommentId)

    // Assert
    expect(remote.body).toContain('Edited draft')
    expect(remote.body).not.toContain('Original draft')
  })

  it('User cannot resolve an unpublished draft thread', { tags: ['extended'] }, async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor } = await prepareReview(runtime, github)
    const root = (await saveDraft(runtime, scope, 'Unpublished draft', anchor)).comment
    const conversation = await runtime.rpc('GetConversation', { scope })
    const thread = conversation.threads.find((thread: Json) => thread.id === root.threadId)

    // Act
    const rejected = await runtime.response('ResolveThread', {
      scope,
      threadId: thread.id,
      resolved: true,
      expectedVersion: thread.version,
      requestKey: randomUUID(),
    })
    const retained = await runtime.rpc('GetConversation', { scope })

    // Assert
    expect(rejected.status).toBeGreaterThanOrEqual(400)
    expect(rejected.body.message).toMatch(/draft|unpublished/i)
    expect(retained.threads.find((thread: Json) => thread.id === root.threadId).state).toBe('open')
  })

  it(
    'User can delete a draft comment without leaving a placeholder',
    { tags: ['extended'] },
    async () => {
      // Arrange
      const { runtime, github } = fixture()
      const { scope, anchor } = await prepareReview(runtime, github)
      const root = (await saveDraft(runtime, scope, 'Delete this draft', anchor)).comment
      const current = await runtime.rpc('GetConversation', { scope })
      root.version = current.comments.find((comment: Json) => comment.id === root.id).version

      // Act
      await runtime.rpc('DeleteComment', {
        scope,
        commentId: root.id,
        expectedVersion: root.version,
        requestKey: randomUUID(),
      })
      await refreshReview(runtime, scope)
      const conversation = await runtime.rpc('GetConversation', { scope })
      const exported = await runtime.rpc('Export', { scope })

      // Assert
      expect(conversation.comments ?? []).toHaveLength(0)
      expect(conversation.threads ?? []).toHaveLength(0)
      expect(exported.markdown).not.toContain('Delete this draft')
      expect(exported.markdown).not.toContain('comment deleted')
    },
  )
})

describe('When resuming an existing draft review', () => {
  const fixture = usePrFixture('backend')

  it(
    'User continues to see existing GitHub drafts after a backend restart',
    { tags: ['extended'] },
    async () => {
      // Arrange
      const { runtime, github } = fixture()
      const { scope, anchor, target, review } = await prepareReview(runtime, github)
      const root = (await saveDraft(runtime, scope, 'Persist through restart', anchor)).comment

      // Act
      await runtime.restart()
      const resumed = (await runtime.rpc('StartReview', { target, requestKey: randomUUID() }))
        .review
      await refreshReview(runtime, scope)
      const conversation = await runtime.rpc('GetConversation', { scope })

      // Assert
      expect(resumed).toMatchObject({ id: review.id, forgeReviewId: expect.any(String) })
      expect(conversation.comments).toEqual(
        expect.arrayContaining([
          expect.objectContaining({
            id: root.id,
            body: 'Persist through restart',
            visibility: 'draft',
          }),
        ]),
      )
    },
  )

  it(
    'User can add more drafts to the same pending GitHub review',
    { tags: ['extended'] },
    async () => {
      // Arrange
      const { runtime, github } = fixture()
      const { scope, anchor, review } = await prepareReview(runtime, github)
      await saveDraft(runtime, scope, 'First draft', anchor)
      const before = (await runtime.rpc('GetReview', { reviewId: review.id })).review
      await runtime.restart()

      // Act
      await saveDraft(runtime, scope, 'Second draft', { kind: 'file', path: github.path })
      const after = (await runtime.rpc('GetReview', { reviewId: review.id })).review
      const remote = await github.api(
        `repos/${github.repository}/pulls/${github.number}/reviews/${after.forgeReviewId}/comments`,
      )

      // Assert
      expect(after.forgeReviewId).toBe(before.forgeReviewId)
      expect(remote).toHaveLength(2)
    },
  )

  it(
    'Drafts deleted directly on GitHub disappear from refresh and export',
    { tags: ['extended'] },
    async () => {
      // Arrange
      const { runtime, github } = fixture()
      const { scope, anchor } = await prepareReview(runtime, github)
      const root = (await saveDraft(runtime, scope, 'Externally deleted draft', anchor)).comment
      await github.api(`repos/${github.repository}/pulls/comments/${root.forgeCommentId}`, 'DELETE')

      // Act
      await refreshReview(runtime, scope)
      await runtime.restart()
      const conversation = await runtime.rpc('GetConversation', { scope })
      const exported = await runtime.rpc('Export', { scope })

      // Assert
      expect(conversation.comments ?? []).toHaveLength(0)
      expect(exported.markdown).not.toContain('Externally deleted draft')
      expect(exported.markdown).not.toContain('comment deleted')
    },
  )
})

describe('When there are changes during a PR review', () => {
  const fixture = usePrFixture('backend')

  it.each([
    ['an unrelated file changes', 'unrelated'],
    ['the target hunk is unaffected', 'unaffected'],
    ['lines are inserted before the selection', 'shifted'],
  ])('User comment placement is preserved when %s', async (_label, change) => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor, review } = await prepareReview(runtime, github)
    const root = (await saveDraft(runtime, scope, 'Keep this placement', anchor)).comment
    const pending = (await runtime.rpc('GetReview', { reviewId: review.id })).review.forgeReviewId
    const text =
      change === 'shifted' ? 'inserted\n' + github.headContent : github.headContent + 'new tail\n'
    await github.changeFiles(
      change === 'unrelated'
        ? { [`${github.run}-other.txt`]: 'unrelated\n' }
        : { [github.path]: text },
    )

    // Act
    await refreshReview(runtime, scope)
    const current = (await runtime.rpc('GetReview', { reviewId: review.id })).review
    const conversation = await runtime.rpc('GetConversation', { scope })
    const thread = conversation.threads.find((thread: Json) => thread.id === root.threadId)

    // Assert
    expect(current).toMatchObject({
      id: review.id,
      forgeReviewId: pending,
      comparison: { pullRequest: { headCommit: github.head } },
    })
    expect(conversation.comments.find((comment: Json) => comment.id === root.id).body).toBe(
      'Keep this placement',
    )
    expect(thread.state).toBe('open')
    expect(thread.currentPath).toBe(github.path)
    expect(thread.anchor.start).toBe('2')
    expect(thread.forgeObservation.current.start).toBe(change === 'shifted' ? '3' : '2')
  })

  it('Saving a selection after an unseen push automatically updates the PR without replacing the review', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor, review } = await prepareReview(runtime, github)
    await github.changeFiles({ [github.path]: 'inserted\n' + github.headContent })

    // Act
    const saved = await saveDraft(runtime, scope, 'Continue without a manual refresh', anchor)
    expect(saved.syncError).toBeFalsy()
    const remote = await readPendingComment(
      runtime,
      github,
      review.id,
      saved.comment.forgeCommentId,
    )
    const current = (await runtime.rpc('GetReview', { reviewId: review.id })).review

    // Assert
    expect(saved.syncError).toBeFalsy()
    expect(remote.thread.line).toBe(3)
    expect(current.id).toBe(review.id)
    expect(current.comparison.pullRequest.headCommit).toBe(github.head)
    const conversation = await runtime.rpc('GetConversation', { scope })
    expect(
      conversation.threads.find((thread: Json) => thread.id === saved.comment.threadId).anchor
        .evidence.selectedText,
    ).toBe('changed')
  })

  it('User comment follows a renamed file', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor } = await prepareReview(runtime, github)
    const root = (await saveDraft(runtime, scope, 'Follow the renamed file', anchor)).comment
    const renamed = `${github.run}-renamed.txt`
    await github.changeFiles({ [github.path]: null, [renamed]: github.headContent })

    // Act
    await refreshReview(runtime, scope)
    const conversation = await runtime.rpc('GetConversation', { scope })
    const thread = conversation.threads.find((thread: Json) => thread.id === root.threadId)

    // Assert
    expect(thread.currentPath).toBe(renamed)
    expect(thread.anchor.path).toBe(github.path)
    expect(thread.anchor.evidence.selectedText).toBe('changed')
  })

  it('A lost selection remains readable as outdated file-level feedback', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor } = await prepareReview(runtime, github)
    const root = (await saveDraft(runtime, scope, 'Keep feedback after losing its line', anchor))
      .comment
    await github.changeFiles({
      [github.path]: github.headContent.replace('changed', 'replacement'),
    })

    // Act
    await refreshReview(runtime, scope)
    const conversation = await runtime.rpc('GetConversation', { scope })
    const thread = conversation.threads.find((thread: Json) => thread.id === root.threadId)
    const exported = await runtime.rpc('Export', { scope })

    // Assert
    expect(thread.forgeObservation.outdated || thread.orphanReason).toBeTruthy()
    expect(thread.anchor.evidence.selectedText).toBe('changed')
    expect(exported.markdown).toContain('Keep feedback after losing its line')
    expect(exported.markdown).toMatch(/outdated|orphaned|original/i)
  })

  it('User comment is retained as orphaned feedback when its file is removed', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor } = await prepareReview(runtime, github)
    const root = (await saveDraft(runtime, scope, 'Keep feedback for the deleted file', anchor))
      .comment
    await github.changeFiles({
      [github.path]: null,
      [`${github.run}-replacement.txt`]: 'replacement\n',
    })

    // Act
    await refreshReview(runtime, scope)
    const conversation = await runtime.rpc('GetConversation', { scope })
    const exported = await runtime.rpc('Export', { scope })

    // Assert
    expect(conversation.comments.find((comment: Json) => comment.id === root.id).body).toBe(
      'Keep feedback for the deleted file',
    )
    expect(conversation.threads.find((thread: Json) => thread.id === root.threadId)).toMatchObject({
      orphanReason: 'file_deleted',
    })
    expect(exported.markdown).toContain(github.path)
  })
})

describe('When submitting a draft review', () => {
  const fixture = usePrFixture('backend')

  it('User can submit comments with a final message', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor, review } = await prepareReview(runtime, github)
    await saveDraft(runtime, scope, 'Inline feedback', anchor)

    // Act
    const { response } = await submitReview(runtime, review.id, 'comment', 'Final user message')
    const remote = await github.api(
      `repos/${github.repository}/pulls/${github.number}/reviews/${response.body.review.forgeReviewId}`,
    )

    // Assert
    expect(response.status).toBe(200)
    expect(response.body.review.submissionStatus).toBe('confirmed')
    expect(remote).toMatchObject({
      state: 'COMMENTED',
      body: expect.stringContaining('Final user message'),
    })
  })

  it.each(['approve', 'request_changes'])(
    'User can submit %s on another author’s PR',
    async (outcome) => {
      // Arrange
      const { runtime, github } = fixture()
      const secondary = await githubCredentials(secondaryUser)
      Object.assign(runtime.env, secondary)
      await runtime.restart()
      const { scope, anchor, review } = await prepareReview(runtime, github, {
        name: 'Second reviewer',
        email: 'sid@cloudvoyant.io',
      })
      await saveDraft(runtime, scope, 'Feedback from another reviewer', anchor)

      // Act
      const { response } = await submitReview(
        runtime,
        review.id,
        outcome,
        'Second reviewer summary',
      )
      const remote = await github.api(
        `repos/${github.repository}/pulls/${github.number}/reviews/${response.body.review.forgeReviewId}`,
        'GET',
        undefined,
        secondary,
      )

      // Assert
      expect(response.status).toBe(200)
      expect(remote).toMatchObject({
        state: outcome === 'approve' ? 'APPROVED' : 'CHANGES_REQUESTED',
        user: { login: secondaryUser },
      })
      expect(response.body.review.submissionStatus).toBe('confirmed')
    },
  )

  it.each(['approve', 'request_changes'])(
    'User cannot submit %s on their own PR',
    async (outcome) => {
      // Arrange
      const { runtime, github } = fixture()
      const { scope, anchor, review } = await prepareReview(runtime, github)
      const root = (await saveDraft(runtime, scope, 'Retain my draft', anchor)).comment

      // Act
      const { response } = await submitReview(runtime, review.id, outcome, 'Retain final text')
      const current = (await runtime.rpc('GetReview', { reviewId: review.id })).review
      const conversation = await runtime.rpc('GetConversation', { scope })

      // Assert
      expect(response.status).toBeGreaterThanOrEqual(400)
      expect(response.body.message).toContain('own PR')
      expect(current).toMatchObject({ state: 'active', submissionStatus: 'unsubmitted' })
      expect(conversation.comments.find((comment: Json) => comment.id === root.id).visibility).toBe(
        'draft',
      )
    },
  )

  it('User can recover authentication failure without losing feedback', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor, review } = await prepareReview(runtime, github)
    const root = (await saveDraft(runtime, scope, 'Retain through authentication failure', anchor))
      .comment
    const token = runtime.env.GH_TOKEN
    runtime.env.GH_TOKEN = 'invalid-e2e-token'
    await runtime.restart()

    // Act
    const { response: denied } = await submitReview(runtime, review.id)
    runtime.env.GH_TOKEN = token
    await runtime.restart()
    const { response: recovered } = await submitReview(runtime, review.id)
    const conversation = await runtime.rpc('GetConversation', { scope })

    // Assert
    expect(denied.status).toBeGreaterThanOrEqual(400)
    expect(recovered.status).toBe(200)
    expect(recovered.body.review.submissionStatus).toBe('confirmed')
    expect(conversation.comments.find((comment: Json) => comment.id === root.id).body).toBe(
      'Retain through authentication failure',
    )
  })

  it('Retrying an accepted submission after restart does not publish twice', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor, review } = await prepareReview(runtime, github)
    await saveDraft(runtime, scope, 'Submit only once', anchor)
    const { request, response } = await submitReview(runtime, review.id)
    await runtime.restart()

    // Act
    const replay = await runtime.rpc('SubmitReview', request)
    const remote = await github.api(
      `repos/${github.repository}/pulls/${github.number}/reviews/${replay.review.forgeReviewId}/comments`,
    )

    // Assert
    expect(replay.review.forgeReviewId).toBe(response.body.review.forgeReviewId)
    expect(replay.review.submissionStatus).toBe('confirmed')
    expect(remote).toHaveLength(1)
  })
})

describe('When addressing published comments or change requests', () => {
  const fixture = usePrFixture('backend')

  async function publishedConcern() {
    const { runtime, github } = fixture()
    const primary = runtime.env.GH_TOKEN
    const secondary = await githubCredentials(secondaryUser)
    Object.assign(runtime.env, secondary)
    await runtime.restart()
    const review = await prepareReview(runtime, github, {
      name: 'Other reviewer',
      email: 'sid@cloudvoyant.io',
    })
    const root = (
      await saveDraft(runtime, review.scope, 'Please address this concern', review.anchor)
    ).comment
    const { response } = await submitReview(
      runtime,
      review.review.id,
      'request_changes',
      'Changes requested',
    )
    expect(response.status).toBe(200)
    await github.api(
      `repos/${github.repository}/pulls/${github.number}/reviews`,
      'GET',
      undefined,
      secondary,
    )
    runtime.env.GH_TOKEN = primary
    await runtime.restart()
    const current = await prepareReview(runtime, github)
    await refreshReview(runtime, current.scope)
    const conversation = await runtime.rpc('GetConversation', { scope: current.scope })
    const comment = conversation.comments.find(
      (comment: Json) => comment.forgeCommentId === root.forgeCommentId,
    )
    const thread = conversation.threads.find((thread: Json) => thread.id === comment.threadId)
    return { ...current, comment, thread }
  }

  it(
    'User sees another author’s published thread and its author',
    { tags: ['extended'] },
    async () => {
      // Arrange
      const { runtime, scope, comment } = await publishedConcern()

      // Act
      const conversation = await runtime.rpc('GetConversation', { scope })

      // Assert
      expect(conversation.comments.find((item: Json) => item.id === comment.id)).toMatchObject({
        visibility: 'published',
        body: expect.stringContaining('Please address this concern'),
      })
      expect(comment.author).toBeTruthy()
    },
  )

  it('User cannot edit another author’s comment', async () => {
    // Arrange
    const { runtime, scope, comment } = await publishedConcern()

    // Act
    const rejected = await runtime.response('EditComment', {
      scope,
      commentId: comment.id,
      body: 'Overwrite another author',
      expectedVersion: comment.version,
      requestKey: randomUUID(),
    })

    // Assert
    expect(rejected.status).toBeGreaterThanOrEqual(400)
    expect(rejected.body.message).toMatch(/own|owner|author/i)
  })

  it(
    'User can respond to another author’s comment with a draft reply',
    { tags: ['extended'] },
    async () => {
      // Arrange
      const { runtime, github, scope, thread } = await publishedConcern()

      // Act
      const reply = await runtime.rpc('Reply', {
        scope,
        threadId: thread.id,
        body: 'Proposed response',
        expectedVersion: thread.version,
        requestKey: randomUUID(),
      })
      const remote = await readPendingComment(
        runtime,
        github,
        scope.reviewId,
        reply.comment.forgeCommentId,
      )

      // Assert
      expect(reply.syncError).toBeFalsy()
      expect(reply.comment.visibility).toBe('draft')
      expect(remote.body).toContain('Proposed response')
    },
  )

  it('User can submit responses with comments and a final message', async () => {
    // Arrange
    const { runtime, github, scope, thread, review } = await publishedConcern()
    await runtime.rpc('Reply', {
      scope,
      threadId: thread.id,
      body: 'Addressed this concern',
      expectedVersion: thread.version,
      requestKey: randomUUID(),
    })

    // Act
    const { response } = await submitReview(runtime, review.id, 'comment', 'Ready for another look')
    const remote = await github.api(
      `repos/${github.repository}/pulls/${github.number}/reviews/${response.body.review.forgeReviewId}`,
    )

    // Assert
    expect(response.status).toBe(200)
    expect(remote.state).toBe('COMMENTED')
    expect(remote.body).toContain('Ready for another look')
  })

  it('User can resolve and reopen a published thread', { tags: ['extended'] }, async () => {
    // Arrange
    const { runtime, github, scope, thread } = await publishedConcern()

    // Act
    await runtime.rpc('ResolveThread', {
      scope,
      threadId: thread.id,
      resolved: true,
      expectedVersion: thread.version,
      requestKey: randomUUID(),
    })
    const resolved = await runtime.rpc('GetConversation', { scope })
    const current = resolved.threads.find((item: Json) => item.id === thread.id)
    const remote = await github.api('graphql', 'POST', {
      query: 'query($id:ID!){node(id:$id){... on PullRequestReviewThread{isResolved}}}',
      variables: { id: current.forgeThreadId },
    })
    await runtime.rpc('ResolveThread', {
      scope,
      threadId: thread.id,
      resolved: false,
      expectedVersion: current.version,
      requestKey: randomUUID(),
    })
    const reopened = await runtime.rpc('GetConversation', { scope })

    // Assert
    expect(current.state).toBe('resolved')
    expect(remote.data.node.isResolved).toBe(true)
    expect(reopened.threads.find((item: Json) => item.id === thread.id).state).toBe('open')
  })

  it('Deleting a published root leaves its reply visible after restart without a placeholder', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor, review } = await prepareReview(runtime, github)
    const root = (await saveDraft(runtime, scope, 'Delete published root', anchor)).comment
    const conversation = await runtime.rpc('GetConversation', { scope })
    const thread = conversation.threads.find((item: Json) => item.id === root.threadId)
    const reply = (
      await runtime.rpc('Reply', {
        scope,
        threadId: thread.id,
        body: 'Keep published reply',
        expectedVersion: thread.version,
        requestKey: randomUUID(),
      })
    ).comment
    await submitReview(runtime, review.id)
    const published = await runtime.rpc('GetConversation', { scope })
    const current = published.comments.find((item: Json) => item.id === root.id)

    // Act
    await runtime.rpc('DeleteComment', {
      scope,
      commentId: root.id,
      expectedVersion: current.version,
      requestKey: randomUUID(),
    })
    await runtime.restart()
    await refreshReview(runtime, scope)
    const remaining = await runtime.rpc('GetConversation', { scope })
    const exported = await runtime.rpc('Export', { scope })
    const remote = await github.api<Json[]>(
      `repos/${github.repository}/pulls/${github.number}/comments`,
    )

    // Assert
    expect(remaining.comments.some((item: Json) => item.id === root.id)).toBe(false)
    expect(remaining.comments.find((item: Json) => item.id === reply.id).body).toBe(
      'Keep published reply',
    )
    expect(remote.some((item) => String(item.id) === root.forgeCommentId)).toBe(false)
    expect(exported.markdown).toContain('Keep published reply')
    expect(exported.markdown).not.toContain('Delete published root')
    expect(exported.markdown).not.toContain('comment deleted')
  })

  it.todo('User can respond with an immediate published reply')
})

describe('When a PR is closed or merged', () => {
  const fixture = usePrFixture('backend')

  it('Closing and reopening a PR preserves its pending feedback and review identity', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor, review, target } = await prepareReview(runtime, github)
    const root = (await saveDraft(runtime, scope, 'Retain on closed PR', anchor)).comment
    await github.api(`repos/${github.repository}/pulls/${github.number}`, 'PATCH', {
      state: 'closed',
    })

    // Act
    await refreshReview(runtime, scope)
    const closed = (await runtime.rpc('GetReview', { reviewId: review.id })).review
    await github.api(`repos/${github.repository}/pulls/${github.number}`, 'PATCH', {
      state: 'open',
    })
    const reopened = await runtime.rpc('StartReview', { target, requestKey: randomUUID() })
    const conversation = await runtime.rpc('GetConversation', { scope })

    // Assert
    expect(closed.forgeState).toBe('closed')
    expect(reopened.review.id).toBe(review.id)
    expect(conversation.comments.find((comment: Json) => comment.id === root.id).body).toBe(
      'Retain on closed PR',
    )
  })

  it('Merged PR feedback remains readable and exportable after restart', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor, review } = await prepareReview(runtime, github)
    await saveDraft(runtime, scope, 'Keep this feedback after merge', anchor)
    const { response } = await submitReview(runtime, review.id)
    expect(response.status).toBe(200)
    await github.api(`repos/${github.repository}/pulls/${github.number}/merge`, 'PUT', {
      merge_method: 'merge',
    })

    // Act
    await refreshReview(runtime, scope)
    await runtime.restart()
    const merged = (await runtime.rpc('GetReview', { reviewId: review.id })).review
    const exported = await runtime.rpc('Export', { scope })

    // Assert
    expect(merged).toMatchObject({ id: review.id, forgeState: 'merged' })
    expect(exported.markdown).toContain('Keep this feedback after merge')
  })
})

describe('When exporting PR feedback', () => {
  const fixture = usePrFixture('backend')

  it('Export identifies file, author, thread state and selected feedback without database state', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const { scope, anchor } = await prepareReview(runtime, github)
    const selected = (await saveDraft(runtime, scope, 'Selected feedback', anchor)).comment
    await saveDraft(runtime, scope, 'Unselected feedback', { kind: 'general' })

    // Act
    const exported = await runtime.rpc('Export', { scope, commentIds: [selected.id] })

    // Assert
    for (const text of ['Selected feedback', github.path, 'E2E user'])
      expect(exported.markdown).toContain(text)
    expect(exported.markdown).not.toContain('Unselected feedback')
    expect(exported.markdown).not.toContain('forge_comment_id')
    expect(exported.markdown).not.toContain('expectedVersion')
  })
})
