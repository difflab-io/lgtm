/**
 * @module-tag e2e
 * @module-tag cli
 * @module-tag github
 * @module-tag pr-review
 */
import { describe, it, expect } from 'vitest'
import { usePrFixture } from '../common/pr.fixtures.js'
import { githubCredentials } from '../common/github.js'

const secondaryUser = process.env.LGTM_E2E_GITHUB_SECONDARY_USER ?? 'skapoor81'

describe('When starting a fresh draft review', () => {
  const fixture = usePrFixture('cli')

  it(
    'User can select a PR and immediately persist an inline draft',
    { tags: ['smoke'] },
    async () => {
      // Arrange
      const { runtime, github } = fixture()
      const pr = `${github.repositoryUrl}/pull/${github.number}`

      // Act
      const started = await runtime.json(['session', 'start', '--pr', pr])
      const saved = await runtime.json([
        'comments',
        'add',
        '--pr',
        pr,
        '--file',
        github.path,
        '--line',
        '2',
        '--body',
        'CLI inline feedback',
      ])
      const current = (await runtime.json(['session', 'show', '--pr', pr])).review
      const remote = await github.api(
        `repos/${github.repository}/pulls/${github.number}/reviews/${current.forgeReviewId}/comments`,
      )

      // Assert
      expect(started.review.state).toBe('active')
      expect(saved.syncError).toBeFalsy()
      expect(saved.comment.visibility).toBe('draft')
      expect(remote).toEqual(
        expect.arrayContaining([
          expect.objectContaining({ body: expect.stringContaining('CLI inline feedback') }),
        ]),
      )
    },
  )

  it('User can reply to a draft comment', { tags: ['extended'] }, async () => {
    // Arrange
    const { runtime, github } = fixture()
    const pr = `${github.repositoryUrl}/pull/${github.number}`
    await runtime.json(['session', 'start', '--pr', pr])
    const root = (
      await runtime.json([
        'comments',
        'add',
        '--pr',
        pr,
        '--file',
        github.path,
        '--line',
        '2',
        '--body',
        'Root comment',
      ])
    ).comment

    // Act
    const reply = await runtime.json([
      'comments',
      'reply',
      root.threadId,
      '--pr',
      pr,
      '--body',
      'Draft reply',
    ])
    const current = await runtime.json(['comments', 'list', '--pr', pr])

    // Assert
    expect(reply.syncError).toBeFalsy()
    expect(current.comments).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          threadId: root.threadId,
          body: 'Draft reply',
          visibility: 'draft',
          forgeCommentId: expect.any(String),
        }),
      ]),
    )
  })

  it('User cannot resolve an unpublished draft thread', { tags: ['extended'] }, async () => {
    // Arrange
    const { runtime, github } = fixture()
    const pr = `${github.repositoryUrl}/pull/${github.number}`
    await runtime.json(['session', 'start', '--pr', pr])
    const root = (
      await runtime.json([
        'comments',
        'add',
        '--pr',
        pr,
        '--file',
        github.path,
        '--line',
        '2',
        '--body',
        'Unpublished comment',
      ])
    ).comment

    // Act
    const rejected = await runtime.cli(['comments', 'resolve', root.threadId, '--pr', pr, '--json'])

    // Assert
    expect(rejected.code).not.toBe(0)
    expect(JSON.parse(rejected.stdout).error.message).toMatch(/draft|unpublished/i)
    expect((await runtime.json(['comments', 'list', '--pr', pr])).threads[0].state).toBe('open')
  })

  it('User can edit and delete their draft without a placeholder in exports', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const pr = `${github.repositoryUrl}/pull/${github.number}`
    await runtime.json(['session', 'start', '--pr', pr])
    const root = (
      await runtime.json([
        'comments',
        'add',
        '--pr',
        pr,
        '--file',
        github.path,
        '--body',
        'Original file feedback',
      ])
    ).comment

    // Act
    await runtime.json(['comments', 'edit', root.id, '--pr', pr, '--body', 'Edited file feedback'])
    const edited = await runtime.json(['comments', 'list', '--pr', pr])
    await runtime.json(['comments', 'delete', root.id, '--pr', pr])
    const exported = await runtime.json(['session', 'export', '--pr', pr])

    // Assert
    expect(edited.comments[0].body).toBe('Edited file feedback')
    expect((await runtime.json(['comments', 'list', '--pr', pr])).comments ?? []).toHaveLength(0)
    expect(exported.markdown).not.toContain('Edited file feedback')
    expect(exported.markdown).not.toContain('comment deleted')
  })
})

