// Shared fixtures: spawn `lgtm-server` + the TanStack SPA dev server with
// isolated port + db + repo. Tests get an `Env` with pre-wired helpers:
//
//   const env = await spawnEnv()
//   await env.cli('comment', 'add', env.repoPath, '--path', 'src/lib.rs', ...)
//   await page.goto(env.webUrl + '?repo=' + encodeURIComponent(env.repoPath) + '&base=HEAD')
//
// Each spawnEnv uses tmpdirs that get cleaned on teardown.

import { spawn, spawnSync } from 'node:child_process'
import { mkdtempSync, rmSync, writeFileSync, mkdirSync, unlinkSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createServer } from 'node:net'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const REPO_ROOT = resolve(__dirname, '..', '..', '..')
const SERVER_BIN = resolve(REPO_ROOT, 'target', 'debug', 'lgtm-server')
const CLI_BIN = resolve(REPO_ROOT, 'target', 'debug', 'lgtm')

export interface Env {
  backendUrl: string
  webUrl: string
  repoPath: string
  dbPath: string
  cli: (...args: string[]) => Promise<{ stdout: string; stderr: string; code: number }>
  teardown: () => Promise<void>
}

export async function spawnEnv(): Promise<Env> {
  const workDir = mkdtempSync(join(tmpdir(), 'lgtm-e2e-'))
  const dbPath = join(workDir, 'db.sqlite')
  const repoPath = resolve(initRepoFixture(join(workDir, 'repo')))

  const port = await pickFreePort()
  const backendUrl = `http://127.0.0.1:${port}`

  const server = spawn(SERVER_BIN, [], {
    env: {
      ...process.env,
      LGTM_ADDR: `127.0.0.1:${port}`,
      LGTM_DB_PATH: dbPath,
      RUST_LOG: 'warn',
    },
    stdio: 'ignore',
  })

  await waitForBackend(backendUrl, 10_000)

  // Spawn the SPA dev server with the same backend baked in via URL params
  // on each page.goto. The dev server itself doesn't need any env vars.
  const webPort = await pickFreePort()
  const web = spawn(
    'bun',
    ['run', 'dev', '--port', String(webPort), '--strictPort'],
    {
      cwd: resolve(REPO_ROOT, 'apps', 'lgtm-web-tanstack-spa'),
      env: {
        ...process.env,
        PORT: String(webPort),
      },
      stdio: 'ignore',
    },
  )
  await waitForUrl(`http://localhost:${webPort}/`, 15_000)

  return {
    backendUrl,
    webUrl: `http://localhost:${webPort}`,
    repoPath,
    dbPath,
    async cli(...args) {
      return runCli(backendUrl, args)
    },
    async teardown() {
      server.kill('SIGKILL')
      web.kill('SIGKILL')
      await delay(50) // let the ports release
      try {
        rmSync(workDir, { recursive: true, force: true })
      } catch {}
    },
  }
}

function runCli(backend: string, args: string[]) {
  return new Promise<{ stdout: string; stderr: string; code: number }>((resolve, reject) => {
    const proc = spawn(CLI_BIN, ['--backend', backend, ...args], { stdio: ['ignore', 'pipe', 'pipe'] })
    let stdout = ''
    let stderr = ''
    proc.stdout.on('data', (b) => (stdout += b.toString()))
    proc.stderr.on('data', (b) => (stderr += b.toString()))
    proc.on('error', reject)
    proc.on('close', (code) => resolve({ stdout, stderr, code: code ?? 1 }))
  })
}

// ---- fixtures ----

function initRepoFixture(dir: string): string {
  mkdirSync(dir, { recursive: true })
  // Minimal git repo with a base commit on `src/lib.rs` and a working-tree
  // modification so there's always exactly one changed file.
  writeFileSync(join(dir, 'README.md'), '# fixture\n')
  mkdirSync(join(dir, 'src'), { recursive: true })
  writeFileSync(join(dir, 'src', 'lib.rs'), 'pub fn hello() {}\n')
  runSync('git', ['-C', dir, 'init', '-q'])
  runSync('git', ['-C', dir, 'config', 'user.email', 'fixture@example.com'])
  runSync('git', ['-C', dir, 'config', 'user.name', 'fixture'])
  runSync('git', ['-C', dir, 'add', '.'])
  runSync('git', ['-C', dir, 'commit', '-q', '-m', 'base'])
  // Working-tree change: an extra function on line 2.
  writeFileSync(join(dir, 'src', 'lib.rs'), 'pub fn hello() {}\npub fn hi() {}\n')
  // On macOS canonicalize so the path matches what `lgtm-cli` and the SPA
  // canonicalize to.
  return runSync('realpath', [dir]).stdout.trim()
}

function runSync(cmd: string, args: string[]): { stdout: string } {
  const proc = spawnSync(cmd, args, { encoding: 'utf-8' })
  if (proc.status !== 0) {
    throw new Error(`${cmd} ${args.join(' ')} failed: ${proc.stderr}`)
  }
  return { stdout: proc.stdout ?? '' }
}

// ---- helpers ----

function pickFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const s = createServer()
    s.unref()
    s.on('error', reject)
    s.listen(0, '127.0.0.1', () => {
      const port = (s.address() as { port: number }).port
      s.close(() => resolve(port))
    })
  })
}

async function waitForBackend(baseUrl: string, timeoutMs: number) {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    try {
      const r = await fetch(`${baseUrl}/lgtm.v1.LgtmService/UpsertUser`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: '__probe', kind: 'AUTHOR_KIND_AGENT' }),
      })
      if (r.ok) return
    } catch {}
    await delay(50)
  }
  throw new Error(`backend at ${baseUrl} not ready in ${timeoutMs}ms`)
}

async function waitForUrl(url: string, timeoutMs: number) {
  const deadline = Date.now() + timeoutMs
  while (Date.now() < deadline) {
    try {
      const r = await fetch(url)
      if (r.ok || r.status < 500) return
    } catch {}
    await delay(100)
  }
  throw new Error(`${url} not ready in ${timeoutMs}ms`)
}

function delay(ms: number) {
  return new Promise((r) => setTimeout(r, ms))
}

export function openUrl(env: Env, params: Record<string, string> = {}): string {
  const qs = new URLSearchParams({
    repo: env.repoPath,
    base: 'HEAD',
    backend: env.backendUrl,
    ...params,
  })
  return `${env.webUrl}/?${qs.toString()}`
}

// Prevent writeFileSync + unlinkSync + rmSync from being tree-shaken out.
export const _keepImports = { writeFileSync, unlinkSync, rmSync }
