import { createServer, type Server } from 'node:http'
import { once } from 'node:events'
import { mkdir, mkdtemp, readFile, rename, rm, writeFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { pathToFileURL } from 'node:url'
import { setTimeout as delay } from 'node:timers/promises'
import { CassetteReplay, isRead, type Cassette, type Exchange } from './github-cassette.js'
import { LiveGithubDriver, type GithubDriver, type GithubFixtureState } from './github-driver.js'
import { command, ROOT } from './test-harness.js'
import { isolatedEnv, type Json } from './test-utils.js'

type Recordings = Record<string, Cassette>

const saves = new Map<string, Promise<void>>()
let lastWrite = 0
const token = (actor: string) => `lgtm-fixture-${actor}`
const privateUserFields = new Set([
  'email',
  'plan',
  'private_gists',
  'total_private_repos',
  'owned_private_repos',
  'disk_usage',
  'two_factor_authentication',
  'collaborators',
])

/** Loopback proxy records genuine GitHub responses or strictly replays a selected cassette. */
export class RecordedGithubDriver implements GithubDriver {
  readonly env: NodeJS.ProcessEnv = {}
  private server?: Server
  private endpoint = ''
  private queue: Promise<unknown> = Promise.resolve()
  private failure?: unknown
  private replay?: CassetteReplay
  private gitReady = false
  private readonly tokens = new Map<string, Promise<string>>()
  private constructor(
    readonly file: string,
    readonly journey: string,
    readonly cassette: Cassette,
    private readonly directory: string,
    private readonly live?: LiveGithubDriver,
  ) {
    if (!live) this.replay = new CassetteReplay(cassette)
    else this.tokens.set('primary', Promise.resolve(live.env.GH_TOKEN!))
  }

  get gitSource() {
    return pathToFileURL(join(this.directory, 'repository.git')).href
  }

  async credentials(account?: string) {
    if (account && account !== (process.env.LGTM_E2E_GITHUB_SECONDARY_USER ?? 'skapoor81'))
      throw new Error(`Unknown fixture account: ${account}`)
    return { GH_TOKEN: token(account ? 'secondary' : 'primary') }
  }

  async api<T = Json>(
    route: string,
    method: string,
    body: Json | undefined,
    env: NodeJS.ProcessEnv,
  ): Promise<T> {
    const response = await fetch(`${this.endpoint}/${route}`, {
      method,
      headers: { authorization: `token ${env.GH_TOKEN}`, 'content-type': 'application/json' },
      body: body ? JSON.stringify(body) : undefined,
      signal: AbortSignal.timeout(30_000),
    })
    const raw = await response.text()
    if (!response.ok) throw new Error(`GitHub ${method} ${route}: ${raw}`)
    return (raw ? JSON.parse(raw) : {}) as T
  }

  async prepareGit(refs: string[]) {
    if (!this.gitReady) {
      await this.git(['init', '--bare', '-q', join(this.directory, 'repository.git')])
      this.gitReady = true
      if (!this.live) {
        if (!this.cassette.gitBundle)
          throw new Error(
            `Missing recorded Git objects in ${this.file}; refresh this test with e2e:record`,
          )
        await writeFile(
          join(this.directory, 'repository.bundle'),
          Buffer.from(this.cassette.gitBundle, 'base64'),
        )
        await this.git([
          '--git-dir',
          join(this.directory, 'repository.git'),
          'fetch',
          '-q',
          join(this.directory, 'repository.bundle'),
          '+refs/*:refs/*',
        ])
      }
    }
    if (this.live) {
      await this.git([
        '--git-dir',
        join(this.directory, 'repository.git'),
        '-c',
        'credential.helper=!gh auth git-credential',
        'fetch',
        '-q',
        `${this.cassette.fixture.repositoryUrl}.git`,
        ...refs.map((ref) => ref.replace(':refs/remotes/origin/', ':refs/heads/')),
      ])
      for (const ref of refs.filter((ref) => /^[0-9a-f]{40}$/.test(ref)))
        await this.git([
          '--git-dir',
          join(this.directory, 'repository.git'),
          'update-ref',
          `refs/recorded/${ref}`,
          ref,
        ])
    }
  }

  async finish(passed: boolean) {
    try {
      await this.queue
      if (passed) {
        if (this.failure) throw this.failure
        this.replay?.verify()
        if (this.live) {
          await mkdir(dirname(this.file), { recursive: true })
          if (this.gitReady) {
            await this.git([
              '--git-dir',
              join(this.directory, 'repository.git'),
              'bundle',
              'create',
              join(this.directory, 'repository.bundle'),
              '--all',
            ])
            this.cassette.gitBundle = (
              await readFile(join(this.directory, 'repository.bundle'))
            ).toString('base64')
          }
          await this.save()
          console.info(`Updated GitHub recording: ${this.file}#${this.journey}`)
        }
      }
    } finally {
      await this.dispose()
    }
  }

  async dispose() {
    if (this.server?.listening) {
      const closed = once(this.server, 'close')
      this.server.close()
      this.server.closeAllConnections()
      await closed
    }
    await rm(this.directory, { recursive: true, force: true })
  }

  /** Serialize selected updates so concurrent journeys retain their sibling entries. */
  private async save() {
    const pending = (saves.get(this.file) ?? Promise.resolve())
      .catch(() => {})
      .then(async () => {
        try {
          const recordings = await RecordedGithubDriver.getRecordings(this.file, true)
          recordings[this.journey] = this.cassette
          const ordered = Object.fromEntries(
            Object.keys(recordings)
              .sort()
              .map((key) => [key, recordings[key]]),
          )
          const serialized = JSON.stringify(ordered, null, 2) + '\n'
          for (const credential of this.tokens.values())
            if (serialized.includes(await credential))
              throw new Error('Refusing to save a GitHub credential in a recording')
          await writeFile(`${this.file}.tmp`, serialized)
          await rename(`${this.file}.tmp`, this.file)
        } finally {
          await rm(`${this.file}.tmp`, { force: true })
        }
      })
    saves.set(this.file, pending)
    try {
      await pending
    } finally {
      if (saves.get(this.file) === pending) saves.delete(this.file)
    }
  }

  private async git(args: string[]) {
    const result = await command('git', args, {
      env: {
        ...isolatedEnv(this.directory),
        ...this.live?.env,
      },
      timeout: 30_000,
    })
    if (result.code) throw new Error(`Recording Git ${args[0]}: ${result.stderr}`)
    return result.stdout
  }

  private async getToken(actor: string) {
    if (!this.tokens.has(actor))
      this.tokens.set(
        actor,
        actor === 'invalid'
          ? Promise.resolve('invalid-e2e-token')
          : actor === 'primary'
            ? Promise.resolve(this.live!.env.GH_TOKEN!)
            : this.live!.credentials(
                process.env.LGTM_E2E_GITHUB_SECONDARY_USER ?? 'skapoor81',
              ).then((env) => env.GH_TOKEN!),
      )
    return this.tokens.get(actor)!
  }

  private async exchange(request: Exchange['request'], headers: Record<string, string>) {
    if (this.replay) return this.replay.respond(request)
    if (!isRead(request)) {
      await delay(Math.max(0, 1100 - (Date.now() - lastWrite)))
      lastWrite = Date.now()
    }
    // Real credentials exist only on this upstream request; they are never recorded.
    const response = await fetch(`https://api.github.com${request.path}`, {
      method: request.method,
      headers: {
        ...headers,
        authorization: `Bearer ${await this.getToken(request.actor)}`,
        'user-agent': 'LGTM-E2E-recorder',
      },
      body: request.body ? JSON.stringify(request.body) : undefined,
      redirect: 'manual',
      signal: AbortSignal.timeout(25_000),
    })
    const raw = await response.text()
    const result = {
      status: response.status,
      body: raw ? JSON.parse(raw) : null,
      headers: Object.fromEntries(
        ['content-type', 'link', 'location']
          .filter((name) => response.headers.has(name))
          .map((name) => [name, response.headers.get(name)!]),
      ),
    }
    const saved =
      request.path === '/user' && result.status === 200
        ? {
            ...result,
            body: Object.fromEntries(
              Object.entries(result.body).filter(([key]) => !privateUserFields.has(key)),
            ),
          }
        : result
    let index = this.cassette.responses.findIndex(
      (response) => JSON.stringify(response) === JSON.stringify(saved),
    )
    if (index === -1) {
      index = this.cassette.responses.length
      this.cassette.responses.push(saved)
    }
    this.cassette.exchanges.push({ request, response: index })
    return result
  }

  private async listen() {
    this.server = createServer((incoming, outgoing) => {
      this.queue = this.queue
        .then(async () => {
          let dispatched = false
          try {
            if (incoming.aborted) return
            let raw = ''
            for await (const chunk of incoming) raw += chunk
            const supplied = incoming.headers.authorization?.replace(/^(token|Bearer) /i, '')
            const actor =
              supplied === token('primary')
                ? 'primary'
                : supplied === token('secondary')
                  ? 'secondary'
                  : 'invalid'
            const url = new URL(incoming.url!, this.endpoint)
            url.searchParams.sort()
            const headers = Object.fromEntries(
              ['accept', 'content-type', 'x-github-api-version']
                .filter((name) => typeof incoming.headers[name] === 'string')
                .map((name) => [name, incoming.headers[name] as string]),
            )
            const request = {
              actor,
              method: incoming.method!,
              path: url.pathname + url.search,
              body: raw ? JSON.parse(raw) : null,
            }
            dispatched = true
            const response = await this.exchange(request, headers)
            outgoing.writeHead(
              response.status,
              Object.fromEntries(
                Object.entries(response.headers).map(([key, value]) => [
                  key,
                  value.replaceAll('https://api.github.com', this.endpoint),
                ]),
              ),
            )
            outgoing.end(response.body === null ? '' : JSON.stringify(response.body))
          } catch (error) {
            // A cancelled, incomplete request never reached the upstream API.
            if (incoming.aborted && !dispatched) return
            this.failure ??= error
            outgoing
              .writeHead(500, { 'content-type': 'application/json' })
              .end(JSON.stringify({ message: String(error) }))
            console.error(error)
          }
        })
        .catch((error) => {
          this.failure ??= error
        })
    })
    this.server.listen(0, '127.0.0.1')
    await once(this.server, 'listening')
    const address = this.server.address()
    if (!address || typeof address === 'string')
      throw new Error('Recorder did not bind a TCP socket')
    this.endpoint = `http://127.0.0.1:${address.port}`
    Object.assign(this.env, {
      GH_TOKEN: token('primary'),
      LGTM_GITHUB_TEST_MODE: '1',
      LGTM_GITHUB_TEST_API_URL: this.endpoint,
    })
  }

  private static async getRecordings(file: string, allowMissing = false): Promise<Recordings> {
    let recordings: Recordings
    try {
      recordings = JSON.parse(await readFile(file, 'utf8'))
    } catch (error) {
      if (allowMissing && (error as NodeJS.ErrnoException).code === 'ENOENT') return {}
      throw new Error(
        `Missing or invalid recording ${file}; run premise run e2e:record for this spec`,
        {
          cause: error,
        },
      )
    }
    if (
      !recordings ||
      typeof recordings !== 'object' ||
      Array.isArray(recordings) ||
      Object.values(recordings).some(
        (cassette) =>
          !cassette ||
          cassette.version !== 1 ||
          !Array.isArray(cassette.exchanges) ||
          !Array.isArray(cassette.responses),
      )
    )
      throw new Error(`Unsupported GitHub recordings: ${file}`)
    return recordings
  }

  static async start(
    file: string,
    journey: string,
    fixture?: GithubFixtureState,
    live?: LiveGithubDriver,
  ) {
    let cassette: Cassette
    if (live && fixture) cassette = { version: 1, fixture, responses: [], exchanges: [] }
    else {
      const recordings = await this.getRecordings(file)
      cassette = recordings[journey]
      if (!cassette)
        throw new Error(
          `Missing recording ${file}#${journey}; run premise run e2e:record for this test`,
        )
    }
    const parent = join(ROOT, 'target/e2e/recordings')
    await mkdir(parent, { recursive: true })
    const driver = new RecordedGithubDriver(
      file,
      journey,
      cassette,
      await mkdtemp(join(parent, 'journey-')),
      live,
    )
    try {
      await driver.listen()
      return driver
    } catch (error) {
      await driver.dispose()
      throw error
    }
  }
}
