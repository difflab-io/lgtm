import { spawn, type ChildProcess } from 'node:child_process'
import { beforeEach, afterEach, expect } from 'vitest'
import { randomUUID } from 'node:crypto'
import { binary, stop, TestHarness } from '../common/test-harness.js'
import { waitFor, type Json } from '../common/test-utils.js'

/** One remote account namespace per case; every device owns its own PGlite process. */
class Cloud {
  endpoint = ''
  log = ''
  child?: ChildProcess
  readonly alice = randomUUID()
  readonly bob = randomUUID()

  async start() {
    this.log = ''
    const address = this.endpoint ? new URL(this.endpoint).host : '127.0.0.1:0'
    this.child = spawn(binary('lgtm-sync-service'), [], {
      env: {
        ...process.env,
        LGTM_SYNC_DATABASE_URL: process.env.LGTM_TEST_POSTGRES_URL,
        LGTM_SYNC_ADDR: address,
        LGTM_SYNC_USERS: JSON.stringify({ 'test-alice': this.alice, 'test-bob': this.bob }),
      },
      stdio: ['ignore', 'pipe', 'pipe'],
    })
    this.child.stderr?.on('data', (chunk) => {
      this.log += chunk
    })
    await waitFor(
      'cloud service startup',
      async () => {
        if (this.child?.exitCode !== null) throw new Error(this.log)
        const address = this.log.match(/LGTM sync service listening on (\S+)/)?.[1]
        if (!address) return false
        this.endpoint = `http://${address}`
        return (await this.request('/v1/account')).status === 200
      },
      Boolean,
      20000,
    )
  }

  async request(path: string, body?: Json, token = 'test-alice') {
    return fetch(`${this.endpoint}${path}`, {
      method: body ? 'POST' : 'GET',
      headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
      body: body ? JSON.stringify(body) : undefined,
      signal: AbortSignal.timeout(5000),
    })
  }

  async pull(token = 'test-alice'): Promise<Json> {
    const response = await this.request('/v1/pull', undefined, token)
    expect(response.status, this.log).toBe(200)
    return response.json()
  }
}

export function useSyncFixture() {
  let a: TestHarness, b: TestHarness, cloud: Cloud
  beforeEach(async () => {
    if (!process.env.LGTM_TEST_POSTGRES_URL) throw new Error('Run premise run poc-test-remote')
    cloud = new Cloud()
    await cloud.start()
    a = await TestHarness.create()
    b = await TestHarness.create()
  })
  afterEach(async ({ task }) => {
    await a?.dispose(task.result?.state === 'fail')
    await b?.dispose(task.result?.state === 'fail')
    await stop(cloud?.child)
  })
  return () => ({ a, b, cloud })
}

export async function signIn(device: TestHarness, endpoint: string) {
  device.env.LGTM_SYNC_URL = endpoint
  device.env.LGTM_SYNC_TOKEN = 'test-alice'
  await device.restart()
}

export async function waitForComment(device: TestHarness, scope: Json, body: string) {
  return waitFor(
    `downloaded comment: ${body}`,
    () => device.rpc('GetConversation', { scope }),
    (value) => value.comments?.some((comment: Json) => comment.body === body),
    20000,
  )
}
