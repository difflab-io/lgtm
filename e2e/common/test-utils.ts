import { join } from 'node:path'
import { createHash } from 'node:crypto'
import { setTimeout as delay } from 'node:timers/promises'

// Connect JSON accepts arbitrary wire fields at the process boundary.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type Json = Record<string, any>
export const hash = (bytes: string | Buffer) =>
  `sha256:${createHash('sha256').update(bytes).digest('hex')}`

export async function waitFor<T>(
  description: string,
  read: () => Promise<T>,
  ready: (value: T) => boolean = Boolean,
  timeout = 10_000,
): Promise<T> {
  const deadline = Date.now() + timeout
  let last: unknown
  while (Date.now() < deadline) {
    try {
      const value = await read()
      if (ready(value)) return value
    } catch (error) {
      last = error
    }
    await delay(20)
  }
  throw new Error(`Timed out: ${description}`, { cause: last })
}

export function isolatedEnv(home: string): NodeJS.ProcessEnv {
  const env = {
    ...process.env,
    HOME: home,
    LGTM_HOME: home,
    GIT_CEILING_DIRECTORIES: home,
    GIT_CONFIG_GLOBAL: '/dev/null',
    GIT_CONFIG_NOSYSTEM: '1',
    XDG_CONFIG_HOME: join(home, 'config'),
    XDG_DATA_HOME: join(home, 'data'),
    XDG_STATE_HOME: join(home, 'state'),
    XDG_CACHE_HOME: join(home, 'cache'),
  }
  for (const key of [
    'GH_TOKEN',
    'GITHUB_TOKEN',
    'LGTM_E2E_GITHUB_TOKEN',
    'LGTM_E2E_GITHUB_SECONDARY_TOKEN',
    'LGTM_RPC_URL',
    'LGTM_DATABASE_URL',
    'LGTM_BACKEND_ADDR',
    'LGTM_GITHUB_TEST_MODE',
    'LGTM_GITHUB_TEST_API_URL',
    'LGTM_IDENTITY',
  ])
    delete env[key as keyof typeof env]
  return env
}
