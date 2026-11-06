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

describe(
  'Given the CLI forge daemon / When starting, inspecting, and stopping the daemon',
  { tags: ['e2e', 'cli', 'daemon'] },
  () => {
    const fixture = useTestHarness('cli')

    it(
      'should start the background forge RPC server and return server_url and rpc_url without ui_url',
      { tags: ['smoke'] },
      async () => {
        // Arrange
        const { runtime } = fixture()

        // Act
        const up = await runtime.json(['up', '--port', '0'])
        const status = await runtime.json(['status'])

        // Assert
        expect(up.server_url).toMatch(/^http:\/\/127\.0\.0\.1:\d+/)
        expect(up.rpc_url).toBe(`${up.server_url}/rpc`)
        expect(up.ui_url).toBeUndefined()
        expect(status).toEqual({
          running: true,
          pid: up.pid,
          server_url: up.server_url,
          rpc_url: up.rpc_url,
        })
      },
    )

    it('should serve health and Connect RPC while returning 404 for root static GET requests', async () => {
      // Arrange
      const { runtime } = fixture()
      const base = runtime.endpoint.replace(/\/rpc$/, '')

      // Act
      const health = await fetch(`${base}/health`)
      const rootGet = await fetch(base)
      const greet = await fetch(`${runtime.endpoint}/lgtm.v1.GreeterService/SayHello`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'Connect-Protocol-Version': '1',
        },
        body: JSON.stringify({ name: 'Daemon' }),
      })

      // Assert
      expect(health.status).toBe(200)
      expect(rootGet.status).toBe(404)
      expect(greet.status).toBe(200)
      expect(await greet.json()).toEqual({ message: 'Hello, Daemon!' })
    })

    it('should stop the background forge daemon cleanly on down', async () => {
      // Arrange
      const { runtime } = fixture()

      // Act
      const stopped = await runtime.cli(['down'])
      const status = await runtime.json(['status'])

      // Assert
      expect(stopped.code).toBe(0)
      expect(status).toMatchObject({ running: false })
    })
  },
)

describe(
  'Given a target workspace directory / When launching the desktop app from the CLI',
  { tags: ['e2e', 'cli', 'daemon'] },
  () => {
    const fixture = useTestHarness('cli')

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

    it('should fail with invalid_argument when the target workspace path does not exist', async () => {
      // Arrange
      const { runtime } = fixture()

      // Act
      const missing = await runtime.cli(['open', 'does-not-exist', '--json'])

      // Assert
      expect(missing.code).not.toBe(0)
      expect(JSON.parse(missing.stdout).error.code).toBe('invalid_argument')
    })
  },
)
