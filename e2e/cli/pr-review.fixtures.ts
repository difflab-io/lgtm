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

export function normalizeDiffAndThreadsOutput(stdout: string): string {
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
