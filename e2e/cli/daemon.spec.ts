/**
 * @module-tag e2e
 * @module-tag cli
 */
import { describe, it, expect } from 'vitest'
import { useTestHarness } from '../common/fixtures.js'

describe('When opening the bundled UI', { tags: ['e2e', 'cli', 'daemon'] }, () => {
  const fixture = useTestHarness('cli')

  it('should reuse the daemon and return a usable UI URL', { tags: ['smoke'] }, async () => {
    // Arrange
    const { runtime } = fixture()

    // Act
    const result = await runtime.json(['ui', '--no-open'])
    const url = result.ui_url as string
    const page = await fetch(url)
    const status = await runtime.json(['status'])

    // Assert
    expect(url).toMatch(/^http:\/\/127\.0\.0\.1:\d+/)
    expect(page.status).toBe(200)
    expect(await page.text()).toContain('LGTM')
    expect(status).toMatchObject({ running: true })
  })

  it('should serve health and stop the application', async () => {
    // Arrange
    const { runtime } = fixture()
    const base = runtime.endpoint.replace(/\/rpc$/, '')

    // Act
    const health = await fetch(`${base}/health`)
    const stopped = await runtime.cli(['down'])
    const status = await runtime.json(['status'])

    // Assert
    expect(health.status).toBe(200)
    expect(stopped.code).toBe(0)
    expect(status).toMatchObject({ running: false })
  })

  it('should start a new daemon for ui when none is running', async () => {
    // Arrange
    const { runtime } = fixture()
    await runtime.cli(['down'])
    try {
      // Act
      const result = await runtime.json(['ui', '--port', '0', '--no-open'])
      const page = await fetch(result.ui_url as string)
      const status = await runtime.json(['status'])

      // Assert
      expect(page.status).toBe(200)
      expect(status).toMatchObject({ running: true })
      expect(status.ui_url).toBe(result.ui_url)
    } finally {
      await runtime.cli(['down'])
    }
  })
})
