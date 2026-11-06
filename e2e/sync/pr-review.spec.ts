import { describe, it, expect } from 'vitest'
import { randomUUID } from 'node:crypto'
import { join } from 'node:path'
import { GithubFixture } from '../common/github.js'
import { ROOT } from '../common/test-harness.js'
import { waitFor } from '../common/test-utils.js'
import { useSyncFixture, signIn } from './sync.fixtures.js'

// @module-tag e2e sync pr-review

describe('When synchronizing forge-backed reviews', { tags: ['e2e', 'sync', 'pr-review'] }, () => {
  const fixture = useSyncFixture()

  it('preserves PR identity and commit comparisons across devices using recorded GitHub reads', async () => {
    // Arrange
    const { a, b, cloud } = fixture()
    const github = await GithubFixture.replay(
      join(ROOT, 'e2e/recordings/backend/pr-review.json'),
      'user-can-start-a-review-for-the-selected-pr-ea86f794',
    )
    try {
      Object.assign(a.env, github.env)
      await a.restart()
      await a.identify()
      const target = {
        pullRequest: { repositoryUrl: github.repositoryUrl, number: String(github.number) },
      }
      const review = (await a.rpc('StartReview', { target, requestKey: randomUUID() })).review

      // Act
      await signIn(a, cloud.endpoint)
      const uploaded = await waitFor(
        'uploaded PR',
        () => cloud.pull(),
        (value) =>
          value.changes.some(
            (change: { snapshot: { sessions: unknown[] } }) =>
              change.snapshot.sessions.length === 1,
          ),
      )
      b.actor = a.actor
      await signIn(b, cloud.endpoint)
      const result = await waitFor(
        'downloaded PR',
        () => b.response('GetReview', { reviewId: review.id }),
        (value) => value.status === 200,
      )

      // Assert
      expect(result.body.review.target).toEqual(target)
      expect(result.body.review.comparison).toEqual(review.comparison)
      expect(uploaded.changes[0].snapshot.context.repository_url).toBe(github.repositoryUrl)
      expect(uploaded.changes[0].snapshot.context.git_dir).toBeNull()
      const pr = await github.api(`repos/${github.repository}/pulls/${github.number}`)
      await github.reset(pr)
      await github.finish(true)
    } catch (error) {
      await github.finish(false)
      throw error
    }
  })
})
