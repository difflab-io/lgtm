/**
 * @module-tag e2e
 * @module-tag backend
 * @module-tag diffs
 */
import { describe, it, expect } from 'vitest'
import { copyFile } from 'node:fs/promises'
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
      const loaded = await runtime.rpc('GetFile', {
        diff,
        path: 'PLAN.md',
        anchors: [{ original: line(bytes, 'PLAN.md') }],
      })
      const reviews = await runtime.rpc('ListReviews')

      // Assert
      expect(files.files[0].path).toBe('PLAN.md')
      expect(loaded.newSide.content).toBe(bytes)
      expect(loaded.placements[0].exact.location.start).toBe('2')
      expect(reviews.reviews ?? []).toEqual([])
    },
  )

  it('Consumers select a range and the backend captures evidence from saved files, Git commits, the index and working files', async () => {
    // Arrange
    const { runtime } = fixture()
    const { bytes, plans, diff } = await prepareSource(runtime)
    const location = { kind: 'line', path: 'PLAN.md', side: 'new', start: '2', end: '2' }

    // Act
    const local = await runtime.rpc('CaptureAnchor', {
      location,
      saved: { root: plans, path: 'PLAN.md', kind: 4 },
    })
    const comparison = await runtime.rpc('CaptureAnchor', {
      location,
      file: { diff, path: 'PLAN.md' },
      side: 'new',
    })

    // Assert
    expect(local.anchor).toEqual(line(bytes, 'PLAN.md'))
    expect(comparison.anchor).toEqual(local.anchor)

    // Arrange
    await runtime.git('init', '-q', '-b', 'main')
    await runtime.write('tracked.txt', 'before\ncommitted selection\nafter\n')
    await runtime.commit('source for backend anchor capture')
    const commit = await runtime.git('rev-parse', 'HEAD')
    await runtime.write('tracked.txt', 'before\nstaged selection\nafter\n')
    await runtime.git('add', 'tracked.txt')
    await runtime.write('tracked.txt', 'before\nworking selection\nafter\n')
    const sources = [
      { kind: 1, commit, selected: 'committed selection', variant: 'commit' },
      { kind: 2, selected: 'staged selection', variant: 'indexFile' },
      { kind: 3, selected: 'working selection', variant: 'workingFile' },
    ]
    for (const source of sources) {
      // Act
      const captured = await runtime.rpc('CaptureAnchor', {
        location: { ...location, path: 'tracked.txt' },
        saved: {
          root: runtime.workspace,
          path: 'tracked.txt',
          kind: source.kind,
          commit: source.commit,
        },
      })

      // Assert
      expect(captured.anchor.evidence.selectedText).toBe(source.selected)
      expect(captured.anchor.source[source.variant]).toBeDefined()
    }
  })

  it('A stale diff descriptor cannot silently return changed source bytes', async () => {
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
})

describe('When locating original selections in current sources', () => {
  const fixture = useTestHarness('backend')

  it('Confirmed current paths relocate selections without changing original evidence', async () => {
    // Arrange
    const { runtime } = fixture()
    const { bytes, plans, target } = await prepareSource(runtime)
    await copyFile(join(plans, 'PLAN.md'), join(plans, 'MOVED.md'))
    const diff = (await runtime.rpc('GetDiff', { target })).diff

    // Act
    const loaded = await runtime.rpc('GetFile', {
      diff,
      path: 'MOVED.md',
      anchors: [{ original: line(bytes, 'PLAN.md'), currentPath: join(plans, 'MOVED.md') }],
    })

    // Assert
    expect(loaded.placements[0].relocated.location.start).toBe('2')
  })

  it('Conflicting current-path claims remain unverified', async () => {
    // Arrange
    const { runtime } = fixture()
    const { bytes, plans, target } = await prepareSource(runtime)
    await copyFile(join(plans, 'PLAN.md'), join(plans, 'MOVED.md'))
    const diff = (await runtime.rpc('GetDiff', { target })).diff

    // Act
    const loaded = await runtime.rpc('GetFile', {
      diff,
      path: 'PLAN.md',
      anchors: [{ original: line(bytes, 'PLAN.md'), currentPath: 'MOVED.md' }],
    })

    // Assert
    expect(loaded.placements[0].unverified).toBeDefined()
  })

  it('A uniquely identifiable selection follows inserted lines', async () => {
    // Arrange
    const { runtime } = fixture()
    const { bytes, target } = await prepareSource(runtime)
    await runtime.write('.difflab/plan/example/PLAN.md', 'inserted\n' + bytes)
    const diff = (await runtime.rpc('GetDiff', { target })).diff

    // Act
    const loaded = await runtime.rpc('GetFile', {
      diff,
      path: 'PLAN.md',
      anchors: [{ original: line(bytes, 'PLAN.md') }],
    })

    // Assert
    expect(loaded.placements[0].relocated.location.start).toBe('3')
  })

  it('A lost selection has an outdated reason instead of an invented line number', async () => {
    // Arrange
    const { runtime } = fixture()
    const { bytes, target } = await prepareSource(runtime)
    await runtime.write('.difflab/plan/example/PLAN.md', 'unrelated replacement\n')
    const diff = (await runtime.rpc('GetDiff', { target })).diff

    // Act
    const loaded = await runtime.rpc('GetFile', {
      diff,
      path: 'PLAN.md',
      anchors: [{ original: line(bytes, 'PLAN.md') }],
    })

    // Assert
    expect(loaded.placements[0].outdated.reason).toEqual(expect.any(String))
    expect(loaded.placements[0].exact).toBeUndefined()
    expect(loaded.placements[0].relocated).toBeUndefined()
  })
})
