/**
 * @module-tag e2e
 * @module-tag backend
 * @module-tag diffs
 */
import { describe, it, expect } from 'vitest'
import { randomUUID } from 'node:crypto'
import { join } from 'node:path'
import { useTestHarness } from '../common/fixtures.js'
import type { TestHarness } from '../common/test-harness.js'
import { line } from './anchors.js'

async function prepareSource(runtime: TestHarness) {
  await runtime.identify()
  const bytes = 'before\nselected\nafter\n'
  const plans = join(runtime.workspace, '.difflab/plan/example')
  await runtime.write('.gitignore', '.difflab/\n')
  await runtime.write('.difflab/plan/example/PLAN.md', bytes)
  const target = { directory: { path: plans } }
  const diff = (await runtime.rpc('GetDiff', { target })).diff
  return { bytes, plans, target, diff }
}

describe('When reading diff metadata and source bytes', () => {
  const fixture = useTestHarness('backend')

  it(
    'Consumers can read ignored content without creating a review',
    { tags: ['smoke'] },
    async () => {
      // Arrange
      const { runtime } = fixture()
      const { bytes, diff } = await prepareSource(runtime)

      // Act
      const files = await runtime.rpc('GetFiles', { diff, pageSize: 1 })
      const loaded = await runtime.rpc('GetFile', { diff, path: 'PLAN.md' })
      const reviews = await runtime.rpc('ListReviews')

      // Assert
      expect(files.files[0].path).toBe('PLAN.md')
      expect(loaded.newSide.content).toBe(bytes)
      expect(reviews.reviews ?? []).toEqual([])
    },
  )

  it('A stale transient descriptor cannot silently return changed source bytes', async () => {
    // Arrange
    const { runtime } = fixture()
    const { bytes, diff } = await prepareSource(runtime)
    await runtime.write('.difflab/plan/example/PLAN.md', 'inserted\n' + bytes)

    // Act
    const rejected = await runtime.response('GetFile', { diff, path: 'PLAN.md' })

    // Assert
    expect(rejected.status).toBeGreaterThanOrEqual(400)
    expect(rejected.body.code).toBe('failed_precondition')
  })

  it('A review reads its saved bytes and validates comment ranges after the workspace changes', async () => {
    // Arrange
    const { runtime } = fixture()
    const { target, bytes } = await prepareSource(runtime)
    const review = (await runtime.rpc('StartReview', { target, requestKey: randomUUID() })).review
    await runtime.write('.difflab/plan/example/PLAN.md', 'replacement\n')
    const scope = { reviewId: review.id }

    // Act
    const diff = (await runtime.rpc('GetDiff', { reviewId: review.id })).diff
    const loaded = await runtime.rpc('GetFile', { diff, path: 'PLAN.md' })
    await runtime.rpc('AddComment', {
      scope,
      anchor: line(review.snapshot.id, 'PLAN.md'),
      body: 'Saved selection',
      expectedVersion: review.version,
      requestKey: randomUUID(),
    })
    const conversation = await runtime.rpc('GetConversation', {
      scope,
      snapshotId: review.snapshot.id,
    })
    const rejected = await runtime.response('AddComment', {
      scope,
      anchor: { ...line(review.snapshot.id, 'PLAN.md'), start: '50', end: '50' },
      body: 'Invalid range',
      expectedVersion: conversation.version,
      requestKey: randomUUID(),
    })

    // Assert
    expect(loaded.newSide.content).toBe(bytes)
    expect(conversation.threads[0].anchor).toMatchObject({
      snapshotId: review.snapshot.id,
      evidence: { selectedText: 'selected' },
    })
    expect(conversation.threads[0].displayLocation).toMatchObject({ path: 'PLAN.md', start: '2' })
    expect(rejected.status).toBeGreaterThanOrEqual(400)
    expect((await runtime.rpc('GetConversation', { scope })).comments).toHaveLength(1)
  })
})
