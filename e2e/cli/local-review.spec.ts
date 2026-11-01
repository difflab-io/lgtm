/**
 * @module-tag e2e
 * @module-tag cli
 * @module-tag local-review
 */
import { describe, it, expect } from 'vitest'
import { readdir } from 'node:fs/promises'
import { join } from 'node:path'
import { useTestHarness } from '../common/fixtures.js'
import { EDITED_LIB_RS, captureCheckoutState, initLocalRepo } from './local-review.fixtures.js'

describe(
  'When staging and committing in local snapshot mode',
  { tags: ['e2e', 'cli', 'local-review'] },
  () => {
    const fixture = useTestHarness('cli')

    it(
      'should_stage_and_shadow_commit_worktree_changes_without_modifying_checkout_or_origin',
      { tags: ['smoke'] },
      async () => {
        // Arrange
        const { runtime } = fixture()
        await initLocalRepo(runtime)
        await runtime.write('src/lib.rs', EDITED_LIB_RS)
        const before = await captureCheckoutState(runtime)

        // Act
        const added = await runtime.json(['add', 'src/lib.rs', '--local'])
        const committed = await runtime.json(['commit', '--local', '-m', 'Local shadow snapshot'])
        const after = await captureCheckoutState(runtime)
        const snapshotDirs = await readdir(join(runtime.home, 'snapshots'))

        // Assert
        expect(added).toEqual({
          staged_paths: ['src/lib.rs'],
          local: true,
        })
        expect(committed.local).toBe(true)
        expect(committed.branch).toBe('feature/local-review')
        expect(committed.commit_sha).not.toBe(before.head)
        expect(committed.author).toMatchObject({
          name: 'Ada Lovelace',
          kind: 'human',
          email: 'ada@example.test',
        })
        expect(after.head).toBe(before.head)
        expect(after.status).toBe(before.status)
        expect(after.index).toEqual(before.index)
        expect(after.originRefs).toBe(before.originRefs)
        expect(snapshotDirs.length).toBe(1)
      },
    )

    it('should_attribute_local_shadow_commits_to_an_agent_with_as', async () => {
      // Arrange
      const { runtime } = fixture()
      await initLocalRepo(runtime)
      await runtime.write('src/lib.rs', EDITED_LIB_RS)

      // Act
      await runtime.json(['add', 'src/lib.rs', '--local'])
      const committed = await runtime.json([
        'commit',
        '--local',
        '--as',
        'anthropic/claude-sonnet-4-5',
        '-m',
        'Agent local snapshot',
      ])
      const textOut = await runtime.cli([
        'commit',
        '--local',
        '--as',
        'claude-sonnet-4-5',
        '-m',
        'Second agent snapshot',
      ])

      // Assert
      expect(committed.local).toBe(true)
      expect(committed.author).toMatchObject({
        name: 'claude-sonnet-4-5',
        kind: 'agent',
        provider: 'anthropic',
        model: 'claude-sonnet-4-5',
      })
      expect(textOut.code).toBe(0)
      expect(textOut.stdout).toContain('(local snapshot) by claude-sonnet-4-5 (agent)')
    })

    it('should_reject_push_in_local_mode_without_contacting_origin', async () => {
      // Arrange
      const { runtime } = fixture()
      await initLocalRepo(runtime)
      const before = await captureCheckoutState(runtime)

      // Act
      const pushOut = await runtime.cli(['push', '--local', '--json'])
      const after = await captureCheckoutState(runtime)

      // Assert
      expect(pushOut.code).not.toBe(0)
      expect(JSON.parse(pushOut.stdout).error.code).toBe('invalid_argument')
      expect(after.originRefs).toBe(before.originRefs)
    })
  },
)

