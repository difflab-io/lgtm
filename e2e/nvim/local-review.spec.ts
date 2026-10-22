/**
 * @module-tag e2e
 * @module-tag nvim
 * @module-tag local-review
 */
import { describe, it, expect } from 'vitest'
import { join } from 'node:path'
import { rename, mkdir, symlink } from 'node:fs/promises'
import { profiles, useEditor } from './fixtures.js'
import { hash, waitFor, type Json } from '../common/test-utils.js'
import type { Editor } from './editor.js'

import {
  prepareStagedFeedback,
  prepareCommittedFeedback,
  prepareShiftedFeedback,
  prepareRenamedFeedback,
  prepareLostSelection,
  prepareDeletedFile,
  prepareRestoredFile,
  prepareWorkingFeedback,
  prepareOrphanedWorkingFeedback,
  prepareIgnoredWorkspace,
  prepareIgnoredDirectory,
  prepareIgnoredDiscussion,
} from './local-review.fixtures.js'
async function placement(e: Editor, id: string) {
  return e.lua(
    `local id=...; local s=require("lgtm.context").current
    for _,t in ipairs(s.conversation.threads) do if t.id==id then return require("lgtm.threads").placement(s,t,s.file.path,"new") end end`,
    id,
  )
}

for (const profile of profiles) {
  describe(`When reviewing staged changes [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)

    it(
      'Comments capture exact index bytes and a reply remains attached to its original discussion',
      { tags: profile === 'required' ? ['smoke'] : [] },
      async () => {
        // Arrange
        const { editor: e, runtime: r } = fixture()

        // Act
        const { original, oid, stagedBytes, replyId } = await prepareStagedFeedback(e, r)

        // Assert
        expect(original.source.indexFile).toMatchObject({
          blobOid: oid,
          contentHash: hash(stagedBytes),
        })
        expect(original.evidence.selectedText).toBe('selected')
        expect(replyId).toBeTruthy()
      },
    )

    it('Completing the staged review and committing preserves the anchor and replies in a branch review', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()

      // Act
      const { rootId, original, replyId } = await prepareCommittedFeedback(e, r)
      const state = (await e.state())!

      // Assert
      expect((await placement(e, rootId)).location.start).toBe(2)
      expect(
        state.conversation.threads.find((thread: Json) => thread.id === rootId).anchor,
      ).toEqual(original)
      expect(state.conversation.comments.map((comment: Json) => comment.id)).toContain(replyId)
    })
  })

  describe(`When reviewed source files change [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)

    it('Inserting lines moves the card to the selected text without changing original evidence', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const { path, rootId, original } = await prepareCommittedFeedback(e, r)

      // Act
      await r.commit('insert leading line')
      await e.openLocal('branch', true)
      await e.show(path)
      const relocated = await placement(e, rootId)
      await e.pane('new', 3)
      await e.popup()

      // Assert
      expect(relocated).toMatchObject({ status: 'relocated', location: { start: 3 } })
      expect(
        (await e.state())!.conversation.threads.find((thread: Json) => thread.id === rootId).anchor,
      ).toEqual(original)
      expect(await e.text()).toContain('Preserve this staged selection')
    })

    it('Renaming the reviewed directory moves its cards and keeps replies attached', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const { moved, rootId, original } = await prepareShiftedFeedback(e, r)

      // Act
      await r.git('mv', 'src', 'moved')
      await r.commit('move reviewed directory')
      await e.openLocal('branch', true)
      await e.show(moved)
      await waitFor(
        'confirmed rename persisted',
        () => e.state(),
        (state) =>
          state?.conversation.threads.find((thread: Json) => thread.id === rootId)?.currentPath ===
          moved,
      )
      const state = (await e.state())!

      // Assert
      expect((await placement(e, rootId)).location.start).toBe(3)
      expect(
        state.conversation.threads.find((thread: Json) => thread.id === rootId).anchor,
      ).toEqual(original)
      expect(state.conversation.comments).toHaveLength(2)
    })

    it('Losing the selected text floats an outdated card above the file with original line evidence', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const { moved, shifted, rootId } = await prepareRenamedFeedback(e, r)

      // Act
      await r.write(moved, shifted.replace('selected', 'replacement'))
      await r.commit('replace reviewed selection')
      await e.openLocal('branch', true)
      await e.show(moved)
      const lost = await placement(e, rootId)
      const marks = await e.marks()
      await e.pane()
      const popup = await e.popup()
      const bounds = await e.value('vim.api.nvim_win_get_config(...)', popup.win)

      // Assert
      expect(lost.status).toBe('outdated')
      expect(lost.location).toBeUndefined()
      expect(marks).toHaveLength(1)
      expect(marks[0][1]).toBe(0)
      expect(marks[0][3].virt_lines_above).toBe(true)
      for (const text of ['Outdated', 'Original lines', 'selected'])
        expect(JSON.stringify(marks)).toContain(text)
      for (const text of ['OUTDATED', 'Original lines 2', 'selected'])
        expect(await e.text()).toContain(text)
      expect(bounds.row).toBe(Math.floor((40 - bounds.height - 2) / 2))
    })

    it('Deleting the file keeps orphaned discussions available in the thread list', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const { moved, rootId } = await prepareLostSelection(e, r)

      // Act
      await r.git('rm', '--', moved)
      await r.commit('delete reviewed file')
      await e.openLocal('branch', true)
      await waitFor(
        'file deletion persisted',
        () => e.state(),
        (state) =>
          state?.conversation.threads.find((thread: Json) => thread.id === rootId)?.orphanReason ===
          'file_deleted',
      )
      await e.command('Lgtm threads')

      // Assert
      for (const text of ['ORPHANED', 'File was deleted', 'Preserve this staged selection'])
        expect(await e.text()).toContain(text)
    })

    it('Restoring matching source reconnects the same discussion to its original selection', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const { moved, shifted, rootId, original } = await prepareDeletedFile(e, r)

      // Act
      await r.write(moved, shifted)
      await r.commit('restore matching source')
      await e.openLocal('branch', true)
      await e.show(moved)
      await waitFor(
        'source reconnected',
        () => e.state(),
        (state) =>
          !state?.conversation.threads.find((thread: Json) => thread.id === rootId)?.orphanReason,
      )

      // Assert
      expect((await placement(e, rootId)).location.start).toBe(3)
      expect(
        (await e.state())!.conversation.threads.find((thread: Json) => thread.id === rootId).anchor,
      ).toEqual(original)
    })

    it('Selecting a diff without the commented file hides its card without orphaning the discussion', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const { rootId } = await prepareRestoredFile(e, r)
      await r.write('other.txt', 'working selection\n')
      await r.git('add', 'other.txt')

      // Act
      await e.openLocal('staged', true)
      await e.show('other.txt')
      const state = (await e.state())!
      const marks = await e.marks()
      await e.command('Lgtm threads')

      // Assert
      expect(state.conversation.comments).toHaveLength(2)
      expect(
        state.conversation.threads.find((thread: Json) => thread.id === rootId).orphanReason,
      ).toBeFalsy()
      expect(marks).toHaveLength(0)
      expect(await e.text()).toContain('Preserve this staged selection')
      expect(await e.text()).not.toContain('ORPHANED')
    })
  })

  describe(`When unstaged source changes [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)

    it('Changing the working file orphans its unstaged comment instead of moving it without proof', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const { working, workingId } = await prepareWorkingFeedback(e, r)

      // Act
      await r.write('other.txt', 'changed working selection\n')
      await e.command('Lgtm refresh')
      await waitFor(
        'working source orphaned',
        () => e.state(),
        (state) =>
          Boolean(
            state?.render_ready &&
            state.conversation.threads.find((thread: Json) => thread.id === workingId)
              ?.orphanReason === 'working_source_changed',
          ),
      )

      // Assert
      expect(working.anchor.source.workingFile).toBeDefined()
      expect((await placement(e, workingId)).status).toBe('orphaned')
    })

    it('Export contains preserved replies and orphaned feedback without internal IDs or source hashes', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const { rootId, oid } = await prepareOrphanedWorkingFeedback(e, r)
      const before = (await e.state())!.conversation

      // Act
      await e.choose('Preview')
      await e.command('Lgtm export review')
      const text = await waitFor(
        'feedback export',
        () => e.text(),
        (text) => text.includes('Retain this unstaged feedback'),
      )
      await e.keys('q')

      // Assert
      expect(text).toContain('1. **[COMMENT]**')
      expect(text).toContain('Keep this reply')
      expect(text).not.toContain(rootId)
      expect(text).not.toContain(oid)
      expect((await e.state())!.conversation).toEqual(before)
    })

    it('Closing the review restores the working buffer’s original key mappings', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const { beforeMapping, workingBuffer } = await prepareWorkingFeedback(e, r)

      // Act
      await e.command('Lgtm close')
      const restored = await e.lua(
        'return vim.api.nvim_buf_call(...,function() return vim.fn.maparg("c","n",false,true).desc end)',
        workingBuffer,
      )

      // Assert
      expect(restored).toBe(beforeMapping)
    })

    it('Resuming after restart preserves renamed anchors, replies and unstaged orphan states', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const { moved, rootId, original, workingId, replyId } = await prepareOrphanedWorkingFeedback(
        e,
        r,
      )
      const reviewId = (await e.state())!.review.id
      await e.command('Lgtm close')
      await e.close()
      await r.restart()
      await e.start()

      // Act
      await e.command('Lgtm sessions')
      await e.wait(
        'require("lgtm.picker").last and not require("lgtm.picker").last.closed',
        'saved review picker',
      )
      await e.keys('<CR>')
      const state = await e.loaded()
      await e.command('Lgtm threads')

      // Assert
      expect(state.review.id).toBe(reviewId)
      expect(state.conversation.threads.find((thread: Json) => thread.id === rootId)).toMatchObject(
        { currentPath: moved, anchor: original },
      )
      expect(
        state.conversation.threads.find((thread: Json) => thread.id === rootId).orphanReason,
      ).toBeFalsy()
      expect(
        state.conversation.threads.find((thread: Json) => thread.id === workingId).orphanReason,
      ).toBe('working_source_changed')
      expect(state.conversation.comments.map((comment: Json) => comment.id)).toContain(replyId)
      for (const text of [
        'Preserve this staged selection',
        'Retain this unstaged feedback',
        'ORPHANED',
      ])
        expect(await e.text()).toContain(text)
    })
  })
  describe(`When navigating an ignored directory review [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)
    it('The explorer navigates files and invalidates changed-file checkmarks', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      await r.git('init', '-q', '-b', 'main')
      await r.write('.gitignore', '.difflab\n')
      await r.commit('base')
      await mkdir(join(r.directory, 'plans'))
      await symlink(join(r.directory, 'plans'), join(r.workspace, '.difflab'), 'dir')
      const plans = join(r.workspace, '.difflab/specific-plan')
      await r.write('.gitignore', '.difflab\n')
      await r.write('.difflab/specific-plan/PLAN.md', '# Plan\nReview ignored content\n')
      await r.write('.difflab/specific-plan/SECOND.md', '# Second\nAnother review file\n')

      // Assert
      expect(await r.git('check-ignore', '.difflab')).toBe('.difflab')

      // Act
      await e.command(`Lgtm directory ${plans}`)
      const directoryReview = await e.loaded()

      // Assert
      expect(directoryReview.file.path).toBe('PLAN.md')
      expect(await e.text(directoryReview.panes.new.buf)).toContain('# Plan')
      expect(
        await e.value('vim.api.nvim_win_get_width(require("lgtm.context").current.explorer.win)'),
      ).toBe(30)

      // Act
      await e.pane()
      await e.keys('f')

      // Assert
      expect(await e.value('require("lgtm.context").current.explorer.win')).toBeNull()

      // Act
      await e.keys('f')
      await e.lua('vim.api.nvim_win_set_cursor(require("lgtm.context").current.explorer.win,{2,0})')
      await e.keys('<CR>')

      // Assert
      await e.wait(
        'require("lgtm.context").current.file.path == "SECOND.md"',
        'selected second file',
      )

      // Act
      await e.keys('[f')

      // Assert
      await e.wait('require("lgtm.context").current.file.path == "PLAN.md"', 'previous file')

      // Act
      await e.keys(' rm')

      // Assert
      expect((await e.state())!.reviewed['PLAN.md']).toBeTruthy()

      // Act
      await e.keys(' rm')

      // Assert
      expect((await e.state())!.reviewed['PLAN.md']).toBeUndefined()

      // Act
      await e.keys(' rm')
      await e.lua(
        'vim.api.nvim_set_current_win(require("lgtm.context").current.explorer.win); vim.api.nvim_win_set_cursor(0,{2,0})',
      )
      await e.keys('m')

      // Assert
      expect((await e.state())!.reviewed['SECOND.md']).toBeTruthy()
      expect(await e.value('vim.api.nvim_win_get_cursor(0)[1]')).toBe(2)

      // Act
      await e.keys('m')

      // Assert
      expect((await e.state())!.reviewed['SECOND.md']).toBeUndefined()

      // Act
      await e.pane()
      await e.keys('<Tab>')

      // Assert
      await e.wait('require("lgtm.context").current.file.path == "SECOND.md"', 'next unreviewed')

      // Act
      await e.keys(']f')

      // Assert
      await e.wait('require("lgtm.context").current.file.path == "PLAN.md"', 'all-file navigation')

      // Act
      await r.write('.difflab/specific-plan/PLAN.md', '# Plan\nReview ignored content changed\n')
      await e.command('Lgtm refresh')

      // Assert
      await e.wait(
        'require("lgtm.context").current.render_ready and not require("lgtm.context").current.reviewed["PLAN.md"]',
        'changed mark invalidated',
      )
    })
  })
  describe(`When commenting on ignored files [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)
    it('Inline feedback has an expandable card and opens a modal that returns focus to the diff', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      await prepareIgnoredDirectory(e, r)
      // Act
      await e.pane('new', 2)
      await e.keys(' rc')
      await e.compose('Directory line review')

      // Assert
      await e.comments(1)
      const state = (await e.state())!
      expect(state.conversation.threads[0].anchor).toMatchObject({
        path: 'PLAN.md',
        side: 'new',
      })
      expect(await e.marks()).toHaveLength(1)

      // Act
      await e.keys('za')

      // Assert
      expect((await e.marks())[0][3].virt_lines.length).toBeGreaterThan(0)

      // Act
      await e.keys('za')
      await e.popup()
      await e.keys('q')

      // Assert
      expect(await e.value('vim.api.nvim_get_current_win()')).toBe(state.panes.new.win)
    })
    it('File-level and general feedback appear in the thread list', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      await prepareIgnoredDirectory(e, r)
      await e.pane('new', 2)
      await e.keys(' rc')
      await e.compose('Directory line review')
      // Act
      await e.command('Lgtm comment file')
      await e.compose('Directory file review')
      await e.command('Lgtm comment general')
      await e.compose('Directory general review')

      // Assert
      await e.comments(3)

      // Act
      await e.command('Lgtm threads')

      // Assert
      expect((await e.text()).split('\n').length).toBeGreaterThan(5)
    })
    it('Saving a local draft shows confirmation without submitting it remotely', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      await prepareIgnoredDiscussion(e, r)
      // Act
      await e.command('Lgtm save')

      // Assert
      await waitFor(
        'save notification',
        () => e.notices(),
        (notices) => notices.some((n) => n.message.includes('Review saved')),
      )
    })
  })
  describe(`When resuming a completed directory review [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)
    it('The session picker restores the completed discussion and export does not create another review', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const plans = await prepareIgnoredDiscussion(e, r)
      // Act
      await e.command('Lgtm complete')

      // Assert
      await e.wait(
        'require("lgtm.context").current.review.state == "completed"',
        'local completion',
      )
      const directoryReview = (await e.state())!

      // Act
      await e.command('Lgtm close')

      // Assert
      expect(await e.state()).toBeNull()

      // Act
      await e.close()
      await r.restart()
      await e.start()
      // Assert
      const reviews = await r.json(['sessions', 'list', '--all'])
      expect(
        reviews.reviews.some(
          (v: Json) => v.id === directoryReview.review.id && v.state === 'completed',
        ),
      ).toBe(true)

      // Act
      await e.command('Lgtm sessions completed')

      // Assert
      await e.wait(
        'require("lgtm.picker").last and not require("lgtm.picker").last.closed',
        'session picker',
      )

      // Act
      await e.keys('specific-plan')

      // Assert
      await e.wait('#require("lgtm.picker").last.filtered == 1', 'session filter')
      expect(await e.value('vim.api.nvim_win_get_width(require("lgtm.picker").last.win)')).toBe(76)

      // Act
      await e.keys('<CR>')

      // Arrange
      const resumed = await e.loaded()

      // Assert
      expect(resumed.review.id).toBe(directoryReview.review.id)
      expect(resumed.conversation.comments).toHaveLength(3)
      const before = (await r.json(['sessions', 'list', '--all'])).reviews.length

      // Act
      await e.command('tabnew')
      await e.edit(join(plans, 'PLAN.md'))
      await e.choose('Preview')
      await e.command('Lgtm export review')

      // Assert
      await waitFor(
        'completed export',
        () => e.text(),
        (text) => text.includes('Directory line review'),
      )
      for (const text of [
        'Directory line review',
        'Directory file review',
        'Directory general review',
      ])
        expect(await e.text()).toContain(text)
      expect((await r.json(['sessions', 'list', '--all'])).reviews).toHaveLength(before)

      // Act
      await e.command('tabclose')
      await e.command('Lgtm close')
    })
  })
  describe(`When an ignored review directory moves [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)
    it('Confirmed root moves preserve discussions and remain usable after restart', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      await prepareIgnoredWorkspace(r)
      const original = join(r.directory, 'plans/move-case')
      await r.write('.difflab/move-case/PLAN.md', 'before\nselected\nafter\n')
      const last = join(r.directory, 'plans/final-plan')

      // Act
      await e.command(`Lgtm directory ${original}`)

      // Assert
      await e.loaded()

      // Act
      await e.show('PLAN.md')
      await e.pane('new', 2)
      await e.keys('c')
      await e.compose('Review before root move')
      let movedState = (await e.state())!
      const reviewId = movedState.review.id
      const commentId = movedState.conversation.comments[0].id

      // Arrange
      const threadId = movedState.conversation.comments[0].threadId
      let from = original

      // Assert
      for (const [index, to] of [join(r.directory, 'plans/moved-plan'), last].entries()) {
        // Act
        await rename(from, to)

        // Assert
        await e.lua(
          'local from,to,pattern=...; vim.api.nvim_exec_autocmds("User",{pattern=pattern,data={from=from,to=to}})',
          from,
          to,
          index === 0 ? 'MiniFilesActionRename' : 'LgtmPathRenamed',
        )
        await waitFor(
          'review root moved',
          () => e.state(),
          (s) => Boolean(s?.root === to && !s.moving && s.render_ready),
        )
        movedState = (await e.state())!
        expect(movedState.conversation.threads.find((t: Json) => t.id === threadId)).toMatchObject({
          sourcePath: join(original, 'PLAN.md'),
          currentPath: join(to, 'PLAN.md'),
          anchor: { path: 'PLAN.md' },
        })
        expect(JSON.stringify(await e.marks())).toContain('Review before root move')
        if (index === 0) {
          // Act
          await e.pane('new', 3)
          await e.keys('c')
          await e.compose('Review after first root move')
        }
        from = to
      }

      // Assert
      expect((await e.state())!.conversation.comments).toHaveLength(2)
      const movedBefore = (await e.state())!.conversation

      // Act
      await e.choose('Preview')
      await e.command('Lgtm export review')

      // Assert
      await waitFor(
        'moved export',
        () => e.text(),
        (text) => text.includes('Review after first root move'),
      )
      expect(await e.text()).toContain('Review before root move')

      // Act
      await e.keys('q')

      // Assert
      expect((await e.state())!.conversation).toEqual(movedBefore)

      // Act
      await e.command('Lgtm close')
      await e.close()
      await r.restart()
      await e.start()
      await e.command(`Lgtm directory ${last}`)

      // Arrange
      const restartedState = await e.loaded()

      // Assert
      expect(restartedState.review.id).toBe(reviewId)
      expect(restartedState.root).toBe(last)
      expect(restartedState.conversation.comments).toHaveLength(2)
      expect(restartedState.conversation.comments.map((c: Json) => c.id)).toContain(commentId)
    })
  })
}
