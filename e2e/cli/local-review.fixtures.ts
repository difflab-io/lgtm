import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import type { TestHarness } from '../common/test-harness.js'

export const INITIAL_LIB_RS = [
  'pub fn alpha() -> i32 {',
  '    1',
  '}',
  '',
  'pub fn beta() -> i32 {',
  '    2',
  '}',
  '',
  'pub fn gamma() -> i32 {',
  '    3',
  '}',
  '',
].join('\n')

export const EDITED_LIB_RS = [
  'pub fn alpha() -> i32 {',
  '    10',
  '}',
  '',
  'pub fn beta() -> i32 {',
  '    20',
  '}',
  '',
  'pub fn gamma() -> i32 {',
  '    30',
  '}',
  '',
].join('\n')

export async function initLocalRepo(runtime: TestHarness, branch = 'feature/local-review') {
  await runtime.git('init', '-q', '-b', branch)
  await runtime.git('config', 'user.name', 'Ada Lovelace')
  await runtime.git('config', 'user.email', 'ada@example.test')
  await runtime.write('README.md', '# LGTM Local Review Fixture\n')
  await runtime.write('src/lib.rs', INITIAL_LIB_RS)
  const baseSha = await runtime.commit('Initial fixture commit')
  const upstream = join(runtime.directory, 'upstream.git')
  await runtime.git('clone', '-q', '--bare', runtime.workspace, upstream)
  await runtime.git('remote', 'add', 'origin', upstream)
  return { baseSha, upstream }
}

export async function captureCheckoutState(runtime: TestHarness) {
  return {
    head: await runtime.git('rev-parse', 'HEAD'),
    status: await runtime.git('status', '--porcelain'),
    index: await readFile(join(runtime.workspace, '.git/index')),
    originRefs: await runtime.git('ls-remote', 'origin'),
  }
}