describe(
  'When creating, listing, and refreshing local snapshot reviews',
  { tags: ['e2e', 'cli', 'local-review'] },
  () => {
    const fixture = useTestHarness('cli')

    it('should_create_and_list_local_reviews_for_the_current_branch_and_all_branches', async () => {
      // Arrange
      const { runtime } = fixture()
      const { baseSha } = await initLocalRepo(runtime, 'feature/alpha')
      await runtime.write('src/lib.rs', EDITED_LIB_RS)

      // Act
      const alphaReview = await runtime.json(['review', 'new', '--local'])
      await runtime.git('checkout', '-q', '-b', 'feature/beta')
      await runtime.write('README.md', '# Beta branch edit\n')
      const betaReview = await runtime.json(['review', 'new', '--local'])
      const betaBranchList = await runtime.json(['review', 'list', '--local'])
      const allBranchesList = await runtime.json(['review', 'list', '--local', '--all'])
      const betaBranchText = await runtime.cli(['review', 'list', '--local'])
      const allBranchesText = await runtime.cli(['review', 'list', '--local', '--all'])
      const betaShowText = await runtime.cli(['review', 'show', '--local'])

      // Assert
      expect(betaBranchText.code).toBe(0)
      expect(allBranchesText.code).toBe(0)
      expect(betaShowText.code).toBe(0)
      expect(alphaReview.review).toMatchObject({
        number: 1,
        name: 'R1-feature-alpha',
        local: true,
        branch: 'feature/alpha',
        status: 'open',
        base_sha: baseSha,
      })
      expect(alphaReview.session).toMatchObject({
        number: 1,
        state: 'draft',
      })
      expect(betaReview.review).toMatchObject({
        number: 2,
        name: 'R2-feature-beta',
        local: true,
        branch: 'feature/beta',
        status: 'open',
      })
      expect((betaBranchList.reviews as Array<{ name: string }>).map((r) => r.name)).toEqual([
        'R2-feature-beta',
      ])
      expect((allBranchesList.reviews as Array<{ name: string }>).map((r) => r.name)).toEqual([
        'R2-feature-beta',
        'R1-feature-alpha',
      ])
    })

    it('should_shift_surviving_comment_anchors_and_mark_overwritten_lines_outdated_on_local_refresh', async () => {
      // Arrange
      const { runtime } = fixture()
      await initLocalRepo(runtime)
      await runtime.write('src/lib.rs', EDITED_LIB_RS)
      const created = await runtime.json(['review', 'new', '--local'])
      const reviewName = (created.review as { name: string }).name
      const initialSnapSha = (created.review as { head_sha: string }).head_sha

      const betaComment = await runtime.json([
        'comment',
        'new',
        'Beta return value will be overwritten',
        '--file',
        'src/lib.rs',
        '--line',
        '6',
        '--local',
      ])
      const gammaComment = await runtime.json([
        'comment',
        'new',
        'Gamma signature will shift down by 2 lines',
        '--file',
        'src/lib.rs',
        '--line',
        '9',
        '--local',
        '--as',
        'claude-sonnet-4-5',
      ])

      const beforeRefreshShow = await runtime.cli(['review', 'show', reviewName, '--local'])

      // Act: insert 2 header lines at the top AND overwrite beta's line 6
      await runtime.write(
        'src/lib.rs',
        [
          '// Header line 1',
          '// Header line 2',
          'pub fn alpha() -> i32 {',
          '    10',
          '}',
          '',
          'pub fn beta() -> i32 {',
          '    999',
          '}',
          '',
          'pub fn gamma() -> i32 {',
          '    30',
          '}',
          '',
        ].join('\n'),
      )
      const refreshed = await runtime.json(['review', 'refresh', reviewName, '--local'])
      const shown = await runtime.json(['review', 'show', reviewName, '--local'])
      const afterRefreshShow = await runtime.cli(['review', 'show', reviewName, '--local'])

      // Assert
      expect(beforeRefreshShow.code).toBe(0)
      expect(beforeRefreshShow.stdout).toContain('── line 9 • unresolved')
      expect(afterRefreshShow.code).toBe(0)
      expect(afterRefreshShow.stdout).toContain('── line 6 • unresolved • outdated')
      expect(afterRefreshShow.stdout).toContain('── line 11 • unresolved')
      expect(refreshed.updated_anchors).toBe(2)
      const refreshedHeadSha = (refreshed.review as { head_sha: string }).head_sha
      expect(refreshedHeadSha).not.toBe(initialSnapSha)

      const comments = shown.comments as Array<{
        number: number
        outdated: boolean
        anchor: {
          line: number
          commit_sha: string
          original_line: number
          original_commit_sha: string
        }
      }>
      const betaAfter = comments.find(
        (c) => c.number === (betaComment.comment as { number: number }).number,
      )!
      const gammaAfter = comments.find(
        (c) => c.number === (gammaComment.comment as { number: number }).number,
      )!

      expect(betaAfter.outdated).toBe(true)
      expect(betaAfter.anchor.original_line).toBe(6)
      expect(betaAfter.anchor.original_commit_sha).toBe(initialSnapSha)

      expect(gammaAfter.outdated).toBe(false)
      expect(gammaAfter.anchor.line).toBe(11)
      expect(gammaAfter.anchor.commit_sha).toBe(refreshedHeadSha)
      expect(gammaAfter.anchor.original_line).toBe(9)
      expect(gammaAfter.anchor.original_commit_sha).toBe(initialSnapSha)
    })
  },
)

