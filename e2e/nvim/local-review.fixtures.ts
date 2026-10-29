import type { Editor } from './editor.js'
import { type TestHarness } from '../common/test-harness.js'
import { type Json } from '../common/test-utils.js'
import { join } from 'node:path'
import { mkdir, symlink } from 'node:fs/promises'

/** Establish real index-backed feedback, while the working file already has a different line layout. */
export async function prepareStagedFeedback(editor: Editor, runtime: TestHarness) {
  await runtime.git('init', '-q', '-b', 'main')
  const path = 'src/source [literal] ü.txt'
  const moved = 'moved/source [literal] ü.txt'
  const tail = [
    ...Array.from({ length: 10 }, (_, index) => `stable context ${index + 1}`),
    'footer',
  ]
  const stagedBytes = 'header\r\nselected\r\n' + tail.join('\r\n')
  const shifted = 'intro\nheader\nselected\n' + tail.join('\n') + '\n'
  await runtime.write(path, 'header\noriginal\n' + tail.join('\n') + '\n')
  await runtime.commit('base')
  await runtime.git('checkout', '-qb', 'feature')
  await runtime.write(path, stagedBytes)
  await runtime.git('add', '--', path)
  const oid = await runtime.git('rev-parse', `:${path}`)
  await runtime.write(path, shifted)
  await editor.openLocal('staged')
  await editor.show(path)
  await editor.pane('new', 2)
  await editor.keys('c')
  await editor.compose('Preserve this staged selection')
  const state = (await editor.comments(1))!
  const rootId = state.conversation.comments[0].threadId
  const original = state.conversation.threads[0].anchor
  await editor.popup()
  await editor.keys('c')
  await editor.compose('Keep this reply with the original discussion')
  const replied = (await editor.comments(2))!
  const replyId = replied.conversation.comments.find(
    (comment: Json) => Number(comment.position) === 1,
  ).id
  return { path, moved, stagedBytes, shifted, oid, rootId, original, replyId }
}

export async function prepareCommittedFeedback(editor: Editor, runtime: TestHarness) {
  const feedback = await prepareStagedFeedback(editor, runtime)
  await editor.command('Lgtm complete')
  await editor.wait(
    'require("lgtm.services.state").current.review.state == "completed"',
    'staged review completed',
  )
  await runtime.git(
    '-c',
    'user.name=E2E',
    '-c',
    'user.email=e2e@example.test',
    'commit',
    '-qm',
    'reviewed index',
  )
  await editor.openLocal('branch')
  await editor.show(feedback.path)
  return feedback
}

export async function prepareWorkingFeedback(editor: Editor, runtime: TestHarness) {
  await runtime.git('init', '-q', '-b', 'main')
  await runtime.write('other.txt', 'original\n')
  await runtime.commit('base')
  await runtime.write('other.txt', 'working selection\n')
  await editor.edit(join(runtime.workspace, 'other.txt'))
  const originalBuffer = await editor.value<number>('vim.api.nvim_get_current_buf()')
  const beforeMapping = await editor.value('vim.fn.maparg("c","n",false,true).desc')
  await editor.openLocal('working')
  await editor.show('other.txt')
  await editor.pane()
  await editor.keys('c')
  await editor.compose('Retain this unstaged feedback')
  const state = (await editor.comments(1))!
  return { workingId: state.conversation.threads[0].id, beforeMapping, originalBuffer }
}

export async function prepareIgnoredWorkspace(runtime: TestHarness) {
  await runtime.git('init', '-q', '-b', 'main')
  await runtime.write('.gitignore', '.difflab\n')
  await runtime.commit('base')
  await mkdir(join(runtime.directory, 'plans'))
  await symlink(join(runtime.directory, 'plans'), join(runtime.workspace, '.difflab'), 'dir')
  const plans = join(runtime.workspace, '.difflab/specific-plan')
  await runtime.write('.difflab/specific-plan/PLAN.md', '# Plan\nReview ignored content\n')
  await runtime.write('.difflab/specific-plan/SECOND.md', '# Second\nAnother review file\n')
  return plans
}

export async function prepareIgnoredDirectory(editor: Editor, runtime: TestHarness) {
  const plans = await prepareIgnoredWorkspace(runtime)
  await editor.command(`Lgtm directory ${plans}`)
  await editor.loaded()
  return plans
}

export async function prepareIgnoredDiscussion(editor: Editor, runtime: TestHarness) {
  const plans = await prepareIgnoredDirectory(editor, runtime)
  await editor.pane('new', 2)
  await editor.keys(' rc')
  await editor.compose('Directory line review')
  await editor.command('Lgtm comment file')
  await editor.compose('Directory file review')
  await editor.command('Lgtm comment general')
  await editor.compose('Directory general review')
  await editor.comments(3)
  return plans
}
