import { beforeEach, afterEach } from 'vitest'
import { TestHarness } from './test-harness.js'

/** Own the application process and workspace for one case. Consumers can add narrower fixtures. */
export function useTestHarness(
  kind: 'forge' | 'backend' | 'cli',
  environment?: () => NodeJS.ProcessEnv,
) {
  let runtime: TestHarness | undefined
  beforeEach(async ({ task }) => {
    runtime = await TestHarness.create(kind, environment?.(), {
      filepath: task.file.filepath,
      name: task.name,
      fullName: task.fullName,
    })
  })
  afterEach(async ({ task }) => {
    try {
      await runtime?.dispose(task.result?.state === 'fail')
    } finally {
      runtime = undefined
    }
  })
  return () => {
    if (!runtime) throw new Error('TestHarness fixture requested outside a test')
    return { runtime }
  }
}
