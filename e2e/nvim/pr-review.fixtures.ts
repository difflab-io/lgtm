import { usePrFixture } from '../common/pr.fixtures.js'
import type { GithubFixture } from '../common/github.js'
import type { Editor } from './editor.js'
import { useEditorProcess, type profiles } from './fixtures.js'

export function usePrEditor(profile: (typeof profiles)[number]) {
  return useEditorProcess(usePrFixture('cli'), profile, {
    timeoutMs: 40_000,
    prepare: async ({ runtime, github }) => {
      await github.checkout(runtime)
    },
  })
}

/** Operate the real branch picker; tests of picker behavior use the individual actions. */
export async function selectBranchPr(editor: Editor, github: GithubFixture, command = 'Lgtm prs') {
  if (command === 'Lgtm review') await editor.choose('PR')
  await editor.command(command)
  await editor.wait(
    'require("lgtm.picker").last and not require("lgtm.picker").last.closed and vim.api.nvim_get_current_buf() == require("lgtm.picker").last.buf and vim.fn.mode():sub(1,1) == "i"',
    'branch PR picker',
  )
  await editor.keys(`#${github.number} `)
  await editor.wait(
    `require("lgtm.picker").last.query == ${JSON.stringify(`#${github.number} `)} and #require("lgtm.picker").last.filtered == 1 and tonumber(require("lgtm.picker").last.filtered[1].number) == ${github.number}`,
    'typed query selects the fixture PR',
  )
  await editor.keys('<CR>')
  return editor.loaded()
}
