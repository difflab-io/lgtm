import { beforeAll, beforeEach, afterEach, afterAll } from 'vitest'
import { GithubFixture } from './github.js'
import { useTestHarness } from './fixtures.js'

/** Share one owned PR per state group, with a fresh process and database for each case. */
export function usePrFixture(kind: 'backend' | 'cli') {
  let github: GithubFixture
  const fixtures: GithubFixture[] = []

  beforeAll(async () => {
    github = await GithubFixture.start()
    fixtures.push(github)
  })
  beforeEach(async () => {
    const pr = await github.api(`repos/${github.repository}/pulls/${github.number}`)
    if (pr.merged) {
      github = await GithubFixture.start()
      fixtures.push(github)
    }
  })
  // Stack order stops the editor and backend before resetting their remote data.
  afterEach(async () => {
    if (!github) return
    const pr = await github.api(`repos/${github.repository}/pulls/${github.number}`)
    if (!pr.merged) await github.reset()
  })
  afterAll(async () => {
    const results = await Promise.allSettled(fixtures.map((fixture) => fixture.dispose()))
    const errors = results
      .filter((result) => result.status === 'rejected')
      .map((result) => result.reason)
    if (errors.length) throw new AggregateError(errors, 'GitHub fixture cleanup failed')
  })
  const runtime = useTestHarness(kind, () => github.env)
  return () => ({ ...runtime(), github })
}
