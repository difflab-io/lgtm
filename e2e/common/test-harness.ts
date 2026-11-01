import { spawn, type ChildProcess } from 'node:child_process'
import { appendFile, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { appendFileSync } from 'node:fs'
import { once } from 'node:events'
import { dirname, join, relative, resolve } from 'node:path'
import { expect } from 'vitest'
import { isolatedEnv, waitFor, type Json } from './test-utils.js'

export interface TestTaskMeta {
  filepath: string
  name: string
  fullName: string
}

function formatCliArg(arg: string): string {
  return /[\s"']/.test(arg) ? JSON.stringify(arg) : arg
}

function formatAuthorDisplay(author?: Json): string {
  if (!author) return 'unknown'
  return author.kind === 'agent' ? `${author.name} (agent)` : String(author.name)
}

function formatTextInspectionOutput(args: string[], stdout: string): string | null {
  const trimmed = stdout.trimEnd()
  if (!args.includes('--json')) return trimmed
  if (!trimmed) return ''
  if (args[0] === 'review' && (args[1] === 'show' || args[1] === 'list')) {
    return null
  }
  try {
    const data = JSON.parse(trimmed) as Json
    if (data.error) {
      return `Error [${data.error.code}]: ${data.error.message}`
    }
    if (args[0] === 'add' && Array.isArray(data.staged_paths)) {
      const mode = data.local ? 'local snapshot' : 'working tree'
      return `Staged ${data.staged_paths.length} path(s) (${mode}): ${data.staged_paths.join(', ')}`
    }
    if (args[0] === 'commit' && typeof data.commit_sha === 'string') {
      const mode = data.local ? 'local snapshot' : 'working tree'
      return `Committed ${data.commit_sha.slice(0, 7)} on ${data.branch} (${mode}) by ${formatAuthorDisplay(data.author)}`
    }
    if (args[0] === 'push' && typeof data.head_sha === 'string') {
      return `Pushed ${data.branch} (${data.head_sha.slice(0, 7)}) to ${data.repository_url}`
    }
    if (args[0] === 'review' && args[1] === 'new' && data.review && data.session) {
      const scope = data.review.local ? 'local' : 'remote'
      return `Created review #${data.review.number}  ${data.review.name}  [${scope}]  (session #${data.session.number} ${data.session.state})`
    }
    if (args[0] === 'review' && args[1] === 'refresh' && data.review) {
      const scope = data.review.local ? 'local' : 'remote'
      return `Refreshed review #${data.review.number}  ${data.review.name}  [${scope}]  Head: ${String(data.review.head_sha).slice(0, 7)}  (${data.updated_anchors} anchor(s) updated)`
    }
    if (args[0] === 'review' && args[1] === 'close' && data.review && data.session) {
      return `Closed review #${data.review.number}  ${data.review.name}  with ${data.session.outcome} (session #${data.session.number} ${data.session.state})`
    }
    if (args[0] === 'comment' && args[1] === 'new' && data.comment) {
      return `Created comment [${data.comment.number}] by ${formatAuthorDisplay(data.comment.author)}`
    }
    if (args[0] === 'comment' && args[1] === 'update' && data.comment) {
      return `Updated comment [${data.comment.number}]`
    }
    if (args[0] === 'comment' && args[1] === 'delete' && data.deleted_number !== undefined) {
      return `Deleted comment [${data.deleted_number}]`
    }
    if (args[0] === 'ui' && typeof data.ui_url === 'string') {
      return data.ui_url
    }
    if (args[0] === 'status' && typeof data.running === 'boolean') {
      return data.running
        ? `LGTM backend is running (ui: ${data.ui_url ?? 'n/a'})`
        : 'LGTM backend is not running'
    }
    return trimmed
  } catch {
    return trimmed
  }
}

export const ROOT = resolve(import.meta.dirname, '../..')
export const binary = (name: string) =>
  join(process.env.CARGO_TARGET_DIR ?? join(ROOT, 'target'), 'debug', name)

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

/** Own a real application process, isolated home/workspace and clients for one test journey. Failed runs retain evidence. */
export class TestHarness {
  endpoint = ''
  rpcTimeoutMs = 40_000
  child?: ChildProcess
  env: NodeJS.ProcessEnv
  log = ''
  cliOutputPath?: string
  private constructor(
    readonly directory: string,
    readonly home: string,
    readonly workspace: string,
    readonly kind: 'forge' | 'backend' | 'cli',
  ) {
    this.env = isolatedEnv(home)
    this.env.GIT_CEILING_DIRECTORIES = directory
    this.env.GH_CONFIG_DIR = join(home, 'gh')
  }

  async launch() {
    if (this.env.LGTM_GITHUB_TEST_MODE === '1')
      this.env.LGTM_GITHUB_TEST_ACTOR =
        this.env.GH_TOKEN === 'lgtm-fixture-primary'
          ? 'primary'
          : this.env.GH_TOKEN === 'lgtm-fixture-secondary'
            ? 'secondary'
            : 'invalid'
    await rm(join(this.home, 'server.port'), { force: true })
    this.log = ''
    const args = this.kind === 'cli' ? ['up', '--foreground', '--no-open', '--port', '0'] : []
    this.child = spawn(binary(this.kind === 'cli' ? 'lgtm' : 'lgtm-forge'), args, {
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
        const response = await this.response('HelloReview', { name: 'E2E' })
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

  async response(method: string, input: Json) {
    const response = await fetch(`${this.endpoint}/lgtm.v1.ReviewService/${method}`, {
      method: 'POST',
      signal: AbortSignal.timeout(this.rpcTimeoutMs),
      body: JSON.stringify(input),
      headers: {
        'Content-Type': 'application/json',
        'Connect-Protocol-Version': '1',
      },
    })
    return { status: response.status, body: (await response.json()) as Json }
  }

  async rpc(method: string, input: Json = {}): Promise<Json> {
    const { status, body } = await this.response(method, input)
    expect(status, `${method}: ${JSON.stringify(body)}`).toBe(200)
    return body
  }

  async cli(args: string[]) {
    const result = await command(binary('lgtm'), args, {
      cwd: this.workspace,
      env: {
        ...this.env,
        LGTM_RPC_URL: this.endpoint,
      },
      timeout: 60_000,
    })
    const textOut = formatTextInspectionOutput(args, result.stdout)
    if (textOut !== null) {
      const displayArgs = args.filter((arg) => arg !== '--json')
      const sections = [
        '',
        '================================================================================',
        `$ lgtm ${displayArgs.map(formatCliArg).join(' ')}`,
        `[exit ${result.code ?? 'null'}]`,
      ]
      if (textOut) {
        sections.push(textOut)
      }
      if (result.stderr) {
        sections.push('--- stderr ---', result.stderr.trimEnd())
      }
      const entry = `${sections.join('\n')}\n`
      await appendFile(join(this.directory, 'cli.log'), entry)
      if (this.cliOutputPath) {
        await appendFile(this.cliOutputPath, entry)
      }
    }
    return result
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

  static async create(
    kind: 'forge' | 'backend' | 'cli' = 'forge',
    env: NodeJS.ProcessEnv = {},
    task?: TestTaskMeta,
  ) {
    const parent = join(ROOT, 'target/e2e', kind)
    await mkdir(parent, { recursive: true })
    const directory = await mkdtemp(join(parent, 'journey-'))
    const home = join(directory, 'home'),
      workspace = join(directory, 'workspace')
    await mkdir(home)
    await mkdir(workspace)
    const runtime = new TestHarness(directory, home, workspace, kind)
    Object.assign(runtime.env, env)
    if (kind === 'cli' && task) {
      const relSpec = relative(join(ROOT, 'e2e'), task.filepath)
        .replaceAll('\\', '/')
        .replace(/\.spec\.ts$/, '')
      const slug = task.name.replace(/[^a-zA-Z0-9_-]+/g, '_')
      runtime.cliOutputPath = join(ROOT, 'e2e/outputs', relSpec, `${slug}.log`)
      await mkdir(dirname(runtime.cliOutputPath), { recursive: true })
      await writeFile(
        runtime.cliOutputPath,
        `# Test: ${task.fullName}\n# Spec: e2e/${relSpec}.spec.ts\n`,
      )
    }
    try {
      await runtime.launch()
      return runtime
    } catch (error) {
      await runtime.dispose(true)
      throw error
    }
  }
}
