import { createHash } from 'node:crypto'
import { setTimeout as delay } from 'node:timers/promises'

// Connect JSON accepts arbitrary wire fields at the process boundary.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type Json = Record<string, any>
export const hash = (bytes: string | Buffer) =>
  `sha256:${createHash('sha256').update(bytes).digest('hex')}`

export async function waitFor<T>(
  description: string,
  read: () => Promise<T>,
  ready: (value: T) => boolean = Boolean,
  timeout = 10_000,
): Promise<T> {
  const deadline = Date.now() + timeout
  let last: unknown
  while (Date.now() < deadline) {
    try {
      const value = await read()
      if (ready(value)) return value
    } catch (error) {
      last = error
    }
    await delay(20)
  }
  throw new Error(`Timed out: ${description}`, { cause: last })
}
