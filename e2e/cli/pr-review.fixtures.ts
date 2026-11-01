import type { GithubFixture } from '../common/github.js'
import type { TestHarness } from '../common/test-harness.js'

export async function setupPrCheckout(runtime: TestHarness, github: GithubFixture) {
  await github.checkout(runtime)
  await runtime.git('config', 'user.name', 'Ada Lovelace')
  await runtime.git('config', 'user.email', 'ada@example.test')
  return {
    prNumber: github.number!,
    filePath: github.path,
    initialHead: github.head,
  }
}
