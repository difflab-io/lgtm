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
  prepareWorkingFeedback,
  prepareIgnoredDirectory,
  prepareIgnoredDiscussion,
} from './local-review.fixtures.js'
async function placement(e: Editor, id: string) {
  return e.lua(
    `local id=...; local s=require("lgtm.services.state").current
    for _,t in ipairs(s.conversation.threads) do if t.id==id then return require("lgtm.features.annotation_overlay").placement(s,t,s.file.path,"new") end end`,
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
        const { original, stagedBytes, replyId } = await prepareStagedFeedback(e, r)

        // Assert
        expect(original.source.commit).toMatchObject({
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
      expect(await placement(e, rootId)).toBeNull()
      expect(
        state.conversation.threads.find((thread: Json) => thread.id === rootId).anchor,
      ).toEqual(original)
      expect(state.conversation.comments.map((comment: Json) => comment.id)).toContain(replyId)
    })
  })

  describe(`When reviewing saved source [${profile}]`, { tags: [profile, 'local-review'] }, () => {
    const fixture = useEditor(profile)

    it('Changing or deleting working files leaves the open staged source and comment locations fixed', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const { path, rootId, original, stagedBytes } = await prepareStagedFeedback(e, r)
      const initial = (await e.state())!

      // Act
      await r.write(path, 'replacement\n')
      await r.git('rm', '-f', '--cached', '--', path)
      await e.pane('new', 2)
      const fixed = await placement(e, rootId)

      // Assert
      expect(fixed).toMatchObject({ status: 'exact', location: { start: 2 } })
      expect((await e.state())!.snapshot.id).toBe(initial.snapshot.id)
      expect(
        (await e.state())!.conversation.threads.find((thread: Json) => thread.id === rootId).anchor,
      ).toEqual(original)
      expect((await e.text(initial.panes.new.buf)).replaceAll('\r', '')).toBe(
        stagedBytes.replaceAll('\r', ''),
      )
      expect(await e.value('vim.bo[...].modifiable', initial.panes.new.buf)).toBe(false)
    })

    it('Refresh adds a revision to the same working session and keeps its earlier discussion', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const { workingId } = await prepareWorkingFeedback(e, r)
      const first = (await e.state())!

      // Act
      await r.write('other.txt', 'replacement saved source\n')
      await e.command('Lgtm refresh')
      await e.wait(
        'require("lgtm.services.state").current.render_ready and require("lgtm.services.state").current.snapshot.id ~= ' +
          JSON.stringify(first.snapshot.id),
        'new saved revision ready',
      )
      const next = (await e.state())!
      await e.command('Lgtm threads')

      // Assert
      expect(next.review.id).toBe(first.review.id)
      expect(next.review.revisions).toHaveLength(2)
      expect(
        next.conversation.threads.find((thread: Json) => thread.id === workingId).anchor.snapshotId,
      ).toBe(first.snapshot.id)
      expect(await e.text()).toContain('Retain this unstaged feedback')
      expect(await e.text()).toContain('SAVED REVISION')
    })

    it('A restart resumes the same saved source even after the original directory moves', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const plans = await prepareIgnoredDiscussion(e, r)
      const initial = (await e.state())!
      await e.close()
      await rename(plans, join(r.directory, 'moved-plan'))
      await r.restart()
      await e.start()

      // Act
      await e.command('Lgtm sessions')
      await e.wait('require("lgtm.ui.search").last and vim.fn.mode() == "i"', 'saved review picker')
      await e.keys('<CR>')
      const resumed = await e.loaded()

      // Assert
      expect(resumed.review.id).toBe(initial.review.id)
      expect(resumed.snapshot).toEqual(initial.snapshot)
      expect(resumed.conversation.comments).toHaveLength(3)
      expect(await e.text(resumed.panes.new.buf)).toContain('Review ignored content')
    })

    it('Closing a snapshot view preserves the original editing buffer and its mappings', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const { beforeMapping, originalBuffer } = await prepareWorkingFeedback(e, r)

      // Act
      await e.command('Lgtm close')
      const restored = await e.lua(
        'return vim.api.nvim_buf_call(...,function() return vim.fn.maparg("c","n",false,true).desc end)',
        originalBuffer,
      )

      // Assert
      expect(restored).toBe(beforeMapping)
      expect(await e.value('vim.bo[...].modifiable', originalBuffer)).toBe(true)
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
        await e.value(
          'vim.api.nvim_win_get_width(require("lgtm.services.state").current.explorer.win)',
        ),
      ).toBe(30)

      // Act
      await e.pane()
      await e.keys('f')

      // Assert
      await e.wait('require("lgtm.services.state").current.explorer.win == nil', 'explorer hidden')
      expect(await e.value('require("lgtm.services.state").current.explorer.win')).toBeNull()

      // Act
      await e.keys('f')
      await e.wait('require("lgtm.services.state").current.explorer.win ~= nil', 'explorer shown')
      await e.lua(
        'vim.api.nvim_win_set_cursor(require("lgtm.services.state").current.explorer.win,{2,0})',
      )
      await e.keys('<CR>')

      // Assert
      await e.wait(
        'require("lgtm.services.state").current.file.path == "SECOND.md"',
        'selected second file',
      )

      // Act
      await e.keys('[f')

      // Assert
      await e.wait('require("lgtm.services.state").current.file.path == "PLAN.md"', 'previous file')

      // Act
      await e.keys(' rm')

      // Assert
      await e.wait(
        'require("lgtm.services.state").current.reviewed["PLAN.md"] ~= nil',
        'PLAN.md marked reviewed',
      )
      expect((await e.state())!.reviewed['PLAN.md']).toBeTruthy()

      // Act
      await e.keys(' rm')

      // Assert
      await e.wait(
        'require("lgtm.services.state").current.reviewed["PLAN.md"] == nil',
        'PLAN.md mark removed',
      )
      expect((await e.state())!.reviewed['PLAN.md']).toBeUndefined()

      // Act
      await e.keys(' rm')
      await e.wait(
        'require("lgtm.services.state").current.reviewed["PLAN.md"] ~= nil',
        'PLAN.md marked before explorer navigation',
      )
      await e.lua(
        'vim.api.nvim_set_current_win(require("lgtm.services.state").current.explorer.win); vim.api.nvim_win_set_cursor(0,{2,0})',
      )
      await e.keys('m')

      // Assert
      await e.wait(
        'require("lgtm.services.state").current.reviewed["SECOND.md"] ~= nil',
        'SECOND.md marked reviewed',
      )
      expect((await e.state())!.reviewed['SECOND.md']).toBeTruthy()
      const explorerLines = await e.value<string[]>(
        'vim.api.nvim_buf_get_lines(require("lgtm.services.state").current.explorer.buf, 0, -1, false)',
      )
      expect(explorerLines[1]).toMatch(/✓ .*SECOND\.md/)
      expect(await e.value('vim.api.nvim_win_get_cursor(0)[1]')).toBe(2)

      // Act
      await e.keys('m')

      // Assert
      await e.wait(
        'require("lgtm.services.state").current.reviewed["SECOND.md"] == nil',
        'SECOND.md mark removed',
      )
      expect((await e.state())!.reviewed['SECOND.md']).toBeUndefined()

      // Act
      await e.pane()
      await e.keys('<Tab>')

      // Assert
      await e.wait(
        'require("lgtm.services.state").current.file.path == "SECOND.md"',
        'next unreviewed',
      )

      // Act
      await e.keys(']f')

      // Assert
      await e.wait(
        'require("lgtm.services.state").current.file.path == "PLAN.md"',
        'all-file navigation',
      )

      // Act
      await r.write('.difflab/specific-plan/PLAN.md', '# Plan\nReview ignored content changed\n')
      await e.command('Lgtm refresh')

      // Assert
      await e.wait(
        'require("lgtm.services.state").current.render_ready and not require("lgtm.services.state").current.reviewed["PLAN.md"]',
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
        'require("lgtm.services.state").current.review.state == "completed"',
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
      const reviews = await r.json(['session', 'list', '--all'])
      expect(
        reviews.reviews.some(
          (v: Json) => v.id === directoryReview.review.id && v.state === 'completed',
        ),
      ).toBe(true)

      // Act
      await e.command('Lgtm sessions completed')

      // Assert
      await e.wait(
        'require("lgtm.ui.search").last and not require("lgtm.ui.search").last.closed and vim.api.nvim_get_current_buf() == require("lgtm.ui.search").last.buf and vim.fn.mode() == "i"',
        'session picker',
      )

      // Act
      await e.keys('specific-plan')

      // Assert
      await e.wait('#require("lgtm.ui.search").last.filtered == 1', 'session filter')
      expect(await e.value('require("lgtm.ui.search").last.closed')).toBe(false)

      // Act
      await e.keys('<CR>')

      // Arrange
      const resumed = await e.loaded()

      // Assert
      expect(resumed.review.id).toBe(directoryReview.review.id)
      expect(resumed.conversation.comments).toHaveLength(3)
      const before = (await r.json(['session', 'list', '--all'])).reviews.length

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
      expect((await r.json(['session', 'list', '--all'])).reviews).toHaveLength(before)

      // Act
      await e.command('tabclose')
      await e.command('Lgtm close')
    })
  })
}
