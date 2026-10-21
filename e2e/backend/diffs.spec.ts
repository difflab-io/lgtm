/**
 * @module-tag e2e
 * @module-tag backend
 * @module-tag diffs
 */
import { describe, it, expect } from 'vitest'
import { copyFile } from 'node:fs/promises'
import { join } from 'node:path'
import { useRuntime } from '../common/fixtures.js'
import type { Runtime } from '../common/runtime.js'
import { line } from './anchors.js'

async function prepareSource(runtime: Runtime) {
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
  const fixture = useRuntime('backend')

  it('Consumers can read ignored content without creating a review', async () => {
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
  const fixture = useRuntime('backend')

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
