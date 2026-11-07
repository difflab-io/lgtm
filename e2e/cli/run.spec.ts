/**
 * @module-tag e2e
 * @module-tag cli
 * @module-tag commands
 */
import { describe, it, expect } from 'vitest'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { useTestHarness } from '../common/fixtures.js'
import { initLocalRepo } from './local-review.fixtures.js'

describe(
  'Given a local workspace / When running LGTM-managed Mise tasks via lgtm run',
  { tags: ['e2e', 'cli', 'commands'] },
  () => {
    const fixture = useTestHarness('cli')

    it(
      'should synthesize .lgtm/mise.toml, exclude .lgtm/ in git, and list default and custom tasks',
      { tags: ['smoke'] },
      async () => {
        // Arrange
        const { runtime } = fixture()
        await initLocalRepo(runtime)
        await runtime.write(
          '.lgtm/mise.toml',
          [
            '[tasks.greet]',
            'description = "Print greeting marker"',
            'run = "echo lgtm-mise-greet-ok"',
            '',
          ].join('\n'),
        )

        // Act
        const listed = await runtime.json(['run', '--list'])
        const textListed = await runtime.cli(['run', '--list'])
        const miseToml = await readFile(join(runtime.workspace, '.lgtm', 'mise.toml'), 'utf8')
        const gitExclude = await readFile(
          join(runtime.workspace, '.git', 'info', 'exclude'),
          'utf8',
        )
        const gitStatus = await runtime.git('status', '--porcelain')

        // Assert
        expect(listed.mise_file).toContain('.lgtm/mise.toml')
        expect(listed.tasks).toEqual(
          expect.arrayContaining([
            expect.objectContaining({
              name: 'check',
              run: 'cargo check',
            }),
            expect.objectContaining({
              name: 'rust:check',
              run: 'cargo check',
            }),
            expect.objectContaining({
              name: 'greet',
              description: 'Print greeting marker',
              run: 'echo lgtm-mise-greet-ok',
            }),
          ]),
        )
        expect(textListed.code).toBe(0)
        expect(textListed.stdout).toContain('greet')
        expect(miseToml).toContain('rust-analyzer = "latest"')
        expect(miseToml).toContain('echo lgtm-mise-greet-ok')
        expect(gitExclude).toContain('.lgtm/')
        expect(gitStatus).toBe('')
      },
    )

    it('should proxy task execution through the managed .lgtm/mise.toml file', async () => {
      // Arrange
      const { runtime } = fixture()
      await initLocalRepo(runtime)
      await runtime.write(
        '.lgtm/config.json',
        `${JSON.stringify({ lsp: { auto_install: false } }, null, 2)}\n`,
      )
      await runtime.write(
        '.lgtm/mise.toml',
        [
          '[tasks.write-marker]',
          'description = "Write marker file"',
          'run = "printf \'managed-run-ok\' > .lgtm/marker.txt"',
          '',
        ].join('\n'),
      )

      // Act
      const runResult = await runtime.cli(['run', 'write-marker'])
      const marker = await readFile(join(runtime.workspace, '.lgtm', 'marker.txt'), 'utf8')

      // Assert
      expect(runResult.code).toBe(0)
      expect(marker).toBe('managed-run-ok')
    })
  },
)
