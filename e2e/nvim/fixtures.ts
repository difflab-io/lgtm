import { beforeEach, afterEach } from 'vitest'
import { Editor } from './editor.js'
import type { TestHarness } from '../common/test-harness.js'
import { useTestHarness } from '../common/fixtures.js'

export const profiles = ['required', 'kak'] as const

/** Add an editor to an existing runtime fixture, preserving its context and cleanup order. */
export function useEditorProcess<T extends { runtime: TestHarness }>(
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
  return useEditorProcess(useTestHarness('cli'), profile)
}
