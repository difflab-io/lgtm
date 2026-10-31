/**
 * @module-tag e2e
 * @module-tag cli
 * @module-tag github
 * @module-tag pr-review
 */
import { describe, it, expect } from 'vitest'
import { usePrFixture } from '../common/pr.fixtures.js'
import type { Json } from '../common/test-utils.js'
import { normalizeDiffAndThreadsOutput, setupPrCheckout } from './pr-review.fixtures.js'

describe(
  'When committing, pushing, and syncing a remote GitHub pull request review',
  { tags: ['e2e', 'cli', 'github', 'pr-review'] },
  () => {
    const fixture = usePrFixture('cli')

    it(
      'should_commit_as_agent_push_branch_and_create_or_refresh_remote_pr_review',
      { tags: ['smoke'] },
      async () => {
        // Arrange
        const { runtime, github } = fixture()
        const { prNumber, filePath, initialHead } = await setupPrCheckout(runtime, github)
        const updatedContent = github.headContent.replace(
          'stable context 1',
          'stable context 1 (updated by CLI push)',
        )
        await runtime.write(filePath, updatedContent)

        // Act
        const added = await runtime.json(['add', filePath])
        const committed = await runtime.json([
          'commit',
          '--as',
          'anthropic/claude-sonnet-4-5',
          '-m',
          'Update PR branch via lgtm CLI',
        ])
        const pushed = await runtime.json(['push'])
        const created = await runtime.json(['review', 'new'])
        const listed = await runtime.json(['review', 'list'])

        // Assert
        expect(added).toEqual({
          staged_paths: [filePath],
          local: false,
        })
        expect(committed.local).toBe(false)
        expect(committed.commit_sha).not.toBe(initialHead)
        expect(committed.author).toMatchObject({
          name: 'claude-sonnet-4-5',
          kind: 'agent',
          provider: 'anthropic',
          model: 'claude-sonnet-4-5',
        })
        expect(pushed).toMatchObject({
          local: false,
          head_sha: committed.commit_sha,
          repository_url: github.repositoryUrl,
        })
        expect(created.review).toMatchObject({
          name: `PR${prNumber}-${pushed.branch}`,
          local: false,
          pr_number: prNumber,
          repository_url: github.repositoryUrl,
          head_sha: committed.commit_sha,
          status: 'open',
        })
        expect((listed.reviews as Array<{ name: string }>).map((r) => r.name)).toContain(
          `PR${prNumber}-${pushed.branch}`,
        )
      },
    )
  },
)

