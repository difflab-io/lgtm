import { describe, expect } from 'vitest'
import { join } from 'node:path'
import { readFile, rename, copyFile, mkdir, symlink } from 'node:fs/promises'
import { profiles, suite } from './fixtures.js'
import { step, waitFor, type Json } from '../common/runtime.js'

// tests -----------------------------------------------------------------------

for (const profile of profiles) {
  const it = suite(profile)
  describe(`Neovim · notes and ignored plans · ${profile}`, () => {
    it('takes notes while browsing, manages nested discussions, reviews ignored plans and resumes completed feedback', async ({
      editor: e,
      runtime: r,
    }) => {
      // Arrange ---------------------------------------------------------------
      await r.git('init', '-q', '-b', 'main')

      await r.git('remote', 'add', 'origin', 'git@github.com:acme/repo.git')
      const file = await r.write('notes.txt', 'one\ntwo\nthree\n')
      await r.write('.gitignore', '.difflab\n')
      await r.commit('base')
      await r.write('notes.txt', 'one\nchanged\nthree\n')
      await mkdir(join(r.directory, 'plans'))
      await symlink(join(r.directory, 'plans'), join(r.workspace, '.difflab'), 'dir')
      const plans = join(r.workspace, '.difflab/specific-plan')
      await r.write('.gitignore', '.difflab\n')
      await r.write('.difflab/specific-plan/PLAN.md', '# Plan\nReview ignored content\n')
      await r.write('.difflab/specific-plan/SECOND.md', '# Second\nAnother review file\n')

      await step(
        'Given an ordinary file, a range note appears as a collapsed card and Enter opens a centered thread',
        async () => {
          // Act ---------------------------------------------------------------
          await e.edit(file)
          await e.command('1,2Lgtm note')
          const composer = await e.composer()

          // Assert ------------------------------------------------------------
          const config = await e.lua(
            'local c=vim.api.nvim_win_get_config(...); return {width=c.width,height=c.height,row=c.row,col=c.col}',
            composer.win,
          )

          expect(config).toMatchObject({ width: 80, height: 12, row: 13, col: 19 })

          // Act ---------------------------------------------------------------
          await e.compose('Original standalone note')

          // Assert ------------------------------------------------------------
          const note = await e.value(
            'require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation',
          )

          expect(note.comments).toHaveLength(1)
          expect(note.threads[0].anchor).toMatchObject({ start: '1', end: '2' })
          expect(
            await e.value(
              'vim.api.nvim_buf_get_extmarks(0,require("lgtm.threads").namespace,0,-1,{})',
            ),
          ).toHaveLength(1)

          await e.lua('vim.api.nvim_win_set_cursor(0,{2,0})')

          // Act ---------------------------------------------------------------
          const popup = await e.popup()

          // Assert ------------------------------------------------------------
          const bounds = await e.lua(
            'local c=vim.api.nvim_win_get_config(...); return {width=c.width,height=c.height,row=c.row,col=c.col}',
            popup.win,
          )

          expect(bounds).toMatchObject({ width: 86, height: 5, row: 16, col: 16 })
        },
      )

      await step(
        'When replies are edited, resolved, reopened and deleted, the modal identifies the selected role and preserves the root',
        async () => {
          // Act ---------------------------------------------------------------
          await e.keys('c')
          await e.compose('Note reply')
          await e.popup()

          // Assert ------------------------------------------------------------
          const text = await e.text()

          expect(text).toContain('TOP-LEVEL COMMENT')
          expect(text).toContain('└─ REPLY 1')
          expect(text).not.toContain('notes.txt')

          // Act ---------------------------------------------------------------
          await e.keys('<Tab>')

          // Assert ------------------------------------------------------------
          expect(
            JSON.stringify(
              await e.value('vim.api.nvim_win_get_config(require("lgtm.threads").last.win).footer'),
            ),
          ).toContain('Reply 1')

          // Act ---------------------------------------------------------------
          await e.keys('e')

          // Assert ------------------------------------------------------------
          expect(
            JSON.stringify(
              await e.value('vim.api.nvim_win_get_config(require("lgtm.composer").last.win).title'),
            ),
          ).toContain('Edit reply 1')

          // Act ---------------------------------------------------------------
          await e.compose('Edited note reply')

          // Assert ------------------------------------------------------------
          let notes = await e.value(
            'require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation',
          )

          expect(notes.comments.map((c: Json) => c.body)).toEqual([
            'Original standalone note',
            'Edited note reply',
          ])

          // Act ---------------------------------------------------------------
          await e.keys(' re')
          await e.compose('Edited standalone note')
          await e.popup()
          await e.keys('x')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation.threads[1].state == "resolved"',
            'note resolved',
          )

          // Act ---------------------------------------------------------------
          await e.popup()
          await e.keys('x')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation.threads[1].state ~= "resolved"',
            'note reopened',
          )

          // Act ---------------------------------------------------------------
          await e.popup('Edited note reply')
          await e.choose('Delete')
          await e.keys('dd')

          // Assert ------------------------------------------------------------
          await e.wait(
            '#require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation.comments == 1',
            'selected reply deleted',
          )
          notes = await e.value(
            'require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation',
          )
          expect(notes.comments[0].body).toBe('Edited standalone note')

          // Act ---------------------------------------------------------------
          await e.choose('Preview')
          await e.command('Lgtm export notes')

          // Assert ------------------------------------------------------------
          await waitFor(
            'notes preview',
            () => e.text(),
            (text) => text.includes('Edited standalone note'),
          )

          // Act ---------------------------------------------------------------
          await e.command('close')
        },
      )

      let directoryReview: Json
      await step(
        'When an ignored directory is reviewed, its persistent file pane supports navigation, checkmarks and content invalidation',
        async () => {
          // Assert ------------------------------------------------------------
          expect(await r.git('check-ignore', '.difflab')).toBe('.difflab')

          // Act ---------------------------------------------------------------
          await e.command(`Lgtm directory ${plans}`)
          directoryReview = await e.loaded()

          // Assert ------------------------------------------------------------
          expect(directoryReview.file.path).toBe('PLAN.md')
          expect(await e.text(directoryReview.panes.new.buf)).toContain('# Plan')
          expect(
            await e.value(
              'vim.api.nvim_win_get_width(require("lgtm.context").current.explorer.win)',
            ),
          ).toBe(30)

          // Act ---------------------------------------------------------------
          await e.pane()
          await e.keys('f')

          // Assert ------------------------------------------------------------
          expect(await e.value('require("lgtm.context").current.explorer.win')).toBeNull()

          // Act ---------------------------------------------------------------
          await e.keys('f')
          await e.lua(
            'vim.api.nvim_win_set_cursor(require("lgtm.context").current.explorer.win,{2,0})',
          )
          await e.keys('<CR>')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.file.path == "SECOND.md"',
            'selected second file',
          )

          // Act ---------------------------------------------------------------
          await e.keys('[f')

          // Assert ------------------------------------------------------------
          await e.wait('require("lgtm.context").current.file.path == "PLAN.md"', 'previous file')

          // Act ---------------------------------------------------------------
          await e.keys(' rm')

          // Assert ------------------------------------------------------------
          expect((await e.state())!.reviewed['PLAN.md']).toBeTruthy()

          // Act ---------------------------------------------------------------
          await e.keys(' rm')

          // Assert ------------------------------------------------------------
          expect((await e.state())!.reviewed['PLAN.md']).toBeUndefined()

          // Act ---------------------------------------------------------------
          await e.keys(' rm')
          await e.lua(
            'vim.api.nvim_set_current_win(require("lgtm.context").current.explorer.win); vim.api.nvim_win_set_cursor(0,{2,0})',
          )
          await e.keys('m')

          // Assert ------------------------------------------------------------
          expect((await e.state())!.reviewed['SECOND.md']).toBeTruthy()
          expect(await e.value('vim.api.nvim_win_get_cursor(0)[1]')).toBe(2)

          // Act ---------------------------------------------------------------
          await e.keys('m')

          // Assert ------------------------------------------------------------
          expect((await e.state())!.reviewed['SECOND.md']).toBeUndefined()

          // Act ---------------------------------------------------------------
          await e.pane()
          await e.keys('<Tab>')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.file.path == "SECOND.md"',
            'next unreviewed',
          )

          // Act ---------------------------------------------------------------
          await e.keys(']f')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.file.path == "PLAN.md"',
            'all-file navigation',
          )

          // Act ---------------------------------------------------------------
          await r.write(
            '.difflab/specific-plan/PLAN.md',
            '# Plan\nReview ignored content changed\n',
          )
          await e.command('Lgtm refresh')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.render_ready and not require("lgtm.context").current.reviewed["PLAN.md"]',
            'changed mark invalidated',
          )
        },
      )

      await step(
        'Then line, file and review-level feedback use cards, expandable folds and a completion notification',
        async () => {
          // Act ---------------------------------------------------------------
          await e.pane('new', 2)
          await e.keys(' rc')
          await e.compose('Directory line review')

          // Assert ------------------------------------------------------------
          await e.comments(1)
          const state = (await e.state())!
          expect(state.conversation.threads[0].anchor).toMatchObject({
            path: 'PLAN.md',
            side: 'new',
          })
          expect(await e.marks()).toHaveLength(1)

          // Act ---------------------------------------------------------------
          await e.keys('za')

          // Assert ------------------------------------------------------------
          expect((await e.marks())[0][3].virt_lines.length).toBeGreaterThan(0)

          // Act ---------------------------------------------------------------
          await e.keys('za')
          await e.popup()
          await e.keys('q')

          // Assert ------------------------------------------------------------
          expect(await e.value('vim.api.nvim_get_current_win()')).toBe(state.panes.new.win)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm comment file')
          await e.compose('Directory file review')
          await e.command('Lgtm comment general')
          await e.compose('Directory general review')

          // Assert ------------------------------------------------------------
          await e.comments(3)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm threads')

          // Assert ------------------------------------------------------------
          expect((await e.text()).split('\n').length).toBeGreaterThan(5)

          // Act ---------------------------------------------------------------
          await e.keys('q')
          await e.command('Lgtm save')

          // Assert ------------------------------------------------------------
          await waitFor(
            'save notification',
            () => e.notices(),
            (notices) => notices.some((n) => n.message.includes('Review saved')),
          )

          // Act ---------------------------------------------------------------
          await e.command('Lgtm complete')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.review.state == "completed"',
            'local completion',
          )

          directoryReview = (await e.state())!

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')

          // Assert ------------------------------------------------------------
          expect(await e.state()).toBeNull()
        },
      )

      await step(
        'When the reader opens working changes, old-side comments and replies survive CodeDiff layout switches',
        async () => {
          // Act ---------------------------------------------------------------
          const state = await e.openLocal('working')

          // Assert ------------------------------------------------------------
          expect(state.base).toBeTruthy()

          // Act ---------------------------------------------------------------
          await e.pane('old', 2)
          await e.command('Lgtm comment line')
          await e.compose('Branch old-side feedback\n\n- Preserve **Markdown** and `code`.')

          // Assert ------------------------------------------------------------
          expect((await e.state())!.conversation.threads[0].anchor.side).toBe('old')

          // Act ---------------------------------------------------------------
          await e.popup()
          await e.keys('c')
          await e.compose('Old-side reply with a useful alternative')

          // Assert ------------------------------------------------------------
          await e.comments(2)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm layout')

          // Assert ------------------------------------------------------------
          await e.wait('require("lgtm.context").current.layout == "inline"', 'inline layout')

          // Act ---------------------------------------------------------------
          await e.command('Lgtm layout')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.layout == "side-by-side"',
            'side by side layout',
          )
        },
      )

      await step(
        'Then promotion, file export and clipboard export retain readable Markdown without mutating saved feedback',
        async () => {
          // Arrange -----------------------------------------------------------
          const reviewTab = (await e.state())!.tab

          // Act ---------------------------------------------------------------
          await e.command('tabnew')
          const notesTab = await e.value('vim.api.nvim_get_current_tabpage()')
          await e.edit(file)
          await e.choose(1, 'Promote')
          await e.command('Lgtm promote')

          // Assert ------------------------------------------------------------
          await e.comments(3)
          expect(await e.value('vim.api.nvim_get_current_tabpage()')).toBe(reviewTab)

          // Act ---------------------------------------------------------------
          await e.lua('vim.cmd(vim.api.nvim_tabpage_get_number(...).."tabclose")', notesTab)
          const before = (await e.state())!.conversation
          await e.choose('Save file')
          await e.command('Lgtm export review')

          // Assert ------------------------------------------------------------
          await waitFor(
            'export file',
            () => readFile(join(r.workspace, 'review.md'), 'utf8'),
            (text) => text.includes('Branch old-side feedback'),
          )

          await e.lua(
            `_G.e2e.copied=nil; vim.g.clipboard={name="E2E clipboard",copy={["+"]=function(lines) _G.e2e.copied=table.concat(lines,"\\n") end,["*"]=function() end},paste={["+"]=function() return {{},"v"} end,["*"]=function() return {{},"v"} end}}`,
          )

          // Act ---------------------------------------------------------------
          await e.choose('Copy to clipboard')
          await e.command('Lgtm export review')

          // Assert ------------------------------------------------------------
          await e.wait('_G.e2e.copied ~= nil', 'clipboard export')

          // Arrange -----------------------------------------------------------
          const markdown = await e.value<string>('_G.e2e.copied')

          // Assert ------------------------------------------------------------
          for (const text of [
            'Edited standalone note',
            '1. **[COMMENT]**',
            ':~2',
            'old (left) side',
            '- Preserve **Markdown** and `code`.',
            '**Reply',
            'useful alternative',
          ])
            expect(markdown).toContain(text)
          for (const comment of before.comments) expect(markdown).not.toContain(comment.id)
          expect((await e.state())!.conversation).toEqual(before)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
        },
      )

      await step(
        'And restarting editor and backend restores ordinary-file cards and searchable completed sessions without creating a successor',
        async () => {
          // Act ---------------------------------------------------------------
          await e.close()
          await r.restart()
          await e.start()
          await e.edit(file)

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.notes").states[vim.api.nvim_get_current_buf()] and require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation ~= nil',
            'persisted notes loaded',
          )
          expect(
            await e.value(
              'vim.api.nvim_buf_get_extmarks(0,require("lgtm.threads").namespace,0,-1,{})',
            ),
          ).toHaveLength(1)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm notes')

          // Assert ------------------------------------------------------------
          await e.wait('require("lgtm.threads").last ~= nil', 'persisted note modal')
          expect(await e.text()).toContain('Edited standalone note')

          // Act ---------------------------------------------------------------
          await e.keys('q')

          // Assert ------------------------------------------------------------
          const reviews = await r.json(['sessions', 'list', '--all'])

          expect(
            reviews.reviews.some(
              (v: Json) => v.id === directoryReview.review.id && v.state === 'completed',
            ),
          ).toBe(true)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm sessions completed')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.picker").last and not require("lgtm.picker").last.closed',
            'session picker',
          )

          // Act ---------------------------------------------------------------
          await e.keys('specific-plan')

          // Assert ------------------------------------------------------------
          await e.wait('#require("lgtm.picker").last.filtered == 1', 'session filter')
          expect(await e.value('vim.api.nvim_win_get_width(require("lgtm.picker").last.win)')).toBe(
            76,
          )

          // Act ---------------------------------------------------------------
          await e.keys('<CR>')

          // Arrange -----------------------------------------------------------
          const resumed = await e.loaded()

          // Assert ------------------------------------------------------------
          expect(resumed.review.id).toBe(directoryReview.review.id)
          expect(resumed.conversation.comments).toHaveLength(3)

          const before = (await r.json(['sessions', 'list', '--all'])).reviews.length

          // Act ---------------------------------------------------------------
          await e.command('tabnew')
          await e.edit(join(plans, 'PLAN.md'))
          await e.choose('Preview')
          await e.command('Lgtm export review')

          // Assert ------------------------------------------------------------
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
        },
      )
    })

    it('moves saved notes and ignored review roots without duplicating discussions, and retains them after restart', async ({
      editor: e,
      runtime: r,
    }) => {
      // Arrange ---------------------------------------------------------------
      const original = join(r.workspace, '.difflab/move-case'),
        notePath = await r.write('.difflab/move-case/NOTE.md', 'before\nselected\nafter\n')

      await r.write('.difflab/move-case/PLAN.md', 'before\nselected\nafter\n')
      const destination = join(r.workspace, '.difflab/moved-note.md')
      let noteId: string, reviewId: string, commentId: string
      await step(
        'Given a saved note, editor rename events move the same discussion while copy events leave it in place',
        async () => {
          // Act ---------------------------------------------------------------
          await e.edit(notePath)
          await e.lua('vim.api.nvim_win_set_cursor(0,{2,0})')
          await e.command('Lgtm note')
          await e.compose('Note before moving')
          noteId = await e.value(
            'require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation.comments[1].id',
          )
          await rename(notePath, destination)
          await e.lua('vim.cmd.file(vim.fn.fnameescape(...))', destination)

          // Assert ------------------------------------------------------------
          await e.wait(
            'not require("lgtm.notes").states[vim.api.nvim_get_current_buf()].moving',
            'note rename settled',
          )
          const state = await e.value(
            'require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation',
          )
          expect(state.comments[0].id).toBe(noteId)
          expect(state.threads[0]).toMatchObject({ sourcePath: notePath, currentPath: destination })

          // Act ---------------------------------------------------------------
          await e.command('Lgtm note')
          await e.compose('New note after moving')

          // Assert ------------------------------------------------------------
          const notes = await e.value(
            'require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation',
          )

          expect(notes.comments).toHaveLength(2)
          expect(notes.threads.some((t: Json) => t.sourcePath === destination)).toBe(true)

          // Arrange -----------------------------------------------------------
          const copy = join(original, 'COPY.md')

          // Act ---------------------------------------------------------------
          await copyFile(destination, copy)

          // Assert ------------------------------------------------------------
          await e.lua(
            'local from,to=...; vim.api.nvim_exec_autocmds("User",{pattern="LgtmPathRenamed",data={from=from,to=to}})',
            destination,
            copy,
          )

          expect(
            await e.value(
              'require("lgtm.notes").states[vim.api.nvim_get_current_buf()].scope.notes.path',
            ),
          ).toBe(destination)
          expect(
            (await r.rpc('GetConversation', { scope: { notes: { path: copy } } }, '')).comments ??
              [],
          ).toHaveLength(0)
        },
      )
      const last = join(r.workspace, '.difflab/final-plan')
      await step(
        'When an ignored review root moves twice, cards remain visible and new feedback records the intermediate source',
        async () => {
          // Act ---------------------------------------------------------------
          await e.command(`Lgtm directory ${original}`)

          // Assert ------------------------------------------------------------
          await e.loaded()

          // Act ---------------------------------------------------------------
          await e.show('PLAN.md')
          await e.pane('new', 2)
          await e.keys('c')
          await e.compose('Review before root move')
          let state = (await e.state())!
          reviewId = state.review.id
          commentId = state.conversation.comments[0].id

          // Arrange -----------------------------------------------------------
          const threadId = state.conversation.comments[0].threadId
          let from = original

          // Assert ------------------------------------------------------------
          for (const [index, to] of [join(r.workspace, '.difflab/moved-plan'), last].entries()) {
            // Act -------------------------------------------------------------
            await rename(from, to)

            // Assert ----------------------------------------------------------
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
            state = (await e.state())!
            expect(state.conversation.threads.find((t: Json) => t.id === threadId)).toMatchObject({
              sourcePath: join(original, 'PLAN.md'),
              currentPath: join(to, 'PLAN.md'),
              anchor: { path: 'PLAN.md' },
            })
            expect(JSON.stringify(await e.marks())).toContain('Review before root move')
            if (index === 0) {
              // Act -----------------------------------------------------------
              await e.pane('new', 3)
              await e.keys('c')
              await e.compose('Review after first root move')
            }
            from = to
          }

          // Assert ------------------------------------------------------------
          expect((await e.state())!.conversation.comments).toHaveLength(2)

          const before = (await e.state())!.conversation

          // Act ---------------------------------------------------------------
          await e.choose('Preview')
          await e.command('Lgtm export review')

          // Assert ------------------------------------------------------------
          await waitFor(
            'moved export',
            () => e.text(),
            (text) => text.includes('Review after first root move'),
          )
          expect(await e.text()).toContain('Review before root move')

          // Act ---------------------------------------------------------------
          await e.keys('q')

          // Assert ------------------------------------------------------------
          expect((await e.state())!.conversation).toEqual(before)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
        },
      )
      await step(
        'Then editor/backend restart resumes both notes and the same ignored review',
        async () => {
          // Act ---------------------------------------------------------------
          await e.close()
          await r.restart()
          await e.start()
          await e.edit(destination)
          await e.command('Lgtm notes')

          // Assert ------------------------------------------------------------
          await e.wait('require("lgtm.threads").last ~= nil', 'moved notes modal')
          expect(await e.text()).toContain('Note before moving')

          // Act ---------------------------------------------------------------
          await e.keys('q')

          // Assert ------------------------------------------------------------
          const notes = await r.rpc(
            'GetConversation',
            { scope: { notes: { path: destination } } },
            '',
          )

          expect(notes.comments).toHaveLength(2)
          expect(notes.comments.map((c: Json) => c.id)).toContain(noteId)

          // Act ---------------------------------------------------------------
          await e.command(`Lgtm directory ${last}`)

          // Arrange -----------------------------------------------------------
          const state = await e.loaded()

          // Assert ------------------------------------------------------------
          expect(state.review.id).toBe(reviewId)
          expect(state.root).toBe(last)
          expect(state.conversation.comments).toHaveLength(2)
          expect(state.conversation.comments.map((c: Json) => c.id)).toContain(commentId)
        },
      )
    })
  })
}