describe('When resuming an existing draft review', () => {
  const fixture = usePrFixture('cli')

  it(
    'User sees existing drafts and can continue the same pending review after restart',
    { tags: ['extended'] },
    async () => {
      // Arrange
      const { runtime, github } = fixture()
      const pr = `${github.repositoryUrl}/pull/${github.number}`
      const started = (await runtime.json(['session', 'start', '--pr', pr])).review
      const saved = (
        await runtime.json([
          'comments',
          'add',
          '--pr',
          pr,
          '--file',
          github.path,
          '--line',
          '2',
          '--body',
          'Saved before restart',
        ])
      ).comment
      const pending = (await runtime.json(['session', 'show', '--pr', pr])).review.forgeReviewId
      await runtime.restart()

      // Act
      const resumed = (await runtime.json(['session', 'start', '--pr', pr])).review
      await runtime.json([
        'comments',
        'add',
        '--pr',
        pr,
        '--file',
        github.path,
        '--body',
        'Added after restart',
      ])
      const current = await runtime.json(['comments', 'list', '--pr', pr])

      // Assert
      expect(resumed).toMatchObject({ id: started.id, forgeReviewId: pending })
      expect(current.comments).toHaveLength(2)
      expect(current.comments).toEqual(
        expect.arrayContaining([
          expect.objectContaining({ id: saved.id, body: 'Saved before restart' }),
        ]),
      )
    },
  )
})

describe('When there are changes during a PR review', () => {
  const fixture = usePrFixture('cli')

  it('User can refresh and continue commenting after new commits without replacing the review', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const pr = `${github.repositoryUrl}/pull/${github.number}`
    const started = (await runtime.json(['session', 'start', '--pr', pr])).review
    const root = (
      await runtime.json([
        'comments',
        'add',
        '--pr',
        pr,
        '--file',
        github.path,
        '--line',
        '2',
        '--body',
        'Preserve this draft',
      ])
    ).comment
    const pending = (await runtime.json(['session', 'show', '--pr', pr])).review.forgeReviewId
    await github.changeFiles({ [github.path]: 'new context\nfirst\nchanged\n' })

    // Act
    const refreshed = await runtime.json(['session', 'refresh', '--pr', pr])
    await runtime.json([
      'comments',
      'add',
      '--pr',
      pr,
      '--file',
      github.path,
      '--body',
      'Continue after push',
    ])
    const current = await runtime.json(['comments', 'list', '--pr', pr])

    // Assert
    expect(refreshed.review).toMatchObject({
      id: started.id,
      forgeReviewId: pending,
      comparison: { pullRequest: { headCommit: github.head } },
    })
    expect(current.comments).toHaveLength(2)
    expect(current.comments).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: root.id, body: 'Preserve this draft' }),
        expect.objectContaining({
          body: 'Continue after push',
          forgeCommentId: expect.any(String),
        }),
      ]),
    )
  })
})

