/**
 * @module-tag e2e
 * @module-tag cli
 * @module-tag local-review
 */
import { describe, it, expect } from 'vitest'
import { readFile, rename } from 'node:fs/promises'
import { join } from 'node:path'
import { useTestHarness } from '../common/fixtures.js'
import type { TestHarness } from '../common/test-harness.js'
import type { Json } from '../common/test-utils.js'

async function prepareDirectory(r: TestHarness) {
  await r.git('init', '-q', '-b', 'main')
  const plans = join(r.workspace, '.difflab/plans/private-plan')
  await r.write('.gitignore', '.difflab/\n')
  await r.write('.difflab/plans/private-plan/PLAN.md', '# Plan\nFirst task\n')
  const review = (await r.json(['session', 'start', '--directory', plans])).review
  return { plans, review }
}

async function prepareDirectoryDiscussion(r: TestHarness) {
  const { plans, review } = await prepareDirectory(r)
  const note = (
    await r.json([
      'comments',
      'add',
      '--directory',
      plans,
      '--file',
      'PLAN.md',
      '--line',
      '2',
      '--body',
      'Clarify the acceptance test',
    ])
  ).comment
  return { plans, review, note }
}

describe('When starting a local directory review', () => {
  const fixture = useTestHarness('cli')
  it('User can review ignored content and edit its feedback', { tags: ['smoke'] }, async () => {
    // Arrange
    const { runtime: r } = fixture()
    const { plans, review } = await prepareDirectory(r)

    // Act
    const note = (
      await r.json([
        'comments',
        'add',
        '--directory',
        plans,
        '--file',
        'PLAN.md',
        '--line',
        '2',
        '--body',
        'Clarify the task',
      ])
    ).comment
    await r.json([
      'comments',
      'edit',
      note.id,
      '--directory',
      plans,
      '--body',
      'Clarify the acceptance test',
    ])

    // Assert
    expect(review.state).toBe('active')
    expect(JSON.stringify(await r.json(['comments', 'list', '--directory', plans]))).toContain(
      'Clarify the acceptance test',
    )
    expect((await r.json(['session', 'export', '--directory', plans])).markdown).toContain(
      'Clarify the acceptance test',
    )
  })
})

describe('When completing and resuming a local review', () => {
  const fixture = useTestHarness('cli')
  it('User can resume, complete and start a successor while retaining discussions', async () => {
    // Arrange
    const { runtime: r } = fixture()
    const { plans, review } = await prepareDirectoryDiscussion(r)
    // Act
    await r.restart()
    const resumedReview = await r.json(['session', 'start', '--directory', plans])
    const active = await r.json(['session', 'list', '--path', plans])

    // Assert
    expect(resumedReview.review.id).toBe(review.id)
    expect(active.reviews.map((saved: Json) => saved.id)).toEqual([review.id])

    // Act
    const completedReview = await r.json(['session', 'complete', '--directory', plans])
    const completed = await r.json(['session', 'list', '--completed', '--path', plans])
    const remaining = await r.json(['session', 'list', '--path', plans])

    // Assert
    expect(completedReview.review.state).toBe('completed')
    expect(completed.reviews.map((saved: Json) => saved.id)).toEqual([review.id])
    expect(remaining.reviews).toEqual([])

    // Act
    const successor = await r.json(['session', 'start', '--directory', plans])
    const history = await r.json(['session', 'list', '--all', '--path', plans])

    // Assert
    expect(successor.review.id).not.toBe(review.id)
    expect(history.reviews.map((saved: Json) => saved.id)).toEqual(
      expect.arrayContaining([review.id, successor.review.id]),
    )
    expect(JSON.stringify(await r.json(['comments', 'list', '--directory', plans]))).toContain(
      'Clarify the acceptance test',
    )
  })
})

describe('When a local review directory moves', () => {
  const fixture = useTestHarness('cli')
  it('User can resume and export the saved revision after the source moves', async () => {
    // Arrange
    const { runtime: r } = fixture()
    const { plans, review } = await prepareDirectoryDiscussion(r)
    await rename(plans, join(r.workspace, '.difflab/plans/renamed'))
    await r.restart()

    // Act
    const resumed = (await r.json(['session', 'show', '--review', review.id])).review
    const exported = await r.json(['session', 'export', '--review', review.id])

    // Assert
    expect(resumed.id).toBe(review.id)
    expect(resumed.snapshot).toEqual(review.snapshot)
    expect(exported.markdown).toContain('Clarify the acceptance test')
  })
})

