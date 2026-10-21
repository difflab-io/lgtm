import { usePrRuntime } from '../common/pr-runtime.js'
import type { GithubFixture } from '../common/github.js'
import type { Editor } from './editor.js'
import { seedEditorDatabase, useEditorProcess, type profiles } from './fixtures.js'

export function usePrEditor(profile: (typeof profiles)[number]) {
  return useEditorProcess(usePrRuntime('cli', seedEditorDatabase), profile, {
    timeoutMs: 40_000,
    prepare: async ({ runtime, github }) => {
      await github.checkout(runtime)
      await github.waitForSearch()
    },
  })
}

/** Operate the real branch picker; tests of picker behavior use the individual actions. */
export async function selectBranchPr(editor: Editor, github: GithubFixture, command = 'Lgtm prs') {
  if (command === 'Lgtm review') await editor.choose('PR')
  await editor.command(command)
  await editor.wait(
    'require("lgtm.picker").last and not require("lgtm.picker").last.closed',
    'branch PR picker',
  )
  await editor.keys(`#${github.number} `)
  await editor.wait('#require("lgtm.picker").last.filtered == 1', 'selected branch PR')
  await editor.keys('<CR>')
  return editor.loaded()
}
