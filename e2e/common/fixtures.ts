import { beforeEach, afterEach } from 'vitest'
import { Runtime } from './runtime.js'

/** Own the backend process and database for one case. Consumers can add narrower fixtures. */
export function useRuntime(kind: 'backend' | 'cli', environment?: () => NodeJS.ProcessEnv) {
  let runtime: Runtime | undefined
  beforeEach(async () => {
    runtime = await Runtime.create(kind, environment?.())
  })
  afterEach(async ({ task }) => {
    try {
      await runtime?.dispose(task.result?.state === 'fail')
    } finally {
      runtime = undefined
    }
  })
  return () => {
    if (!runtime) throw new Error('Runtime fixture requested outside a test')
    return { runtime }
  }
}
