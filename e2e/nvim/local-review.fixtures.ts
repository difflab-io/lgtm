import type { Editor } from './editor.js'
import { type TestHarness } from '../common/test-harness.js'
import { waitFor, type Json } from '../common/test-utils.js'
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
    'require("lgtm.context").current.review.state == "completed"',
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

export async function prepareShiftedFeedback(editor: Editor, runtime: TestHarness) {
  const feedback = await prepareCommittedFeedback(editor, runtime)
  await runtime.commit('insert leading line')
  await editor.openLocal('branch', true)
  await editor.show(feedback.path)
  return feedback
}

export async function prepareRenamedFeedback(editor: Editor, runtime: TestHarness) {
  const feedback = await prepareShiftedFeedback(editor, runtime)
  await runtime.git('mv', 'src', 'moved')
  await runtime.commit('move reviewed directory')
  await editor.openLocal('branch', true)
  await editor.show(feedback.moved)
  await waitFor(
    'confirmed rename persisted',
    () => editor.state(),
    (state) =>
      state?.conversation.threads.find((thread: Json) => thread.id === feedback.rootId)
        ?.currentPath === feedback.moved,
  )
  return feedback
}

export async function prepareLostSelection(editor: Editor, runtime: TestHarness) {
  const feedback = await prepareRenamedFeedback(editor, runtime)
  await runtime.write(feedback.moved, feedback.shifted.replace('selected', 'replacement'))
  await runtime.commit('replace reviewed selection')
  await editor.openLocal('branch', true)
  await editor.show(feedback.moved)
  return feedback
}

export async function prepareDeletedFile(editor: Editor, runtime: TestHarness) {
  const feedback = await prepareRenamedFeedback(editor, runtime)
  await runtime.git('rm', '--', feedback.moved)
  await runtime.commit('delete reviewed file')
  await editor.openLocal('branch', true)
  await waitFor(
    'file deletion persisted',
    () => editor.state(),
    (state) =>
      state?.conversation.threads.find((thread: Json) => thread.id === feedback.rootId)
        ?.orphanReason === 'file_deleted',
  )
  return feedback
}

export async function prepareRestoredFile(editor: Editor, runtime: TestHarness) {
  const feedback = await prepareDeletedFile(editor, runtime)
  await runtime.write(feedback.moved, feedback.shifted)
  await runtime.commit('restore matching source')
  await editor.openLocal('branch', true)
  await editor.show(feedback.moved)
  return feedback
}

export async function prepareWorkingFeedback(editor: Editor, runtime: TestHarness) {
  const feedback = await prepareRenamedFeedback(editor, runtime)
  await editor.command('Lgtm complete')
  await editor.wait(
    'require("lgtm.context").current.review.state == "completed"',
    'branch review completed before starting a working review',
  )
  await runtime.write('other.txt', 'working selection\n')
  await editor.command('Lgtm close')
  await editor.edit(join(runtime.workspace, 'other.txt'))
  const beforeMapping = await editor.value('vim.fn.maparg("c","n",false,true).desc')
  await editor.openLocal('working')
  await editor.show('other.txt')
  await editor.pane()
  await editor.keys('c')
  await editor.compose('Retain this unstaged feedback')
  const state = (await editor.state())!
  const working = state.conversation.threads.find(
    (thread: Json) => thread.anchor.path === 'other.txt',
  )
  return {
    ...feedback,
    workingId: working.id,
    working,
    beforeMapping,
    workingBuffer: state.panes.new.buf,
  }
}

export async function prepareOrphanedWorkingFeedback(editor: Editor, runtime: TestHarness) {
  const feedback = await prepareWorkingFeedback(editor, runtime)
  await runtime.write('other.txt', 'changed working selection\n')
  await editor.command('Lgtm refresh')
  await waitFor(
    'working source orphaned',
    () => editor.state(),
    (state) =>
      Boolean(
        state?.render_ready &&
        state?.conversation.threads.find((thread: Json) => thread.id === feedback.workingId)
          ?.orphanReason === 'working_source_changed',
      ),
  )
  return feedback
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
