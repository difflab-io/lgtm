import type { Editor } from './editor.js'
import type { TestHarness } from '../common/test-harness.js'
import type { GithubFixture } from '../common/github.js'
import { selectBranchPr } from './pr-review.fixtures.js'
import { waitFor, type Json } from '../common/test-utils.js'

export const threadFlows = ['local', 'forge'] as const
export type ThreadFlow = (typeof threadFlows)[number]

export interface ThreadDiscussion {
  editor: Editor
  runtime: TestHarness
  github?: GithubFixture
  flow: ThreadFlow
  path: string
  buf: number
  threadId: string
  anchor: Json
  content: string
  excerpt: string
}

export async function prepareThread(
  flow: ThreadFlow,
  context: { editor: Editor; runtime: TestHarness; github?: GithubFixture },
): Promise<ThreadDiscussion> {
  const { editor, runtime, github } = context
  const path = github?.path ?? 'thread.txt'
  const content = github?.headContent ?? 'first\nselected\n' + 'stable context\n'.repeat(40)
  const excerpt = github ? 'changed' : 'selected'
  if (flow === 'forge') {
    if (!github) throw new Error('Forge threads require the GitHub fixture')
    await selectBranchPr(editor, github)
  } else {
    await runtime.git('init', '-q', '-b', 'main')
    await runtime.write(path, content.replace('selected', 'original'))
    await runtime.commit('base')
    await runtime.write(path, content)
    await editor.openLocal('working')
    await editor.show(path)
  }
  await editor.pane('new', 2)
  await editor.keys('c')
  await editor.compose('Thread concern')
  await editor.lua('vim.api.nvim_win_set_cursor(0,{2,0})')
  await editor.popup('Thread concern')
  await editor.keys('c')
  await editor.compose('Thread reply')
  if (flow === 'forge') {
    await editor.choose('comment')
    await editor.command('Lgtm submit')
    await editor.compose('Thread fixture review')
    await editor.wait(
      'require("lgtm.services.state").current.review.submissionStatus == "confirmed"',
      'thread discussion published',
    )
    await editor.command('Lgtm close')
    await selectBranchPr(editor, github!)
    await editor.pane('new', 2)
  }
  const state = await editor.loaded()
  const root = state.conversation.comments.find((comment: Json) =>
    comment.body.startsWith('Thread concern'),
  )
  const thread = state.conversation.threads.find((thread: Json) => thread.id === root.threadId)
  return {
    ...context,
    flow,
    path,
    buf: state.panes.new.buf,
    threadId: thread.id,
    anchor: thread.anchor,
    content,
    excerpt,
  }
}

export async function getThreadState(discussion: ThreadDiscussion): Promise<Json> {
  return (await discussion.editor.state())!
}

export async function getThreadPlacement(discussion: ThreadDiscussion): Promise<Json> {
  return discussion.editor.lua(
    `local id, path = ...; local s=require("lgtm.services.state").current
    for _,thread in ipairs(s.conversation.threads) do
      if thread.id == id then return require("lgtm.features.annotation_overlay").placement(s,thread,path,"new") end
    end`,
    discussion.threadId,
    discussion.path,
  )
}

export async function openThreadsPane(discussion: ThreadDiscussion) {
  await discussion.editor.command('Lgtm threads')
  await discussion.editor.wait(
    'require("lgtm.features.threads_pane").last and vim.api.nvim_get_current_win() == require("lgtm.features.threads_pane").last.win',
    'threads pane open',
  )
  return discussion.editor.lua(
    `local id = ...; local panel=require("lgtm.features.threads_pane").last
    for _,entry in ipairs(panel.entries) do
      if entry.thread.id == id then
        vim.api.nvim_win_set_cursor(panel.win,{entry.line,0})
        return { win=panel.win, buf=panel.buf, cursor=vim.api.nvim_win_get_cursor(panel.win) }
      end
    end
    error("Thread not found")`,
    discussion.threadId,
  )
}

export async function changeThreadSource(discussion: ThreadDiscussion) {
  const { editor, runtime, github, path, content, excerpt } = discussion
  const changed = content.replace(excerpt, 'replacement')
  if (github) {
    await github.changeFiles({ [path]: changed })
    await editor.command('Lgtm refresh')
    await editor.wait(
      `require("lgtm.services.state").current.head == "${github.head}" and require("lgtm.services.state").current.render_ready`,
      'changed PR comparison ready',
    )
    await waitFor(
      'provider marks thread outdated',
      () => getThreadPlacement(discussion),
      (placement) => placement?.status === 'outdated',
      editor.timeoutMs,
    )
  } else {
    await runtime.write(path, changed)
  }
}