describe(
  'When drafting comments, inspecting diff hunks, and closing a local review',
  { tags: ['e2e', 'cli', 'local-review'] },
  () => {
    const fixture = useTestHarness('cli')

    it('should_create_update_reply_resolve_and_delete_local_comments_as_human_and_agent', async () => {
      // Arrange
      const { runtime } = fixture()
      await initLocalRepo(runtime)
      await runtime.write('src/lib.rs', EDITED_LIB_RS)
      await runtime.json(['review', 'new', '--local'])

      // Act
      const inline = await runtime.json([
        'comment',
        'new',
        'Inline finding on alpha',
        '--file',
        'src/lib.rs',
        '--line',
        '2',
        '--local',
      ])
      const range = await runtime.json([
        'comment',
        'new',
        'Range finding across beta function',
        '--file',
        'src/lib.rs',
        '--start-line',
        '5',
        '--line',
        '7',
        '--local',
        '--as',
        'claude-sonnet-4-5',
      ])
      const general = await runtime.json([
        'comment',
        'new',
        'General architectural note',
        '--local',
      ])
      const inlineNum = (inline.comment as { number: number }).number
      const rangeNum = (range.comment as { number: number }).number
      const generalNum = (general.comment as { number: number }).number

      const reply = await runtime.json([
        'comment',
        'new',
        'Addressed in follow-up',
        '--reply-to',
        String(inlineNum),
        '--local',
        '--as',
        'claude-sonnet-4-5',
      ])
      const updated = await runtime.json([
        'comment',
        'update',
        String(inlineNum),
        '--body',
        'Updated inline finding on alpha',
        '--resolved',
        'true',
        '--local',
      ])
      const deleted = await runtime.json(['comment', 'delete', String(generalNum), '--local'])
      const shown = await runtime.json(['review', 'show', '--local'])
      const shownText = await runtime.cli(['review', 'show', '--local'])

      // Assert
      expect(shownText.code).toBe(0)
      expect(range.comment).toMatchObject({
        number: rangeNum,
        body: 'Range finding across beta function',
        author: { name: 'claude-sonnet-4-5', kind: 'agent' },
        anchor: { file_path: 'src/lib.rs', start_line: 5, line: 7 },
      })
      expect(reply.comment).toMatchObject({
        reply_to_number: inlineNum,
        author: { name: 'claude-sonnet-4-5', kind: 'agent' },
      })
      expect(updated.comment).toMatchObject({
        number: inlineNum,
        body: 'Updated inline finding on alpha',
        resolved: true,
      })
      expect(deleted).toEqual({ deleted_number: generalNum })
      const remainingNumbers = (shown.comments as Array<{ number: number }>).map((c) => c.number)
      expect(remainingNumbers).toContain(inlineNum)
      expect(remainingNumbers).toContain(rangeNum)
      expect(remainingNumbers).not.toContain(generalNum)
    })

    it('should_render_inline_diff_hunks_and_threads_grouped_by_file_or_session_and_close_review', async () => {
      // Arrange
      const { runtime } = fixture()
      await initLocalRepo(runtime)
      await runtime.write('src/lib.rs', EDITED_LIB_RS)
      const created = await runtime.json(['review', 'new', '--local'])
      const reviewName = (created.review as { name: string }).name

      const alphaComment = await runtime.json([
        'comment',
        'new',
        'Alpha return constant check',
        '--file',
        'src/lib.rs',
        '--line',
        '2',
        '--local',
      ])
      const betaComment = await runtime.json([
        'comment',
        'new',
        'Beta range check',
        '--file',
        'src/lib.rs',
        '--start-line',
        '5',
        '--line',
        '7',
        '--local',
        '--as',
        'claude-sonnet-4-5',
      ])
      const alphaNum = (alphaComment.comment as { number: number }).number
      await runtime.json(['comment', 'update', String(alphaNum), '--resolved', 'true', '--local'])

      // Act
      const fileText = await runtime.cli([
        'review',
        'show',
        reviewName,
        '--local',
        '--group-by',
        'file',
      ])
      const sessionText = await runtime.cli([
        'review',
        'show',
        reviewName,
        '--local',
        '--group-by',
        'session',
      ])
      const unresolvedJson = await runtime.json([
        'review',
        'show',
        reviewName,
        '--local',
        '--unresolved',
      ])
      const unresolvedText = await runtime.cli([
        'review',
        'show',
        reviewName,
        '--local',
        '--unresolved',
      ])
      const closed = await runtime.json([
        'review',
        'close',
        reviewName,
        '--local',
        '--with',
        'approve',
        '--body',
        'Local review approved',
      ])

      // Assert
      expect(fileText.code).toBe(0)
      expect(fileText.stdout).toContain(`Review #1  ${reviewName}  [local]  open`)
      expect(fileText.stdout).toContain('══ src/lib.rs')
      expect(fileText.stdout).toContain('── line 2 • resolved')
      expect(fileText.stdout).toContain('── lines 5-7 • unresolved')
      expect(fileText.stdout).toContain('+  2 │     10')
      expect(fileText.stdout).toContain('+  6 │     20')
      expect(fileText.stdout).toContain('Alpha return constant check')
      expect(fileText.stdout).toContain('Beta range check')

      expect(sessionText.code).toBe(0)
      expect(sessionText.stdout).toContain('══ Session #1 • Ada Lovelace')
      expect(sessionText.stdout).toContain('══ Session #2 • claude-sonnet-4-5 (agent)')
      expect(sessionText.stdout).toContain('── src/lib.rs:5-7 • unresolved')
      expect(unresolvedText.code).toBe(0)

      const unresolvedComments = unresolvedJson.comments as Array<{ number: number }>
      expect(unresolvedComments.map((c) => c.number)).toEqual([
        (betaComment.comment as { number: number }).number,
      ])
      expect((unresolvedJson.diff_hunks as Array<{ file_path: string }>).length).toBe(1)

      expect(closed.session).toMatchObject({
        state: 'published',
        outcome: 'approve',
        body: 'Local review approved',
      })
    })
  },
)
