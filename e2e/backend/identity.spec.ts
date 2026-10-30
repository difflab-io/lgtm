/**
 * @module-tag e2e
 * @module-tag backend
 */
import { describe, it, expect } from 'vitest'
import { useTestHarness } from '../common/fixtures.js'

describe('When discovering the review API', () => {
  const fixture = useTestHarness('backend')

  it('Consumers can discover the supported review contracts without creating reviews', async () => {
    // Arrange
    const { runtime } = fixture()

    // Act
    const capabilities = await runtime.rpc('GetCapabilities')
    const reviews = await runtime.rpc('ListReviews')

    // Assert
    expect(capabilities.capabilities).toEqual(
      expect.arrayContaining([
        'review_targets',
        'feedback_scopes',
        'review_snapshots',
        'diff_service',
        'rejected_submission_recovery',
      ]),
    )
    expect(reviews.reviews ?? []).toEqual([])
  })

  it('Retired workspace and session RPCs are unavailable', async () => {
    // Arrange
    const { runtime } = fixture()

    // Act
    const results = await Promise.all(
      [
        'EnsureContext',
        'StartSession',
        'GetSession',
        'ListSessions',
        'CompleteSession',
        'RefreshContext',
        'GetWorkspace',
      ].map((method) => runtime.response(method, {})),
    )

    // Assert
    for (const result of results) expect(result.body.code).toBe('unimplemented')
  })
})
