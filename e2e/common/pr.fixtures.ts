import { beforeAll, beforeEach, afterEach, afterAll } from 'vitest'
import { createHash } from 'node:crypto'
import { join, relative } from 'node:path'
import { GithubFixture } from './github.js'
import { useTestHarness } from './fixtures.js'
import { ROOT } from './test-harness.js'

/** Live groups share run-owned resources; each replay/record cassette covers one whole test. */
export function usePrFixture(kind: 'backend' | 'cli') {
  let github: GithubFixture | undefined
  const fixtures: GithubFixture[] = []
  let merged = false
  const mode = process.env.LGTM_E2E_GITHUB_MODE ?? 'replay'
  if (!['replay', 'record', 'live'].includes(mode))
    throw new Error('LGTM_E2E_GITHUB_MODE must be replay, record, or live')
  beforeAll(async () => {
    if (mode !== 'replay') {
      github = await GithubFixture.start()
      fixtures.push(github)
    }
  })
  beforeEach(async ({ task }) => {
    const spec = relative(join(ROOT, 'e2e'), task.file.filepath).replaceAll('\\', '/')
    const title = task.fullName.replace(/ \[(?:required|kak)\]/g, '')
    const slug = task.name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .slice(0, 80)
    const id = createHash('sha256').update(title).digest('hex').slice(0, 8)
    const file = join(ROOT, 'e2e/recordings', spec.replace(/\.spec\.ts$/, '.json'))
    const journey = `${slug}-${id}`
    if (mode === 'replay') {
      github = undefined
      github = await GithubFixture.replay(file, journey)
    } else {
      if (merged) {
        github = await GithubFixture.start()
        fixtures.push(github)
        merged = false
      }
      if (mode === 'record') await github!.record(file, journey)
    }
    await github!.prepareSnapshots()
    console.info(`${mode} GitHub journey: ${spec}#${journey}`)
  })
  // Stack order stops the editor/backend before recording or replaying cleanup.
  afterEach(async ({ task }) => {
    if (!github) return
    let passed = task.result?.state !== 'fail'
    try {
      const pr = await github.api(`repos/${github.repository}/pulls/${github.number}`)
      merged = pr.merged
      if (!merged) await github.reset(pr)
    } catch (error) {
      passed = false
      throw error
    } finally {
      if (mode !== 'live') await github.finish(passed)
      if (mode === 'replay') github = undefined
    }
  })
  afterAll(async () => {
    const results = await Promise.allSettled(fixtures.map((fixture) => fixture.dispose()))
    const errors = results
      .filter((result) => result.status === 'rejected')
      .map((result) => result.reason)
    if (errors.length) throw new AggregateError(errors, 'GitHub fixture cleanup failed')
  })
  const runtime = useTestHarness(kind, () => github!.env)
  return () => {
    if (!github) throw new Error('GitHub fixture requested outside a test')
    return { ...runtime(), github }
  }
}
