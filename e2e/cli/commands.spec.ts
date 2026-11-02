/**
 * @module-tag e2e
 * @module-tag cli
 */
import { describe, it, expect } from 'vitest'
import { readdir } from 'node:fs/promises'
import { useTestHarness } from '../common/fixtures.js'

describe(
  'When validating CLI command shape and workspace boundaries',
  { tags: ['e2e', 'cli', 'commands'] },
  () => {
    const fixture = useTestHarness('cli')

    it(
      'should expose the new command shape and reject removed commands and bad outcomes',
      { tags: ['smoke'] },
      async () => {
        // Arrange
        const { runtime } = fixture()

        // Act
        const help = await runtime.cli(['--help'])
        const reviewHelp = await runtime.cli(['review', '--help'])
        const commentHelp = await runtime.cli(['comment', '--help'])
        const removedSession = await runtime.cli(['session', 'list'])
        const removedComments = await runtime.cli(['comments', 'list'])
        const badOutcome = await runtime.cli(['review', 'close', 'demo', '--with', 'merge'])

        // Assert
        expect(help.code).toBe(0)
        for (const name of [
          'add',
          'commit',
          'push',
          'review',
          'comment',
          'ui',
          'up',
          'down',
          'status',
        ])
          expect(help.stdout).toContain(name)
        for (const sub of ['list', 'new', 'refresh', 'show', 'close'])
          expect(reviewHelp.stdout).toContain(sub)
        for (const sub of ['new', 'update', 'delete']) expect(commentHelp.stdout).toContain(sub)
        expect(removedSession.code).not.toBe(0)
        expect(removedComments.code).not.toBe(0)
        expect(badOutcome.code).not.toBe(0)
        expect(badOutcome.stderr).toContain('merge')
      },
    )

    it('should fail outside a git repository without mutating workspace or home', async () => {
      // Arrange
      const { runtime } = fixture()

      // Act
      const listOut = await runtime.cli(['review', 'list', '--local', '--json'])
      const addOut = await runtime.cli(['add', 'file.txt', '--local', '--json'])
      const workspaceEntries = await readdir(runtime.workspace)

      // Assert
      expect(listOut.code).not.toBe(0)
      expect(JSON.parse(listOut.stdout).error.code).toBe('invalid_argument')
      expect(addOut.code).not.toBe(0)
      expect(JSON.parse(addOut.stdout).error.code).toBe('invalid_argument')
      expect(workspaceEntries).toEqual([])
    })
  },
)
