import { spawn, type ChildProcess } from 'node:child_process'
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { appendFileSync } from 'node:fs'
import { once } from 'node:events'
import { join, resolve } from 'node:path'
import { expect } from 'vitest'
import { waitFor, type Json } from './test-utils.js'

export const ROOT = resolve(import.meta.dirname, '../..')
export const binary = (name: string) =>
  join(process.env.CARGO_TARGET_DIR ?? join(ROOT, 'target'), 'debug', name)

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

export async function command(
  program: string,
  args: string[],
  options: { cwd?: string; env?: NodeJS.ProcessEnv; input?: string; timeout?: number } = {},
) {
  const child = spawn(program, args, {
    cwd: options.cwd ?? ROOT,
    env: options.env,
    stdio: ['pipe', 'pipe', 'pipe'],
  })
  let stdout = '',
    stderr = ''
  child.stdout.on('data', (chunk) => {
    stdout += chunk
  })
  child.stderr.on('data', (chunk) => {
    stderr += chunk
  })
  child.stdin.end(options.input)
  const timer = setTimeout(() => child.kill('SIGKILL'), options.timeout ?? 15_000)
  try {
    const [code] = await once(child, 'close')
    return { code: code as number | null, stdout, stderr }
  } finally {
    clearTimeout(timer)
  }
}

export async function stop(child?: ChildProcess) {
  if (!child || child.exitCode !== null || child.signalCode) return
  const closed = once(child, 'close')
  child.kill('SIGTERM')
  const timer = setTimeout(() => child.kill('SIGKILL'), 3000)
  try {
    await closed
  } finally {
    clearTimeout(timer)
  }
}

/** Own a real application process, isolated database/workspace and the clients used by one test journey. Failed runs retain evidence. */
export class TestHarness {
  endpoint = ''
  actor = ''
  rpcTimeoutMs = 40_000
  child?: ChildProcess
  env: NodeJS.ProcessEnv
  log = ''
  private constructor(
    readonly directory: string,
    readonly home: string,
    readonly workspace: string,
    readonly kind: 'backend' | 'cli',
  ) {
    this.env = isolatedEnv(home)
    this.env.GIT_CEILING_DIRECTORIES = directory
    this.env.GH_CONFIG_DIR = join(home, 'gh')
  }

  async launch() {
    await rm(join(this.home, 'server.port'), { force: true })
    this.log = ''
    const args = this.kind === 'cli' ? ['up', '--foreground', '--no-open', '--port', '0'] : []
    this.child = spawn(binary(this.kind === 'cli' ? 'lgtm' : 'lgtm-backend'), args, {
      cwd: this.workspace,
      env: { ...this.env, LGTM_BACKEND_ADDR: '127.0.0.1:0' },
      stdio: ['ignore', 'pipe', 'pipe'],
    })
    for (const stream of [this.child.stdout, this.child.stderr])
      stream?.on('data', (chunk) => {
        this.log += chunk
        appendFileSync(join(this.directory, 'backend.log'), chunk)
      })
    await waitFor(
      'backend listening socket',
      async () => {
        if (this.child?.exitCode !== null) throw new Error(`Backend exited: ${this.log}`)
        const address = this.log.match(/LGTM backend listening on (http:\/\/[^\s]+)/)?.[1]
        if (address) this.endpoint = address
        else if (this.kind === 'cli')
          this.endpoint = `http://127.0.0.1:${(await readFile(join(this.home, 'server.port'), 'utf8')).trim()}/rpc`
        if (!this.endpoint) return false
        const response = await this.response('GetCapabilities', {})
        return response.status === 200
      },
      Boolean,
      20_000,
    )
  }

  async restart() {
    await stop(this.child)
    this.endpoint = ''
    await this.launch()
  }

  async response(method: string, input: Json, actor = this.actor) {
    const service = ['EnsureIdentity', 'GetCapabilities'].includes(method)
      ? 'IdentityService'
      : ['GetDiff', 'GetFiles', 'GetFile'].includes(method)
        ? 'DiffService'
        : 'ReviewService'
    const response = await fetch(`${this.endpoint}/lgtm.v1.${service}/${method}`, {
      method: 'POST',
      signal: AbortSignal.timeout(this.rpcTimeoutMs),
      body: JSON.stringify(input),
      headers: {
        'Content-Type': 'application/json',
        'Connect-Protocol-Version': '1',
        ...(actor ? { 'x-lgtm-identity': actor } : {}),
      },
    })
    return { status: response.status, body: (await response.json()) as Json }
  }

  async rpc(method: string, input: Json = {}, actor = this.actor): Promise<Json> {
    const { status, body } = await this.response(method, input, actor)
    expect(status, `${method}: ${JSON.stringify(body)}`).toBe(200)
    return body
  }

  async identify(input: Json = { name: 'E2E reviewer', email: 'e2e@example.test' }) {
    this.actor = (await this.rpc('EnsureIdentity', input, '')).actor.id
    return this.actor
  }

  async cli(args: string[]) {
    return command(binary('lgtm'), args, {
      cwd: this.workspace,
      env: {
        ...this.env,
        LGTM_RPC_URL: this.endpoint,
        LGTM_NAME: 'E2E reviewer',
        LGTM_EMAIL: 'e2e@example.test',
      },
      timeout: 60_000,
    })
  }

  async json(args: string[]): Promise<Json> {
    const response = await this.cli([...args, '--json'])
    expect(response.code, `${args.join(' ')}\n${response.stderr}\n${response.stdout}`).toBe(0)
    return JSON.parse(response.stdout)
  }

  async git(...args: string[]) {
    const response = await command('git', ['-C', this.workspace, ...args], { env: this.env })
    expect(response.code, response.stderr).toBe(0)
    return response.stdout.trim()
  }

  async write(path: string, data: string | Buffer) {
    const full = join(this.workspace, path)
    await mkdir(resolve(full, '..'), { recursive: true })
    await writeFile(full, data)
    return full
  }

  async commit(message: string) {
    await this.git('add', '.')
    await this.git(
      '-c',
      'user.name=E2E',
      '-c',
      'user.email=e2e@example.test',
      'commit',
      '-qm',
      message,
    )
    return this.git('rev-parse', 'HEAD')
  }

  async dispose(failed = false) {
    await stop(this.child)
    if (failed) console.error(`Retained E2E evidence: ${this.directory}`)
    else await rm(this.directory, { recursive: true, force: true })
  }

  static async create(kind: 'backend' | 'cli' = 'backend', env: NodeJS.ProcessEnv = {}) {
    const parent = join(ROOT, 'target/e2e', kind)
    await mkdir(parent, { recursive: true })
    const directory = await mkdtemp(join(parent, 'journey-'))
    const home = join(directory, 'home'),
      workspace = join(directory, 'workspace')
    await mkdir(home)
    await mkdir(workspace)
    const runtime = new TestHarness(directory, home, workspace, kind)
    Object.assign(runtime.env, env)
    try {
      await runtime.launch()
      return runtime
    } catch (error) {
      await runtime.dispose(true)
      throw error
    }
  }
}
