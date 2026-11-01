/**
 * @module-tag e2e
 * @module-tag cli
 * @module-tag github
 * @module-tag local-review
 * @module-tag pr-review
 */
import { describe, it, expect } from 'vitest'
import { usePrFixture } from '../common/pr.fixtures.js'
import type { Json } from '../common/test-utils.js'
import { setupPrCheckout } from './pr-review.fixtures.js'

function normalizeDiffAndThreadsOutput(stdout: string): string {
  return stdout
    .trim()
    .split('\n')
    .slice(2)
    .join('\n')
    .replace(/\[\d+\]/g, '[#]')
    .replace(/reply to #\d+/g, 'reply to #N')
    .replace(/session #\d+/gi, (match) =>
      match.startsWith('Session') ? 'Session #S' : 'session #S',
    )
}

describe(
  'When showing and closing a remote PR review with local workflow parity',
  { tags: ['e2e', 'cli', 'github', 'local-review', 'pr-review'] },
  () => {
    const fixture = usePrFixture('cli')

    it('should_produce_identical_diff_hunk_and_thread_output_between_local_and_remote_reviews_and_submit_session_to_github', async () => {
      // Arrange
      const { runtime, github } = fixture()
      const { prNumber, filePath } = await setupPrCheckout(runtime, github)

      // Create both a local snapshot review (--local) and a remote PR review on the exact same checkout
      await runtime.json(['review', 'new', '--local'])
      await runtime.json(['review', 'new'])

      for (const localArgs of [['--local'], []] as const) {
        const root = await runtime.json([
          'comment',
          'new',
          'Verify changed line 2 parity',
          '--file',
          filePath,
          '--line',
          '2',
          ...localArgs,
        ])
        const rootNum = (root.comment as { number: number }).number
        await runtime.json([
          'comment',
          'new',
          'Follow-up reply in thread',
          '--reply-to',
          String(rootNum),
          '--as',
          'claude-sonnet-4-5',
          ...localArgs,
        ])
        await runtime.json([
          'comment',
          'update',
          String(rootNum),
          '--resolved',
          'true',
          ...localArgs,
        ])
        await runtime.json([
          'comment',
          'new',
          'Unresolved range comment on lines 3-5',
          '--file',
          filePath,
          '--start-line',
          '3',
          '--line',
          '5',
          '--as',
          'claude-sonnet-4-5',
          ...localArgs,
        ])
      }

      // Act
      const localShowJson = await runtime.json(['review', 'show', '--local', '--group-by', 'file'])
      const remoteShowJson = await runtime.json(['review', 'show', '--group-by', 'file'])

      const localFileText = await runtime.cli(['review', 'show', '--local', '--group-by', 'file'])
      const remoteFileText = await runtime.cli(['review', 'show', '--group-by', 'file'])

      const localSessionText = await runtime.cli([
        'review',
        'show',
        '--local',
        '--group-by',
        'session',
      ])
      const remoteSessionText = await runtime.cli(['review', 'show', '--group-by', 'session'])

      const localUnresolvedText = await runtime.cli(['review', 'show', '--local', '--unresolved'])
      const remoteUnresolvedText = await runtime.cli(['review', 'show', '--unresolved'])

      const localClosed = await runtime.json([
        'review',
        'close',
        '--local',
        '--with',
        'comment',
        '--body',
        'Completed parity review pass',
        '--as',
        'claude-sonnet-4-5',
      ])
      const remoteClosed = await runtime.json([
        'review',
        'close',
        '--with',
        'comment',
        '--body',
        'Completed parity review pass',
        '--as',
        'claude-sonnet-4-5',
      ])
      const githubReviews = await github.api<Json[]>(
        `repos/${github.repository}/pulls/${prNumber}/reviews?per_page=100`,
      )

      // Assert: JSON diff_hunks and comment anchors match identically between local and remote reviews
      expect(remoteShowJson.diff_hunks).toEqual(localShowJson.diff_hunks)
      expect((remoteShowJson.diff_hunks as unknown[]).length).toBe(2)

      const normalizeComments = (comments: Json[]) =>
        comments.map((c) => ({
          body: c.body,
          resolved: c.resolved,
          outdated: c.outdated,
          isReply: c.reply_to_number !== null,
          authorKind: c.author.kind,
          authorName: c.author.name,
          anchor: c.anchor
            ? {
                file_path: c.anchor.file_path,
                start_line: c.anchor.start_line,
                line: c.anchor.line,
                original_start_line: c.anchor.original_start_line,
                original_line: c.anchor.original_line,
              }
            : null,
        }))
      expect(normalizeComments(remoteShowJson.comments as Json[])).toEqual(
        normalizeComments(localShowJson.comments as Json[]),
      )

      // Assert: terminal diff & thread output parity across --group-by file, --group-by session, and --unresolved
      expect(localFileText.code).toBe(0)
      expect(remoteFileText.code).toBe(0)
      expect(normalizeDiffAndThreadsOutput(remoteFileText.stdout)).toBe(
        normalizeDiffAndThreadsOutput(localFileText.stdout),
      )

      expect(localSessionText.code).toBe(0)
      expect(remoteSessionText.code).toBe(0)
      expect(normalizeDiffAndThreadsOutput(remoteSessionText.stdout)).toBe(
        normalizeDiffAndThreadsOutput(localSessionText.stdout),
      )

      expect(localUnresolvedText.code).toBe(0)
      expect(remoteUnresolvedText.code).toBe(0)
      expect(normalizeDiffAndThreadsOutput(remoteUnresolvedText.stdout)).toBe(
        normalizeDiffAndThreadsOutput(localUnresolvedText.stdout),
      )

      // Assert: both local and remote sessions transition to published, and GitHub records the submitted review
      expect(localClosed.session).toMatchObject({
        state: 'published',
        outcome: 'comment',
        body: 'Completed parity review pass',
      })
      expect(remoteClosed.session).toMatchObject({
        state: 'published',
        outcome: 'comment',
        body: 'Completed parity review pass',
      })
      const submittedOnGithub = githubReviews.find(
        (r) => r.state === 'COMMENTED' && String(r.body).includes('Completed parity review pass'),
      )
      expect(submittedOnGithub).toBeDefined()
      expect(String(submittedOnGithub!.body)).toContain(
        '> Generated by AI agent `anthropic/claude-sonnet-4-5` via LGTM',
      )
    })
  },
)
