/**
 * @module-tag e2e
 * @module-tag cli
 */
import { chmod, mkdir, readFile, realpath, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { describe, it, expect } from 'vitest'
import { useTestHarness } from '../common/fixtures.js'
import { binary, command } from '../common/test-harness.js'
import { waitFor } from '../common/test-utils.js'

describe('Given an existing workspace directory', { tags: ['e2e', 'cli', 'open'] }, () => {
  const fixture = useTestHarness('cli')

  describe('When launching the desktop app from the CLI', () => {
    it(
      'should resolve the workspace directory and spawn lgtm-desktop via lgtm open [path] and lgtm [path]',
      { tags: ['smoke'] },
      async () => {
        // Arrange
        const { runtime } = fixture()
        const targetDir = join(runtime.workspace, 'subproject')
        await mkdir(targetDir, { recursive: true })
        const canonicalTarget = await realpath(targetDir)
        const markerPath = join(runtime.directory, 'desktop-launched.log')
        const fakeDesktop = join(runtime.directory, 'fake-lgtm-desktop')
        await writeFile(
          fakeDesktop,
          `#!/bin/sh\nprintf '%s|%s\\n' "$1" "$LGTM_WORKSPACE" >> "${markerPath}"\n`,
        )
        await chmod(fakeDesktop, 0o755)
        const env = { ...runtime.env, LGTM_DESKTOP_BIN: fakeDesktop }

        // Act
        const openSubcommand = await command(binary('lgtm'), ['open', 'subproject', '--json'], {
          cwd: runtime.workspace,
          env,
        })
        const positionalOpen = await command(binary('lgtm'), ['subproject', '--json'], {
          cwd: runtime.workspace,
          env,
        })

        // Assert
        expect(openSubcommand.code).toBe(0)
        expect(JSON.parse(openSubcommand.stdout)).toMatchObject({
          workspace_path: canonicalTarget,
        })
        expect(positionalOpen.code).toBe(0)
        expect(JSON.parse(positionalOpen.stdout)).toMatchObject({
          workspace_path: canonicalTarget,
        })
        const launchedLines = await waitFor(
          'desktop launch marker lines',
          async () => {
            try {
              const raw = await readFile(markerPath, 'utf8')
              return raw.trim().split('\n').filter(Boolean)
            } catch {
              return []
            }
          },
          (lines) => lines.length === 2,
          5_000,
        )
        expect(launchedLines).toEqual([
          `${canonicalTarget}|${canonicalTarget}`,
          `${canonicalTarget}|${canonicalTarget}`,
        ])
      },
    )
  })

  describe('When the configured desktop binary does not exist', () => {
    it('should fail with not_found', async () => {
      // Arrange
      const { runtime } = fixture()
      const missingDesktop = join(runtime.directory, 'missing-lgtm-desktop')
      const env = { ...runtime.env, LGTM_DESKTOP_BIN: missingDesktop }

      // Act
      const result = await command(binary('lgtm'), ['open', '.', '--json'], {
        cwd: runtime.workspace,
        env,
      })

      // Assert
      expect(result.code).not.toBe(0)
      expect(JSON.parse(result.stdout).error.code).toBe('not_found')
    })
  })
})

describe('Given a non-existent workspace path', { tags: ['e2e', 'cli', 'open'] }, () => {
  const fixture = useTestHarness('cli')

  describe('When launching the desktop app with lgtm open', () => {
    it('should fail with invalid_argument', async () => {
      // Arrange
      const { runtime } = fixture()

      // Act
      const missing = await runtime.cli(['open', 'does-not-exist', '--json'])

      // Assert
      expect(missing.code).not.toBe(0)
      expect(JSON.parse(missing.stdout).error.code).toBe('invalid_argument')
    })
  })
})
