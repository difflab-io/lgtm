import { describe, expect } from 'vitest'
import { join } from 'node:path'
import { profiles, suite } from './fixtures.js'
import { hash, step, waitFor, type Json } from '../common/runtime.js'
import type { Editor } from '../common/editor.js'

// fixtures --------------------------------------------------------------------

async function placement(e: Editor, id: string) {
  return e.lua(
    `local id=...; local s=require("lgtm.context").current
    for _,t in ipairs(s.conversation.threads) do if t.id==id then return require("lgtm.threads").placement(s,t,s.file.path,"new") end end`,
    id,
  )
}

// tests -----------------------------------------------------------------------

for (const profile of profiles) {
  const it = suite(profile)
  describe(`Neovim · source preservation · ${profile}`, () => {
    it('carries staged feedback through commits, line shifts, renames, deletion, restoration, filters and restart', async ({
      editor: e,
      runtime: r,
      github: gh,
    }) => {
      // Arrange ---------------------------------------------------------------
      await r.git('init', '-q', '-b', 'main')
      const path = 'src/source [literal] ü.txt',
        moved = 'moved/source [literal] ü.txt'
      const tail = [
        ...Array.from({ length: 10 }, (_, index) => `stable context ${index + 1}`),
        'footer',
      ]
      const originalBytes = 'header\noriginal\n' + tail.join('\n') + '\n'
      const stagedBytes = 'header\r\nselected\r\n' + tail.join('\r\n')
      const shifted = 'intro\nheader\nselected\n' + tail.join('\n') + '\n'

      await r.write(path, originalBytes)
      await r.commit('base')
      await r.git('checkout', '-qb', 'feature')
      await r.write(path, stagedBytes)
      await r.git('add', '--', path)
      const oid = await r.git('rev-parse', `:${path}`)
      await r.write(path, shifted)
      let rootId: string, replyId: string, original: Json, workingId: string, reviewId: string

      await step(
        'Given CRLF index bytes and a different working copy, lowercase c captures stage zero exactly',
        async () => {
          // Act ---------------------------------------------------------------
          await e.openLocal('staged')
          await e.show(path)
          await e.pane('new', 2)
          await e.keys('c')
          await e.compose('Preserve this staged selection')

          // Arrange -----------------------------------------------------------
          let state = (await e.comments(1))!
          rootId = state.conversation.comments[0].threadId
          original = state.conversation.threads[0].anchor

          // Assert ------------------------------------------------------------
          expect(original.source.indexFile).toMatchObject({
            blobOid: oid,
            contentHash: hash(stagedBytes),
          })
          expect(original.evidence.selectedText).toBe('selected')

          // Act ---------------------------------------------------------------
          await e.popup()
          await e.keys('c')
          await e.compose('Keep this reply with the original discussion')
          state = (await e.comments(2))!
          replyId = state.conversation.comments.find((c: Json) => Number(c.position) === 1).id

          // Assert ------------------------------------------------------------
          expect(replyId).toBeTruthy()

          // Act ---------------------------------------------------------------
          await e.command('Lgtm complete')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.review.state=="completed"',
            'staged review complete',
          )

          // Act ---------------------------------------------------------------
          await r.git(
            '-c',
            'user.name=E2E',
            '-c',
            'user.email=e2e@example.test',
            'commit',
            '-qm',
            'reviewed index',
          )
        },
      )

      await step(
        'When the staged source is committed and later shifts, the branch view keeps IDs and immutable index evidence',
        async () => {
          // Act ---------------------------------------------------------------
          await e.openLocal('branch')
          await e.show(path)

          // Assert ------------------------------------------------------------
          expect((await placement(e, rootId)).location.start).toBe(2)
          let state = (await e.state())!
          expect(state.conversation.threads.find((t: Json) => t.id === rootId).anchor).toEqual(
            original,
          )
          expect(state.conversation.comments.map((c: Json) => c.id)).toContain(replyId)

          // Act ---------------------------------------------------------------
          await r.commit('insert leading line')

          await e.openLocal('branch', true)
          await e.show(path)

          // Assert ------------------------------------------------------------
          expect(await placement(e, rootId)).toMatchObject({
            status: 'relocated',
            location: { start: 3 },
          })

          // Act ---------------------------------------------------------------
          await e.pane('new', 3)
          await e.popup()

          // Assert ------------------------------------------------------------
          expect(await e.text()).toContain('Preserve this staged selection')

          // Act ---------------------------------------------------------------
          await e.keys('q')

          // Assert ------------------------------------------------------------
          state = (await e.state())!

          expect(state.conversation.comments).toHaveLength(2)
        },
      )

      await step(
        'Then a confirmed directory rename moves one discussion without copying it or rewriting its origin',
        async () => {
          // Act ---------------------------------------------------------------
          await r.git('mv', 'src', 'moved')
          await r.commit('move reviewed directory')
          await e.openLocal('branch', true)
          await e.show(moved)

          // Assert ------------------------------------------------------------
          await waitFor(
            'confirmed rename persisted',
            () => e.state(),
            (s) =>
              s?.conversation.threads.find((t: Json) => t.id === rootId)?.currentPath === moved,
          )
          const state = (await e.state())!
          expect((await placement(e, rootId)).location.start).toBe(3)
          expect(state.conversation.threads.find((t: Json) => t.id === rootId).anchor).toEqual(
            original,
          )
          expect(state.conversation.comments).toHaveLength(2)
        },
      )

      await step(
        'When selected text disappears, the card floats to the file header and its centered modal shows the lost original lines',
        async () => {
          // Act ---------------------------------------------------------------
          await r.write(moved, shifted.replace('selected', 'replacement'))
          await r.commit('replace reviewed selection')
          await e.openLocal('branch', true)
          await e.show(moved)

          // Arrange -----------------------------------------------------------
          const lost = await placement(e, rootId)

          // Assert ------------------------------------------------------------
          expect(lost.status).toBe('outdated')
          expect(lost.location).toBeUndefined()
          const marks = await e.marks()
          expect(marks).toHaveLength(1)
          expect(marks[0][1]).toBe(0)
          expect(marks[0][3].virt_lines_above).toBe(true)
          for (const text of ['Outdated', 'Original lines', 'selected'])
            expect(JSON.stringify(marks)).toContain(text)

          // Act ---------------------------------------------------------------
          await e.pane()
          const popup = await e.popup()

          // Assert ------------------------------------------------------------
          for (const text of ['OUTDATED', 'Original lines 2', 'selected'])
            expect(await e.text()).toContain(text)
          const bounds = await e.value('vim.api.nvim_win_get_config(...)', popup.win)

          expect(bounds.row).toBe(Math.floor((40 - bounds.height - 2) / 2))

          // Act ---------------------------------------------------------------
          await e.keys('q')
        },
      )

      await step(
        'Then deleted files retain orphaned discussions and matching restoration reconnects them to their current line',
        async () => {
          // Act ---------------------------------------------------------------
          await r.git('rm', '--', moved)
          await r.commit('delete reviewed file')
          await e.openLocal('branch', true)

          // Assert ------------------------------------------------------------
          await waitFor(
            'file deletion persisted',
            () => e.state(),
            (s) =>
              s?.conversation.threads.find((t: Json) => t.id === rootId)?.orphanReason ===
              'file_deleted',
          )

          // Act ---------------------------------------------------------------
          await e.command('Lgtm threads')

          // Assert ------------------------------------------------------------
          for (const text of ['ORPHANED', 'File was deleted', 'Preserve this staged selection'])
            expect(await e.text()).toContain(text)

          // Act ---------------------------------------------------------------
          await e.keys('q')
          await r.write(moved, shifted)
          await r.commit('restore matching source')
          await e.openLocal('branch', true)
          await e.show(moved)

          // Assert ------------------------------------------------------------
          await waitFor(
            'restored source reconnected',
            () => e.state(),
            (s) => !s?.conversation.threads.find((t: Json) => t.id === rootId)?.orphanReason,
          )
          expect((await placement(e, rootId)).location.start).toBe(3)
          expect(
            (await e.state())!.conversation.threads.find((t: Json) => t.id === rootId).anchor,
          ).toEqual(original)
        },
      )

      await step(
        'When a staged diff hides another file’s feedback, it remains in the context and export rather than becoming orphaned',
        async () => {
          // Act ---------------------------------------------------------------
          await r.write('other.txt', 'working selection\n')
          await r.git('add', 'other.txt')
          await e.openLocal('staged', true)
          await e.show('other.txt')

          // Assert ------------------------------------------------------------
          const state = (await e.state())!

          expect(state.conversation.comments).toHaveLength(2)
          expect(
            state.conversation.threads.find((t: Json) => t.id === rootId).orphanReason,
          ).toBeFalsy()
          expect(await e.marks()).toHaveLength(0)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm threads')

          // Assert ------------------------------------------------------------
          expect(await e.text()).toContain('Preserve this staged selection')
          expect(await e.text()).not.toContain('ORPHANED')

          // Act ---------------------------------------------------------------
          await e.keys('q')
          await e.command('Lgtm complete')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.review.state=="completed"',
            'staged view complete',
          )

          // Act ---------------------------------------------------------------
          await r.git('reset', '--mixed', 'HEAD')
        },
      )

      await step(
        'Then changed unstaged anchors become labeled orphans while exports and normal-file mappings remain intact',
        async () => {
          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
          await e.edit(join(r.workspace, 'other.txt'))
          const beforeMapping = await e.value('vim.fn.maparg("c","n",false,true).desc')
          await e.openLocal('working')
          await e.show('other.txt')
          await e.pane()
          await e.keys('c')
          await e.compose('Retain this unstaged feedback')
          let state = (await e.state())!

          // Arrange -----------------------------------------------------------
          const working = state.conversation.threads.find(
            (t: Json) => t.anchor.path === 'other.txt',
          )
          workingId = working.id

          // Assert ------------------------------------------------------------
          expect(working.anchor.source.workingFile).toBeDefined()

          // Act ---------------------------------------------------------------
          await r.write('other.txt', 'changed working selection\n')
          await e.command('Lgtm refresh')

          // Assert ------------------------------------------------------------
          await waitFor(
            'working source orphaned',
            () => e.state(),
            (s) =>
              Boolean(
                s?.render_ready &&
                s?.conversation.threads.find((t: Json) => t.id === workingId)?.orphanReason ===
                  'working_source_changed',
              ),
          )
          expect((await placement(e, workingId)).status).toBe('orphaned')

          state = (await e.state())!

          // Arrange -----------------------------------------------------------
          const before = state.conversation

          // Act ---------------------------------------------------------------
          await e.choose('Preview')
          await e.command('Lgtm export review')

          // Assert ------------------------------------------------------------
          await waitFor(
            'feedback export',
            () => e.text(),
            (text) => text.includes('Retain this unstaged feedback'),
          )
          const text = await e.text()
          expect(text).toContain('1. **[COMMENT]**')
          expect(text).toContain('Keep this reply')
          expect(text).not.toContain(rootId)
          expect(text).not.toContain(oid)

          // Act ---------------------------------------------------------------
          await e.keys('q')

          // Assert ------------------------------------------------------------
          expect((await e.state())!.conversation).toEqual(before)
          reviewId = state.review.id

          // Arrange -----------------------------------------------------------
          const workingBuffer = state.panes.new.buf

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')

          // Assert ------------------------------------------------------------
          expect(
            await e.lua(
              'return vim.api.nvim_buf_call(...,function() return vim.fn.maparg("c","n",false,true).desc end)',
              workingBuffer,
            ),
          ).toBe(beforeMapping)
          expect(gh.contentReads).toBe(0)
          expect(gh.requests).toHaveLength(0)
        },
      )

      await step(
        'And backend/editor restart preserves moved evidence, reply identity and unstaged orphans in the same review',
        async () => {
          // Act ---------------------------------------------------------------
          await e.close()
          await r.restart()
          await e.start()
          await e.lua(
            `local id=...; require("lgtm.rpc").call("GetReview",{reviewId=id},function(err,result)
          if err then _G.e2e.open_error=err else require("lgtm.review").open(require("lgtm.context").apply({},result.review)) end end)`,
            reviewId,
          )

          // Arrange -----------------------------------------------------------
          const state = await e.loaded(),
            movedThread = state.conversation.threads.find((t: Json) => t.id === rootId)

          // Assert ------------------------------------------------------------
          expect(movedThread).toMatchObject({ currentPath: moved })
          expect(movedThread.orphanReason).toBeFalsy()
          expect(movedThread.anchor).toEqual(original)
          expect(
            state.conversation.threads.find((t: Json) => t.id === workingId).orphanReason,
          ).toBe('working_source_changed')
          expect(state.conversation.comments.map((c: Json) => c.id)).toContain(replyId)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm threads')

          // Assert ------------------------------------------------------------
          for (const text of [
            'Preserve this staged selection',
            'Retain this unstaged feedback',
            'ORPHANED',
          ])
            expect(await e.text()).toContain(text)
        },
      )
    })
  })
}
