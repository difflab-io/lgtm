import { DatabaseSync } from 'node:sqlite'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { test as base } from 'vitest'
import { Github } from '../common/github.js'
import { Editor } from '../common/editor.js'
import { ROOT, Runtime } from '../common/runtime.js'

export const profiles = ['required', 'kak'] as const
export function suite(profile: (typeof profiles)[number]) {
  return base.extend<{ editor: Editor; runtime: Runtime; github: Github }>({
    github: async ({ task }, use) => {
      const github = await Github.start()
      console.info(`Given isolated editor profile ${profile} for ${task.name}`)
      github.control({ base: 'a'.repeat(40), head: 'b'.repeat(40) })
      try {
        await use(github)
      } finally {
        await github.close()
      }
      if (github.errors.length) throw new Error(github.errors.join('\n'))
    },
    runtime: async ({ github, task }, use) => {
      const runtime = await Runtime.create('cli', github, async (home) => {
        const database = new DatabaseSync(join(home, 'reviews.sqlite'))
        try {
          database.exec(
            await readFile(
              join(ROOT, 'libs/lgtm-db/src/tests/fixtures/installed_20261019.sql'),
              'utf8',
            ),
          )
        } finally {
          database.close()
        }
      })
      try {
        await use(runtime)
      } finally {
        await runtime.dispose(task.result?.state === 'fail')
      }
    },
    editor: async ({ runtime, task }, use) => {
      const editor = new Editor(runtime, profile)
      try {
        await editor.start()
        await use(editor)
      } finally {
        if (task.result?.state === 'fail') console.error(await editor.diagnostics())
        await editor.close()
      }
    },
  })
}
