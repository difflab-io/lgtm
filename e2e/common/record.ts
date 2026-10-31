import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { command, ROOT } from './test-harness.js'

const suites = new Set(['forge', 'backend', 'cli', 'nvim', 'all'])
const suite = process.argv[2]
const filters = process.argv.slice(3)

async function recordSuite(target: 'forge' | 'cli' | 'nvim') {
  const defaultSpec = join(ROOT, 'e2e', target, 'pr-review.spec.ts')
  if (!filters.length && !existsSync(defaultSpec)) {
    console.error(`No recordable pr-review.spec.ts found for ${target}`)
    process.exitCode = 2
    return
  }
  const specArgs = filters.length ? filters : ['pr-review.spec.ts']
  const buildTarget = target === 'cli' ? '//apps/lgtm-cli:build' : '//apps/lgtm-forge:build'
  const build = await command('mise', ['run', buildTarget], {
    cwd: ROOT,
    env: process.env,
    timeout: 180_000,
  })
  if (build.code) {
    process.stderr.write(build.stderr || build.stdout)
    process.exitCode = build.code
    return
  }

  const run = await command(
    'bun',
    ['x', 'vitest', 'run', '--config', `e2e/${target}/vitest.config.ts`, ...specArgs],
    {
      cwd: ROOT,
      env: {
        ...process.env,
        LGTM_E2E_GITHUB_MODE: 'record',
      },
      timeout: 600_000,
    },
  )
  process.stdout.write(run.stdout)
  process.stderr.write(run.stderr)
  if (run.code) process.exitCode = run.code
}

if (!suite || !suites.has(suite)) {
  console.error('Use e2e:record -- forge|cli|nvim|all [Vitest filters]')
  process.exitCode = 2
} else if (suite === 'all') {
  for (const target of ['forge', 'cli'] as const) {
    if (existsSync(join(ROOT, 'e2e', target, 'pr-review.spec.ts'))) {
      await recordSuite(target)
      if (process.exitCode) break
    }
  }
} else {
  const normalized = suite === 'backend' ? 'forge' : (suite as 'forge' | 'cli' | 'nvim')
  await recordSuite(normalized)
}