describe('When submitting a draft review', () => {
  const fixture = usePrFixture('cli')

  it('Authentication failure returns a JSON error, retains drafts and allows recovery', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const pr = `${github.repositoryUrl}/pull/${github.number}`
    await runtime.json(['session', 'start', '--pr', pr])
    const root = (
      await runtime.json([
        'comments',
        'add',
        '--pr',
        pr,
        '--file',
        github.path,
        '--body',
        'Keep through authentication failure',
      ])
    ).comment
    const token = runtime.env.GH_TOKEN
    runtime.env.GH_TOKEN = 'invalid-e2e-token'
    await runtime.restart()

    // Act
    const rejected = await runtime.cli([
      'session',
      'submit',
      '--pr',
      pr,
      '--outcome',
      'comment',
      '--body',
      'Final text',
      '--json',
    ])
    runtime.env.GH_TOKEN = token
    await runtime.restart()
    const retained = await runtime.json(['comments', 'list', '--pr', pr])
    const corrected = await runtime.json([
      'session',
      'submit',
      '--pr',
      pr,
      '--outcome',
      'comment',
      '--body',
      'Final text',
    ])

    // Assert
    expect(rejected.code).not.toBe(0)
    expect(JSON.parse(rejected.stdout).error.message).toMatch(/auth|credential|401/i)
    expect(retained.comments.find((comment: { id: string }) => comment.id === root.id).body).toBe(
      'Keep through authentication failure',
    )
    expect(corrected.review.submissionStatus).toBe('confirmed')
  })

  it('User can submit comments, export final feedback and continue with a new draft', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const pr = `${github.repositoryUrl}/pull/${github.number}`
    await runtime.json(['session', 'start', '--pr', pr])
    await runtime.json(['comments', 'add', '--pr', pr, '--body', 'General CLI feedback'])

    // Act
    const submitted = await runtime.json([
      'session',
      'submit',
      '--pr',
      pr,
      '--outcome',
      'comment',
      '--body',
      'Final CLI message',
    ])
    const remote = await github.api(
      `repos/${github.repository}/pulls/${github.number}/reviews/${submitted.review.forgeReviewId}`,
    )
    const exported = await runtime.json(['session', 'export', '--pr', pr])

    // Assert
    expect(submitted.review.submissionStatus).toBe('confirmed')
    expect(remote).toMatchObject({
      state: 'COMMENTED',
      body: expect.stringContaining('Final CLI message'),
    })
    for (const text of ['General CLI feedback', 'Final CLI message'])
      expect(exported.markdown).toContain(text)

    // Act
    const saved = await runtime.json([
      'comments',
      'add',
      '--pr',
      pr,
      '--file',
      github.path,
      '--line',
      '2',
      '--body',
      'CLI feedback after publishing',
    ])
    const continued = await runtime.json(['session', 'show', '--pr', pr])
    const previous = await runtime.json([
      'session',
      'show',
      '--pr',
      pr,
      '--session',
      submitted.review.id,
    ])
    const pending = await github.api(
      `repos/${github.repository}/pulls/${github.number}/reviews/${continued.review.forgeReviewId}`,
    )

    // Assert
    expect(continued.review.id).not.toBe(submitted.review.id)
    expect(continued.review.state).toBe('active')
    expect(saved.comment).toMatchObject({
      body: 'CLI feedback after publishing',
      visibility: 'draft',
      originSessionId: continued.review.id,
    })
    expect(saved.comment.forgeCommentId).toBeTruthy()
    expect(pending.state).toBe('PENDING')
    expect(previous.review).toMatchObject({
      state: 'completed',
      submissionStatus: 'confirmed',
      body: 'Final CLI message',
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
      const pr = `${github.repositoryUrl}/pull/${github.number}`
      await runtime.json(['session', 'start', '--pr', pr])

      // Act
      const submitted = await runtime.json([
        'session',
        'submit',
        '--pr',
        pr,
        '--outcome',
        outcome,
        '--body',
        'Another reviewer’s assessment',
      ])
      const remote = await github.api(
        `repos/${github.repository}/pulls/${github.number}/reviews/${submitted.review.forgeReviewId}`,
        'GET',
        undefined,
        secondary,
      )

      // Assert
      expect(submitted.review.submissionStatus).toBe('confirmed')
      expect(remote).toMatchObject({
        state: outcome === 'approve' ? 'APPROVED' : 'CHANGES_REQUESTED',
        user: { login: secondaryUser },
      })
    },
  )

  it.each(['approve', 'request_changes'])(
    'User cannot submit %s on their own PR and can correct the outcome',
    async (outcome) => {
      // Arrange
      const { runtime, github } = fixture()
      const pr = `${github.repositoryUrl}/pull/${github.number}`
      await runtime.json(['session', 'start', '--pr', pr])
      const draft = (
        await runtime.json([
          'comments',
          'add',
          '--pr',
          pr,
          '--file',
          github.path,
          '--body',
          'Retain this draft',
        ])
      ).comment

      // Act
      const rejected = await runtime.cli([
        'session',
        'submit',
        '--pr',
        pr,
        '--outcome',
        outcome,
        '--body',
        'Retain this final text',
        '--json',
      ])
      const retained = await runtime.json(['comments', 'list', '--pr', pr])
      const corrected = await runtime.json([
        'session',
        'submit',
        '--pr',
        pr,
        '--outcome',
        'comment',
        '--body',
        'Corrected final text',
      ])

      // Assert
      expect(rejected.code).not.toBe(0)
      expect(JSON.parse(rejected.stdout).error.message).toContain('own PR')
      expect(
        retained.comments.find((comment: { id: string }) => comment.id === draft.id).visibility,
      ).toBe('draft')
      expect(corrected.review.submissionStatus).toBe('confirmed')
    },
  )
})

describe('When addressing published comments or change requests', () => {
  const fixture = usePrFixture('cli')

  it('User can reply to another reviewer with a draft and submit a comment response', async () => {
    // Arrange
    const { runtime, github } = fixture()
    const secondary = await githubCredentials(secondaryUser)
    await github.api(
      `repos/${github.repository}/pulls/${github.number}/reviews`,
      'POST',
      {
        event: 'REQUEST_CHANGES',
        body: 'Please address this concern',
        comments: [{ path: github.path, line: 2, side: 'RIGHT', body: 'Published concern' }],
      },
      secondary,
    )
    const pr = `${github.repositoryUrl}/pull/${github.number}`
    await runtime.json(['session', 'start', '--pr', pr])
    const conversation = await runtime.json(['comments', 'list', '--pr', pr])
    const root = conversation.comments.find((comment: { body: string }) =>
      comment.body.includes('Published concern'),
    )

    // Act
    const reply = await runtime.json([
      'comments',
      'reply',
      root.threadId,
      '--pr',
      pr,
      '--body',
      'Addressed this concern',
    ])
    const pending = (await runtime.json(['session', 'show', '--pr', pr])).review
    const drafts = await github.api<Array<{ body: string }>>(
      `repos/${github.repository}/pulls/${github.number}/reviews/${pending.forgeReviewId}/comments`,
    )
    const submitted = await runtime.json([
      'session',
      'submit',
      '--pr',
      pr,
      '--outcome',
      'comment',
      '--body',
      'Ready for another look',
    ])
    const remote = await github.api(
      `repos/${github.repository}/pulls/${github.number}/reviews/${submitted.review.forgeReviewId}`,
    )

    // Assert
    expect(reply.comment.visibility).toBe('draft')
    expect(drafts.some((comment) => comment.body.includes('Addressed this concern'))).toBe(true)
    expect(remote.state).toBe('COMMENTED')
    expect(remote.body).toContain('Ready for another look')
  })
})
