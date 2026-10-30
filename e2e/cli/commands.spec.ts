/**
 * @module-tag e2e
 * @module-tag cli
 */
import { describe, it, expect } from 'vitest'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { useTestHarness } from '../common/fixtures.js'

describe('When using the CLI skeleton', { tags: ['e2e', 'cli', 'commands'] }, () => {
  const fixture = useTestHarness('cli')

  it(
    'should_call_the_bundled_hello_review_RPC_and_label_demo_output',
    { tags: ['smoke'] },
    async () => {
      // Arrange
      const { runtime } = fixture()

      // Act
      const reply = await runtime.json(['review', 'list', '--local'])

      // Assert
      expect(reply.demo).toBe(true)
      expect(reply.message).toMatch(/^Hello, .+!$/)
    },
  )

  it('should_reject_unimplemented_operations_without_changing_checkout_or_upstream', async () => {
    // Arrange
    const { runtime } = fixture()
    await runtime.git('init', '-q')
    await runtime.write('file.txt', 'committed\n')
    const head = await runtime.commit('initial')
    const upstream = join(runtime.directory, 'upstream.git')
    await runtime.git('clone', '--bare', runtime.workspace, upstream)
    await runtime.git('remote', 'add', 'origin', upstream)
    await runtime.write('file.txt', 'working changes\n')
    await runtime.git('add', 'file.txt')
    await runtime.write('file.txt', 'unstaged changes\n')
    const status = await runtime.git('status', '--porcelain')
    const before = {
      files: await readFile(join(runtime.workspace, 'file.txt'), 'utf8'),
      index: await readFile(join(runtime.workspace, '.git/index')),
      refs: await runtime.git('show-ref'),
      upstream: await runtime.git('ls-remote', 'origin'),
      status,
    }
    const commands = [
      ['commit', '--as', 'Agent', '-m', 'snapshot'],
      ['push'],
      ['review', 'new'],
      ['comment', 'Concern', '--as', 'Agent'],
    ]

    // Act
    const results = []
    for (const command of commands) {
      results.push(await runtime.cli([...command, '--json']))
      results.push(await runtime.cli([...command, '--local', '--json']))
    }
    for (const outcome of ['comment', 'reject', 'approve', 'request'])
      results.push(await runtime.cli(['review', 'close', 'demo', '--with', outcome, '--json']))

    // Assert
    for (const result of results) {
      expect(result.code).not.toBe(0)
      expect(JSON.parse(result.stdout).error.code).toBe('not_implemented')
    }
    expect(await runtime.git('rev-parse', 'HEAD')).toBe(head)
    expect(await readFile(join(runtime.workspace, 'file.txt'), 'utf8')).toBe(before.files)
    expect(await readFile(join(runtime.workspace, '.git/index'))).toEqual(before.index)
    expect(await runtime.git('show-ref')).toBe(before.refs)
    expect(await runtime.git('ls-remote', 'origin')).toBe(before.upstream)
    expect(await runtime.git('status', '--porcelain')).toBe(before.status)
  })

  it('should_expose_the_new_command_shape_and_reject_removed_commands_and_bad_outcomes', async () => {
    // Arrange
    const { runtime } = fixture()

    // Act
    const help = await runtime.cli(['--help'])
    const removedSession = await runtime.cli(['session', 'list'])
    const removedComments = await runtime.cli(['comments', 'list'])
    const badOutcome = await runtime.cli(['review', 'close', 'demo', '--with', 'merge'])

    // Assert
    expect(help.code).toBe(0)
    for (const name of ['commit', 'push', 'review', 'comment', 'ui', 'up', 'down', 'status'])
      expect(help.stdout).toContain(name)
    expect(removedSession.code).not.toBe(0)
    expect(removedComments.code).not.toBe(0)
    expect(badOutcome.code).not.toBe(0)
    expect(badOutcome.stderr).toContain('merge')
  })
})
