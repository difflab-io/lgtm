import { test as base } from 'vitest'
import { Github } from './github.js'
import { Runtime } from './runtime.js'

export function suite(kind: 'backend' | 'cli') {
  return base.extend<{ github: Github; runtime: Runtime }>({
    github: async ({ task }, use) => {
      const github = await Github.start()
      console.info(`Given isolated GitHub fixture for ${task.name}`)
      try {
        await use(github)
      } finally {
        await github.close()
      }
      if (github.errors.length)
        throw new Error(`Unexpected provider requests: ${github.errors.join('\n')}`)
    },
    runtime: async ({ github, task }, use) => {
      const runtime = await Runtime.create(kind, github)
      try {
        await use(runtime)
      } finally {
        await runtime.dispose(task.result?.state === 'fail')
      }
    },
  })
}
