/**
 * @module-tag e2e
 * @module-tag forge
 */
import { describe, it, expect } from 'vitest'
import { useTestHarness } from '../common/fixtures.js'

describe(
  'Given a local Git repository / When calling ReviewService RPCs on the forge backend',
  { tags: ['e2e', 'forge', 'review-service'] },
  () => {
    const fixture = useTestHarness('forge')

    it(
      'should create a local review, persist comments, and return inline diff hunks',
      { tags: ['smoke'] },
      async () => {
        // Arrange
        const { runtime } = fixture()
        await runtime.git('init', '-q', '-b', 'feature/forge-rpc')
        await runtime.git('config', 'user.name', 'Ada Lovelace')
        await runtime.git('config', 'user.email', 'ada@example.test')
        await runtime.write('src/lib.rs', 'pub fn alpha() -> i32 {\n    1\n}\n')
        await runtime.commit('Initial commit')
        await runtime.write('src/lib.rs', 'pub fn alpha() -> i32 {\n    10\n}\n')

        // Act
        const added = await runtime.rpc('Add', {
          gitDir: runtime.workspace,
          paths: ['src/lib.rs'],
          local: true,
        })
        const committed = await runtime.rpc('Commit', {
          gitDir: runtime.workspace,
          message: 'Snapshot update',
          local: true,
        })
        const created = await runtime.rpc('CreateReview', {
          gitDir: runtime.workspace,
          local: true,
          asAgent: 'claude-sonnet-4-5',
        })
        const comment = await runtime.rpc('CreateComment', {
          gitDir: runtime.workspace,
          body: 'Use a named constant for the return value in alpha().',
          filePath: 'src/lib.rs',
          line: 2,
          local: true,
          asAgent: 'claude-sonnet-4-5',
        })
        const closed = await runtime.rpc('CloseReview', {
          gitDir: runtime.workspace,
          local: true,
          withOutcome: 'REVIEW_OUTCOME_APPROVE',
          body: 'Completed initial review pass.',
          asAgent: 'claude-sonnet-4-5',
        })
        const reply = await runtime.rpc('CreateComment', {
          gitDir: runtime.workspace,
          body: 'Extracted the constant in the follow-up commit.',
          replyTo: comment.comment.number,
          local: true,
        })
        const detail = await runtime.rpc('ShowReview', {
          gitDir: runtime.workspace,
          local: true,
          groupBy: 'REVIEW_GROUP_BY_FILE',
        })
        const listed = await runtime.rpc('ListReviews', {
          gitDir: runtime.workspace,
          local: true,
          all: true,
        })

        // Assert
        expect(added.stagedPaths).toEqual(['src/lib.rs'])
        expect(committed.local).toBe(true)
        expect(created.review.name).toBe('R1-feature-forge-rpc')
        expect(closed.session.state).toBe('SESSION_STATE_PUBLISHED')
        expect(reply.comment.replyToNumber).toBe(comment.comment.number)
        expect(detail.comments).toHaveLength(2)
        expect(detail.diffHunks).toHaveLength(1)
        expect(listed.reviews).toHaveLength(1)
      },
    )

    it('should reject removed legacy RPC methods', async () => {
      // Arrange
      const { runtime } = fixture()

      // Act
      const helloReview = await runtime.response('HelloReview', { name: 'Legacy' })
      const startReview = await runtime.response('StartReview', {})

      // Assert
      expect(helloReview.status).not.toBe(200)
      expect(startReview.status).not.toBe(200)
    })
  },
)
