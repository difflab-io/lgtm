import type { Editor } from './editor.js'
import type { TestHarness } from '../common/test-harness.js'
import type { GithubFixture } from '../common/github.js'
import { selectBranchPr } from './pr-review.fixtures.js'
import { waitFor, type Json } from '../common/test-utils.js'

export const threadFlows = ['local', 'notes', 'forge'] as const
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

/** Establish an actual discussion; forge cases import published GitHub feedback. */
export async function prepareThread(
  flow: ThreadFlow,
  context: { editor: Editor; runtime: TestHarness; github?: GithubFixture },
): Promise<ThreadDiscussion> {
  const { editor, runtime, github } = context
  const path = github?.path ?? 'thread.txt'
  const content = github?.headContent ?? 'first\nselected\n' + 'stable context\n'.repeat(40)
  const excerpt = github ? 'changed' : 'selected'
  if (flow === 'forge') {
    if (!github) throw new Error('Forge threads require the real GitHub fixture')
    await selectBranchPr(editor, github)
    await editor.pane('new', 2)
    await editor.keys('c')
  } else {
    if (flow === 'local') {
      await runtime.git('init', '-q', '-b', 'main')
      await runtime.write(path, content.replace('selected', 'original'))
      await runtime.commit('base')
    }
    const file = await runtime.write(path, content)
    if (flow === 'notes') {
      await editor.edit(file)
      await editor.command('2Lgtm note')
    } else {
      await editor.openLocal('working')
      await editor.show(path)
      await editor.pane('new', 2)
      await editor.keys('c')
    }
  }
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
      'thread discussion published to GitHub',
    )
    await editor.command('Lgtm close')
    await selectBranchPr(editor, github!)
    await editor.pane('new', 2)
  }
  const buf = await editor.value<number>('vim.api.nvim_get_current_buf()')
  const state =
    flow === 'notes'
      ? await editor.lua(
          'local s=require("lgtm.services.state").notes[...]; return {conversation=s.conversation}',
          buf,
        )
      : (await editor.loaded())!
  const root = state.conversation.comments.find((comment: Json) =>
    comment.body.startsWith('Thread concern'),
  )
  if (!root) throw new Error('Thread concern not found')
  const thread = state.conversation.threads.find((thread: Json) => thread.id === root.threadId)
  if (!thread) throw new Error('Thread discussion not found')
  return {
    ...context,
    flow,
    path,
    buf,
    threadId: thread.id,
    anchor: thread.anchor,
    content,
    excerpt,
  }
}

export async function getThreadState(discussion: ThreadDiscussion): Promise<Json> {
  if (discussion.flow === 'notes')
    return discussion.editor.lua(
      'local s=require("lgtm.services.state").notes[...]; return {conversation=s.conversation}',
      discussion.buf,
    )
  return (await discussion.editor.state())!
}

export async function getThreadPlacement(discussion: ThreadDiscussion): Promise<Json> {
  return discussion.editor.lua(
    `local notes, buf, id, path = ...
    local s = notes and require("lgtm.services.state").notes[buf] or require("lgtm.services.state").current
    for _, thread in ipairs(s.conversation.threads) do
      if thread.id == id then return require("lgtm.features.annotation_overlay").placement(s, thread, path, "new") end
    end
    error("Thread not found")`,
    discussion.flow === 'notes',
    discussion.buf,
    discussion.threadId,
    discussion.path,
  )
}

export async function openThreadsPane(discussion: ThreadDiscussion): Promise<Json> {
  await discussion.editor.command(discussion.flow === 'notes' ? 'Lgtm notes' : 'Lgtm threads')
  await discussion.editor.wait(
    'require("lgtm.features.threads_pane").last and vim.api.nvim_win_is_valid(require("lgtm.features.threads_pane").last.win) and vim.api.nvim_get_current_win() == require("lgtm.features.threads_pane").last.win',
    'threads pane open',
  )
  return discussion.editor.lua(
    `local id = ...; local panel = require("lgtm.features.threads_pane").last
    for _, entry in ipairs(panel.entries) do
      if entry.thread.id == id then
        vim.api.nvim_win_set_cursor(panel.win, {entry.line, 0})
        return {win=panel.win, buf=panel.buf, cursor=vim.api.nvim_win_get_cursor(panel.win)}
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
      `require("lgtm.services.state").current.head == "${github.head}" and not require("lgtm.services.state").current.initializing`,
      'changed PR comparison ready',
    )
    await editor.loaded()
  } else {
    await runtime.write(path, changed)
    if (discussion.flow === 'notes') {
      await editor.command('edit!')
      await editor.lua('require("lgtm.services.state").notes[...].refresh()', discussion.buf)
    } else await editor.command('Lgtm refresh')
  }
  await waitFor(
    'changed thread source has no confirmed line',
    () => getThreadPlacement(discussion),
    (placement) => placement?.status === (discussion.flow === 'local' ? 'stale' : 'outdated'),
    editor.timeoutMs,
  )
}
