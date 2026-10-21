import { DatabaseSync } from 'node:sqlite'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { beforeEach, afterEach } from 'vitest'
import { Editor } from './editor.js'
import { ROOT, type Runtime } from '../common/runtime.js'
import { useRuntime } from '../common/fixtures.js'

export const profiles = ['required', 'kak'] as const

/** Exercise an upgrade from the installed schema in every editor case. */
export async function seedEditorDatabase(home: string) {
  const database = new DatabaseSync(join(home, 'reviews.sqlite'))
  try {
    database.exec(
      await readFile(join(ROOT, 'libs/lgtm-db/src/tests/fixtures/installed_20261019.sql'), 'utf8'),
    )
  } finally {
    database.close()
  }
}

/** Add an editor to an existing runtime fixture, preserving its context and cleanup order. */
export function useEditorProcess<T extends { runtime: Runtime }>(
  fixture: () => T,
  profile: (typeof profiles)[number],
  options: { prepare?: (context: T) => Promise<void>; timeoutMs?: number } = {},
) {
  let editor: Editor | undefined
  beforeEach(async () => {
    const context = fixture()
    await options.prepare?.(context)
    editor = new Editor(context.runtime, profile)
    if (options.timeoutMs) editor.timeoutMs = options.timeoutMs
    await editor.start()
  })
  afterEach(async ({ task }) => {
    const current = editor
    try {
      if (task.result?.state === 'fail' && current) console.error(await current.diagnostics())
    } finally {
      editor = undefined
      await current?.close()
    }
  })
  return () => {
    if (!editor) throw new Error('Editor fixture requested outside a test')
    return { ...fixture(), editor }
  }
}

export function useEditor(profile: (typeof profiles)[number]) {
  return useEditorProcess(useRuntime('cli', seedEditorDatabase), profile)
}
