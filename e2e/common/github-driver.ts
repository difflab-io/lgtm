import { command } from './test-harness.js'
import type { Json } from './test-utils.js'

export type GithubFixtureState = {
  repository: string
  repositoryUrl: string
  run: string
  nodeId: string
  base: string
  head: string
  initialHead: string
  number: number
}

/** The API and credentials vary; fixture setup and consumer journeys do not. */
export interface GithubDriver {
  readonly env: NodeJS.ProcessEnv
  readonly gitSource?: string
  prepareGit?(refs: string[]): Promise<void>
  credentials(account?: string): Promise<NodeJS.ProcessEnv>
  api<T = Json>(
    route: string,
    method: string,
    body: Json | undefined,
    env: NodeJS.ProcessEnv,
  ): Promise<T>
  dispose(): Promise<void>
}

export class LiveGithubDriver implements GithubDriver {
  private constructor(readonly env: NodeJS.ProcessEnv) {}

  async credentials(account?: string) {
    if (account && process.env.LGTM_E2E_GITHUB_SECONDARY_TOKEN)
      return { GH_TOKEN: process.env.LGTM_E2E_GITHUB_SECONDARY_TOKEN }
    const primary =
      process.env.LGTM_E2E_GITHUB_TOKEN ?? process.env.GH_TOKEN ?? process.env.GITHUB_TOKEN
    if (!account && primary) return { GH_TOKEN: primary }
    const selected = account ?? process.env.LGTM_E2E_GITHUB_PRIMARY_USER ?? 'skapoor8'
    const env = { ...process.env }
    delete env.GH_TOKEN
    delete env.GITHUB_TOKEN
    const result = await command(
      'gh',
      ['auth', 'token', '--hostname', 'github.com', '--user', selected],
      { env },
    )
    if (result.code || !result.stdout.trim())
      throw new Error(
        `GitHub credentials unavailable for ${selected}. Run gh auth login or set the matching E2E token.`,
      )
    return { GH_TOKEN: result.stdout.trim() }
  }

  async api<T = Json>(
    route: string,
    method: string,
    body: Json | undefined,
    env: NodeJS.ProcessEnv,
  ): Promise<T> {
    const result = await command(
      'gh',
      [
        'api',
        '--hostname',
        'github.com',
        '--method',
        method,
        route,
        ...(body ? ['--input', '-'] : []),
      ],
      {
        input: body ? JSON.stringify(body) : undefined,
        timeout: 20_000,
        env: { ...process.env, ...env },
      },
    )
    if (result.code !== 0)
      throw new Error(
        `GitHub ${method} ${route}: ${result.code === null ? 'process timed out or was interrupted' : result.stderr}`,
      )
    return (result.stdout ? JSON.parse(result.stdout) : {}) as T
  }

  async dispose() {}

  static async start(env?: NodeJS.ProcessEnv) {
    const driver = new LiveGithubDriver({})
    Object.assign(driver.env, env ?? (await driver.credentials()))
    return driver
  }
}
