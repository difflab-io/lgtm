import { afterAll, beforeAll, describe, expect, test } from 'bun:test'
import { $ } from 'bun'
import { fileURLToPath } from 'node:url'
import { sayHello } from './rpc'

const root = fileURLToPath(new URL('../../../../', import.meta.url))
let backend: ReturnType<typeof Bun.spawn> | undefined
let baseUrl: string

beforeAll(async () => {
  await $`mise exec -- cargo build -p lgtm-backend`.cwd(root).quiet()
  backend = Bun.spawn([`${root}/target/debug/lgtm-backend`], {
    cwd: root,
    env: { ...process.env, LGTM_BACKEND_ADDR: '127.0.0.1:0' },
    stderr: 'pipe',
    stdout: 'ignore',
  })
  const reader = (backend.stderr as ReadableStream<Uint8Array>).getReader()
  const decoder = new TextDecoder()
  let output = ''
  try {
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
  } finally {
    reader.releaseLock()
  }
}, 120000)

afterAll(async () => {
  if (backend) {
    backend.kill('SIGINT')
    await backend.exited
  }
})

describe('generated gRPC-Web client against the Rust backend', () => {
  test('returns a greeting from the server', async () => {
    expect(await sayHello('Web', baseUrl)).toBe('Hello, Web!')
  })

  test('uses the server default for an empty name', async () => {
    expect(await sayHello('', baseUrl)).toBe('Hello, World!')
  })

  test('reports an unavailable backend', async () => {
    await expect(sayHello('Web', 'http://127.0.0.1:0')).rejects.toThrow()
  })
})
