/**
 * @module-tag e2e
 * @module-tag cli
 */
import { describe, it, expect } from 'vitest'
import { useTestHarness } from '../common/fixtures.js'

describe('Given a stopped forge daemon', { tags: ['e2e', 'cli', 'daemon'] }, () => {
  const fixture = useTestHarness('cli')

  describe('When starting the daemon with up', () => {
    it(
      'should start the background forge RPC server and return server_url and rpc_url without ui_url',
      { tags: ['smoke'] },
      async () => {
        // Arrange
        const { runtime } = fixture()
        await runtime.cli(['down'])

        // Act
        const up = await runtime.json(['up', '--port', '0'])

        // Assert
        expect(up.running).toBe(true)
        expect(up.server_url).toMatch(/^http:\/\/127\.0\.0\.1:\d+/)
        expect(up.rpc_url).toBe(`${up.server_url}/rpc`)
        expect(up.ui_url).toBeUndefined()
      },
    )
  })

  describe('When checking status', () => {
    it('should report running false when no daemon is active', async () => {
      // Arrange
      const { runtime } = fixture()
      await runtime.cli(['down'])

      // Act
      const status = await runtime.json(['status'])

      // Assert
      expect(status).toEqual({ running: false })
    })
  })

  describe('When stopping the daemon with down', () => {
    it('should exit 0 and report that LGTM is not running', async () => {
      // Arrange
      const { runtime } = fixture()
      await runtime.cli(['down'])

      // Act
      const down = await runtime.cli(['down'])

      // Assert
      expect(down.code).toBe(0)
      expect(down.stdout).toContain('LGTM is not running')
    })
  })
})

describe('Given a running forge daemon', { tags: ['e2e', 'cli', 'daemon'] }, () => {
  const fixture = useTestHarness('cli')

  describe('When starting the daemon again with up', () => {
    it('should reuse the existing daemon pid and urls', async () => {
      // Arrange
      const { runtime } = fixture()
      const initial = await runtime.json(['status'])

      // Act
      const again = await runtime.json(['up', '--port', '0'])

      // Assert
      expect(again).toEqual({
        running: true,
        pid: initial.pid,
        server_url: initial.server_url,
        rpc_url: initial.rpc_url,
      })
    })
  })

  describe('When checking status', () => {
    it('should return running true with pid, server_url, and rpc_url', async () => {
      // Arrange
      const { runtime } = fixture()
      const base = runtime.endpoint.replace(/\/rpc$/, '')

      // Act
      const status = await runtime.json(['status'])

      // Assert
      expect(status.running).toBe(true)
      expect(status.server_url).toBe(base)
      expect(status.rpc_url).toBe(runtime.endpoint)
      expect(status.ui_url).toBeUndefined()
    })
  })

  describe('When sending HTTP and Connect RPC requests', () => {
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
  })

  describe('When stopping the daemon with down', () => {
    it('should stop the background forge daemon cleanly and transition status to running false', async () => {
      // Arrange
      const { runtime } = fixture()

      // Act
      const stopped = await runtime.cli(['down'])
      const status = await runtime.json(['status'])

      // Assert
      expect(stopped.code).toBe(0)
      expect(status).toEqual({ running: false })
    })
  })
})