describe(
  'When drafting, replying to, resolving, and deleting remote PR comments',
  { tags: ['e2e', 'cli', 'github', 'pr-review'] },
  () => {
    const fixture = usePrFixture('cli')

    it('should_sync_draft_comments_and_agent_disclosure_footers_to_github_in_near_real_time', async () => {
      // Arrange
      const { runtime, github } = fixture()
      const { prNumber, filePath } = await setupPrCheckout(runtime, github)
      await runtime.json(['review', 'new'])

      // Act
      const humanComment = await runtime.json([
        'comment',
        'new',
        'Verify changed line behavior',
        '--file',
        filePath,
        '--line',
        '2',
      ])
      const agentComment = await runtime.json([
        'comment',
        'new',
        'Check context range invariants',
        '--file',
        filePath,
        '--start-line',
        '3',
        '--line',
        '5',
        '--as',
        'anthropic/claude-sonnet-4-5',
      ])
      const humanNum = (humanComment.comment as { number: number }).number
      const agentNum = (agentComment.comment as { number: number }).number

      const updatedHuman = await runtime.json([
        'comment',
        'update',
        String(humanNum),
        '--body',
        'Verify changed line behavior (updated)',
        '--resolved',
        'true',
      ])

      const remoteCommentsBeforeDelete = await github.api<Json[]>(
        `repos/${github.repository}/pulls/${prNumber}/comments?per_page=100`,
      )
      const deleted = await runtime.json([
        'comment',
        'delete',
        String(agentNum),
        '--as',
        'anthropic/claude-sonnet-4-5',
      ])
      const remoteCommentsAfterDelete = await github.api<Json[]>(
        `repos/${github.repository}/pulls/${prNumber}/comments?per_page=100`,
      )

      // Assert
      expect(humanComment.comment).toMatchObject({
        number: humanNum,
        body: 'Verify changed line behavior',
        author: { kind: 'human' },
      })
      expect((humanComment.comment as { forge_id: string | null }).forge_id).toBeTruthy()
      expect(agentComment.comment).toMatchObject({
        number: agentNum,
        body: 'Check context range invariants',
        author: {
          name: 'claude-sonnet-4-5',
          kind: 'agent',
          provider: 'anthropic',
          model: 'claude-sonnet-4-5',
        },
      })
      expect(updatedHuman.comment).toMatchObject({
        number: humanNum,
        body: 'Verify changed line behavior (updated)',
        resolved: true,
      })

      const remoteAgentComment = remoteCommentsBeforeDelete.find((c) =>
        String(c.body).includes('Check context range invariants'),
      )
      expect(remoteAgentComment).toBeDefined()
      expect(String(remoteAgentComment!.body)).toContain(
        '> Generated by AI agent `anthropic/claude-sonnet-4-5` via LGTM',
      )

      const remoteHumanComment = remoteCommentsBeforeDelete.find((c) =>
        String(c.body).includes('Verify changed line behavior (updated)'),
      )
      expect(remoteHumanComment).toBeDefined()

      expect(deleted).toEqual({ deleted_number: agentNum })
      expect(
        remoteCommentsAfterDelete.some((c) =>
          String(c.body).includes('Check context range invariants'),
        ),
      ).toBe(false)
    })
  },
)

describe(
  'When refreshing a remote PR review after mid-flight commits',
  { tags: ['e2e', 'cli', 'github', 'pr-review'] },
  () => {
    const fixture = usePrFixture('cli')

    it('should_pull_updated_pr_head_and_remap_comment_anchors_on_refresh', async () => {
      // Arrange
      const { runtime, github } = fixture()
      const { prNumber, filePath, initialHead } = await setupPrCheckout(runtime, github)
      await runtime.json(['review', 'new'])
      const anchored = await runtime.json([
        'comment',
        'new',
        'Anchor on line 6 that should shift down by 2 lines after remote commit',
        '--file',
        filePath,
        '--line',
        '6',
        '--as',
        'claude-sonnet-4-5',
      ])
      const commentNum = (anchored.comment as { number: number }).number

      // Act: push a mid-flight commit to the PR on GitHub that inserts 2 lines at the top of the file
      await github.changeFiles({
        [filePath]: `inserted header 1\ninserted header 2\n${github.headContent}`,
      })
      await github.fetch(runtime, github.head)
      const refreshed = await runtime.json(['review', 'refresh', `PR${prNumber}`])
      const shown = await runtime.json(['review', 'show', `PR${prNumber}`])

      // Assert
      expect((refreshed.review as { head_sha: string }).head_sha).toBe(github.head)
      expect(refreshed.updated_anchors).toBeGreaterThanOrEqual(1)

      const remapped = (
        shown.comments as Array<{
          number: number
          outdated: boolean
          author: { kind: string; name: string }
          anchor: {
            line: number
            commit_sha: string
            original_line: number
            original_commit_sha: string
          }
        }>
      ).find((c) => c.number === commentNum)!
      expect(remapped).toBeDefined()
      expect(remapped.outdated).toBe(false)
      expect(remapped.author).toMatchObject({
        kind: 'agent',
        name: 'claude-sonnet-4-5',
      })
      expect(remapped.anchor.line).toBe(8)
      expect(remapped.anchor.commit_sha).toBe(github.head)
      expect(remapped.anchor.original_line).toBe(6)
      expect(remapped.anchor.original_commit_sha).toBe(initialHead)
    })
  },
)

describe(
  'When showing and closing a remote PR review with local workflow parity',
  { tags: ['e2e', 'cli', 'github', 'pr-review'] },
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
