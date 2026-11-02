import { afterAll, beforeAll, describe, expect, test } from 'bun:test'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { getReviewDemo } from './services/review'

const root = fileURLToPath(new URL('../../../../', import.meta.url))
let backend: ReturnType<typeof Bun.spawn> | undefined
let home: string | undefined
let baseUrl: string

// Mise builds the backend before this suite; compilation has no test-hook deadline.
beforeAll(async () => {
  home = await mkdtemp(join(tmpdir(), 'lgtm-web-rpc-'))
  backend = Bun.spawn([join(root, 'target/debug/lgtm-forge')], {
    cwd: root,
    env: {
      ...process.env,
      LGTM_HOME: home,
      LGTM_BACKEND_ADDR: '127.0.0.1:0',
    },
    stderr: 'pipe',
    stdout: 'ignore',
  })
  const reader = (backend.stderr as ReadableStream<Uint8Array>).getReader()
  const decoder = new TextDecoder()
  let output = ''
  let timer: ReturnType<typeof setTimeout> | undefined
  try {
    await Promise.race([
      (async () => {
        while (true) {
          const chunk = await reader.read()
          if (chunk.done) throw new Error(`Backend exited before listening: ${output}`)
          output += decoder.decode(chunk.value, { stream: true })
          const match = output.match(/listening on (http:\/\/127\.0\.0\.1:\d+)/)
          if (match) {
            baseUrl = match[1]
            return
          }
        }
      })(),
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => {
          backend?.kill('SIGKILL')
          reject(new Error(`Backend did not listen within 20 seconds: ${output}`))
        }, 20_000)
      }),
    ])
  } finally {
    clearTimeout(timer)
    await reader.cancel()
    reader.releaseLock()
  }
}, 25_000)

afterAll(async () => {
  if (backend) {
    backend.kill('SIGINT')
    await backend.exited
  }
  if (home) await rm(home, { recursive: true, force: true })
})

describe('Given the web frontend service / When calling the Rust greeting RPC', () => {
  test('should return demo data with the real greeting', async () => {
    expect(await getReviewDemo('Web', baseUrl)).toEqual({ demo: true, message: 'Hello, Web!' })
  })

  test('should use the server default for an empty name', async () => {
    expect((await getReviewDemo('', baseUrl)).message).toBe('Hello, World!')
  })

  test('should report an unavailable backend without fake success', async () => {
    await expect(getReviewDemo('Web', 'http://127.0.0.1:0')).rejects.toThrow()
  })
})