describe('When exporting a working-tree review', () => {
  const fixture = useTestHarness('cli')
  it('User gets readable old-side feedback, replies and final text without internal state', async () => {
    // Arrange
    const { runtime: r } = fixture()
    await r.git('init', '-q', '-b', 'main')
    await r.write('review.md', 'before\nold selection\nafter\n')
    await r.commit('base')
    await r.write('review.md', 'before\nnew selection\nafter\n')

    // Act
    await r.json(['session', 'start', '--mode', 'working'])
    const root = (
      await r.json([
        'comments',
        'add',
        '--mode',
        'working',
        '--file',
        'review.md',
        '--line',
        '2',
        '--side',
        'old',
        '--body',
        'Explain the fallback.\n\n- Keep **Markdown** and `code`.',
      ])
    ).comment
    await r.json([
      'comments',
      'reply',
      root.threadId,
      '--mode',
      'working',
      '--body',
      'Use the documented alternative.',
    ])
    await r.json([
      'session',
      'complete',
      '--mode',
      'working',
      '--body',
      'Please address these cases before merging.',
    ])
    const before = await r.json(['comments', 'list', '--mode', 'working']),
      output = join(r.workspace, 'feedback.md')

    // Assert
    const markdown = (await r.json(['session', 'export', '--mode', 'working', '--output', output]))
      .markdown
    expect(await readFile(output, 'utf8')).toBe(markdown)
    for (const text of [
      '1. **[COMMENT]**',
      '`review.md:~2`',
      'old (left) side',
      'Explain the fallback.',
      '- Keep **Markdown** and `code`.',
      '**Reply',
      'Use the documented alternative.',
      '## Overall feedback',
      'Please address these cases before merging.',
    ])
      expect(markdown).toContain(text)
    for (const comment of before.comments) expect(markdown).not.toContain(comment.id)
    for (const field of ['sha256:', 'originSessionId', 'postingStatus', 'contextId'])
      expect(markdown).not.toContain(field)
    expect(await r.json(['comments', 'list', '--mode', 'working'])).toEqual(before)
  })
})

describe('When staged changes become branch commits', () => {
  const fixture = useTestHarness('cli')
  it('User retains exact index evidence and feedback across the commit', async () => {
    // Arrange
    const { runtime: r } = fixture()
    await r.git('init', '-q', '-b', 'main')
    await r.write('source [literal] ü.txt', 'before\noriginal\nafter\n')
    const base = await r.commit('source base')
    await r.git('checkout', '-qb', 'feature')
    const path = 'source [literal] ü.txt'
    await r.write(path, 'before\r\nselected\r\nafter')
    await r.git('add', '--', path)
    const blob = await r.git('rev-parse', `:${path}`)
    await r.write(path, 'before\nunsaved to index\nafter\n')

    // Act
    await r.json(['session', 'start', '--mode', 'staged'])
    const saved = (
      await r.json([
        'comments',
        'add',
        '--file',
        path,
        '--line',
        '2',
        '--body',
        'Keep this staged behavior',
      ])
    ).comment

    // Assert
    const original = await r.json(['comments', 'list']),
      anchor = original.threads.find((t: Json) => t.id === saved.threadId).anchor
    expect(anchor.source.commit.sha).toBeTruthy()
    expect(anchor.snapshotId).toBeTruthy()
    expect(anchor.evidence.selectedText).toBe('selected')
    expect(anchor.source.indexFile).toBeUndefined()

    // Act
    await r.git(
      '-c',
      'user.name=E2E',
      '-c',
      'user.email=e2e@example.test',
      'commit',
      '-qm',
      'reviewed staged change',
    )
    const head = await r.git('rev-parse', 'HEAD')
    await r.json(['session', 'complete'])
    const branch = (await r.json(['session', 'start', '--mode', 'branch', '--base', base])).review

    // Assert
    expect(branch.comparison.branch).toEqual({ baseCommit: base, headCommit: head })
    const carried = await r.json(['comments', 'list'])
    expect(carried.comments.map((c: Json) => c.id)).toContain(saved.id)
    expect(carried.threads.find((t: Json) => t.id === saved.threadId).anchor).toEqual(anchor)
    const stagedMarkdown = (await r.json(['session', 'export'])).markdown
    expect(stagedMarkdown).toContain('Keep this staged behavior')
    expect(stagedMarkdown).not.toContain(blob)
  })
})
