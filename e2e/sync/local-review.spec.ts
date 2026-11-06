import { describe, it, expect } from 'vitest'
import { randomUUID } from 'node:crypto'
import { waitFor, type Json } from '../common/test-utils.js'
import { useSyncFixture, signIn, waitForComment } from './sync.fixtures.js'

// @module-tag e2e sync local-review

describe('When synchronizing local reviews', { tags: ['e2e', 'sync', 'local-review'] }, () => {
  const fixture = useSyncFixture()

  it.each(['working', 'staged', 'branch', 'directory'])(
    'replicates %s review sessions, discussion and completion',
    async (kind) => {
      // Arrange
      const { a, b, cloud } = fixture()
      await a.identify()
      await a.git('init', '-q', '-b', 'main')
      await a.write('main.rs', 'before\n')
      const base = await a.commit('base')
      await a.git('checkout', '-qb', 'feature')
      await a.write('main.rs', 'after\n')
      let comparison: Json = { [kind]: { baseCommit: base } }
      if (kind === 'staged') await a.git('add', '.')
      if (kind === 'branch')
        comparison = { branch: { baseCommit: base, headCommit: await a.commit('change') } }
      const target =
        kind === 'directory'
          ? { directory: { path: a.workspace } }
          : { localGit: { path: a.workspace } }
      const review = (
        await a.rpc('StartReview', {
          target,
          comparison: kind === 'directory' ? undefined : comparison,
          requestKey: randomUUID(),
        })
      ).review
      const scope = { reviewId: review.id }
      const comment = (
        await a.rpc('AddComment', {
          scope,
          body: `${kind} feedback`,
          anchor: { kind: 'general' },
          expectedVersion: review.version,
          requestKey: randomUUID(),
        })
      ).comment
      await a.rpc('CompleteReview', {
        reviewId: review.id,
        body: 'Ready to address',
        expectedVersion: (await a.rpc('GetConversation', { scope })).version,
        requestKey: randomUUID(),
      })

      // Act
      await signIn(a, cloud.endpoint)
      await waitFor(
        'uploaded review',
        () => cloud.pull(),
        (value) => value.changes.length === 1,
      )
      b.actor = a.actor
      await signIn(b, cloud.endpoint)
      const conversation = await waitForComment(b, scope, `${kind} feedback`)
      expect(conversation.comments.some((row: Json) => row.id === comment.id)).toBe(true)

      // Assert
      const downloaded = (await b.rpc('GetReview', { reviewId: review.id })).review
      expect(downloaded.state).toBe('completed')
      expect(downloaded.comparison).toHaveProperty(kind)
      expect((await b.rpc('Export', { scope })).markdown).toContain(`${kind} feedback`)
      expect(JSON.stringify(await cloud.pull())).not.toContain(a.workspace)
    },
  )
})
