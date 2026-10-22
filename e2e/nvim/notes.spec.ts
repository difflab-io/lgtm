/**
 * @module-tag e2e
 * @module-tag nvim
 * @module-tag notes
 */
import { describe, it, expect } from 'vitest'
import { join } from 'node:path'
import { readFile, rename, copyFile } from 'node:fs/promises'
import { profiles, useEditor } from './fixtures.js'
import type { Editor } from './editor.js'
import type { Runtime } from '../common/runtime.js'
import { waitFor, type Json } from '../common/runtime.js'

async function prepareBrowsingFile(r: Runtime) {
  await r.git('init', '-q', '-b', 'main')
  const file = await r.write('notes.txt', 'one\ntwo\nthree\n')
  await r.write('.gitignore', '.difflab\n')
  await r.commit('base')
  await r.write('notes.txt', 'one\nchanged\nthree\n')
  return file
}

async function prepareNote(e: Editor, r: Runtime, body: string, reply?: string) {
  const file = await prepareBrowsingFile(r)

  await e.edit(file)
  await e.command('1,2Lgtm note')
  await e.compose(body)
  if (reply) {
    await e.lua('vim.api.nvim_win_set_cursor(0,{2,0})')
    await e.popup()
    await e.keys('c')
    await e.compose(reply)
  }
  return file
}

for (const profile of profiles) {
  describe(`When adding notes while browsing files [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)
    it(
      'The note composer is centered and the saved selection has a visible card',
      { tags: profile === 'required' ? ['smoke'] : [] },
      async () => {
        // Arrange
        const { editor: e, runtime: r } = fixture()
        const file = await prepareBrowsingFile(r)

        // Act
        await e.edit(file)
        await e.command('1,2Lgtm note')
        const composer = await e.composer()

        // Assert
        const config = await e.lua(
          'local c=vim.api.nvim_win_get_config(...); return {width=c.width,height=c.height,row=c.row,col=c.col}',
          composer.win,
        )
        expect(config).toMatchObject({ width: 80, height: 12, row: 13, col: 19 })

        // Act
        await e.compose('Original standalone note')

        // Assert
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
      },
    )
  })
  describe(`When discussing notes [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)
    it('Nested replies identify the selected reply and edit its own text', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      await prepareNote(e, r, 'Original standalone note')
      // Act
      const popup = await e.popup()

      // Assert
      const bounds = await e.lua(
        'local c=vim.api.nvim_win_get_config(...); return {width=c.width,height=c.height,row=c.row,col=c.col}',
        popup.win,
      )
      expect(bounds).toMatchObject({ width: 86, height: 5, row: 16, col: 16 })

      // Act
      await e.keys('c')
      await e.compose('Note reply')
      await e.popup()

      // Assert
      const text = await e.text()
      expect(text).toContain('TOP-LEVEL COMMENT')
      expect(text).toContain('└─ REPLY 1')
      expect(text).not.toContain('notes.txt')

      // Act
      await e.keys('<Tab>')

      // Assert
      expect(
        JSON.stringify(
          await e.value('vim.api.nvim_win_get_config(require("lgtm.threads").last.win).footer'),
        ),
      ).toContain('Reply 1')

      // Act
      await e.keys('e')

      // Assert
      expect(
        JSON.stringify(
          await e.value('vim.api.nvim_win_get_config(require("lgtm.composer").last.win).title'),
        ),
      ).toContain('Edit reply 1')

      // Act
      await e.compose('Edited note reply')

      // Assert
      const notes = await e.value(
        'require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation',
      )
      expect(notes.comments.map((c: Json) => c.body)).toEqual([
        'Original standalone note',
        'Edited note reply',
      ])
    })
    it('Root editing and resolution preserve the attached discussion', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      await prepareNote(e, r, 'Original standalone note', 'Edited note reply')
      // Act
      await e.keys(' re')
      await e.compose('Edited standalone note')
      await e.popup()
      await e.keys('x')

      // Assert
      await e.wait(
        'require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation.threads[1].state == "resolved"',
        'note resolved',
      )
      const cards = await e.value(
        'vim.api.nvim_buf_get_extmarks(0,require("lgtm.threads").namespace,0,-1,{details=true})',
      )
      expect(JSON.stringify(cards)).toContain('Resolved')
      expect(JSON.stringify(cards)).not.toContain('Edited standalone note')
      expect(JSON.stringify(cards)).toContain('✓')
      const rows = cards.flatMap((mark: Json) => mark[3].virt_lines ?? [])
      expect(rows).toHaveLength(1)
      const statusRow = rows[0].map((chunk: [string, unknown]) => chunk[0]).join('')
      expect(statusRow).toMatch(/✓ $/)
      expect(await e.lua('return vim.fn.strdisplaywidth(...)', statusRow)).toBe(
        await e.value('vim.api.nvim_win_get_width(0)'),
      )

      // Act
      await e.popup()

      // Assert
      expect((await e.text()).toUpperCase()).toContain('RESOLVED')
      expect(await e.text()).toContain('Edited note reply')

      // Act
      await e.keys('x')

      // Assert
      await e.wait(
        'require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation.threads[1].state ~= "resolved"',
        'note reopened',
      )
    })
    it('Deleting the selected reply preserves its top-level note', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      await prepareNote(e, r, 'Edited standalone note', 'Edited note reply')
      // Act
      await e.popup('Edited note reply')
      await e.choose('Delete')
      await e.keys('dd')

      // Assert
      await e.wait(
        '#require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation.comments == 1',
        'selected reply deleted',
      )
      const notes = await e.value(
        'require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation',
      )
      expect(notes.comments[0].body).toBe('Edited standalone note')
    })
  })
  describe(`When promoting and exporting notes [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)
    it('User can promote notes into the active review and export readable feedback', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const file = await prepareNote(e, r, 'Edited standalone note')
      // Act
      await e.choose('Preview')
      await e.command('Lgtm export notes')

      // Assert
      await waitFor(
        'notes preview',
        () => e.text(),
        (text) => text.includes('Edited standalone note'),
      )

      // Act
      await e.command('close')
      const state = await e.openLocal('working')

      // Assert
      expect(state.base).toBeTruthy()

      // Act
      await e.pane('old', 2)
      await e.command('Lgtm comment line')
      await e.compose('Branch old-side feedback\n\n- Preserve **Markdown** and `code`.')

      // Assert
      expect((await e.state())!.conversation.threads[0].anchor.side).toBe('old')

      // Act
      await e.popup()
      await e.keys('c')
      await e.compose('Old-side reply with a useful alternative')

      // Assert
      await e.comments(2)

      // Act
      await e.command('Lgtm layout')

      // Assert
      await e.wait('require("lgtm.context").current.layout == "inline"', 'inline layout')

      // Act
      await e.command('Lgtm layout')

      // Assert
      await e.wait(
        'require("lgtm.context").current.layout == "side-by-side"',
        'side by side layout',
      )

      // Arrange
      const reviewTab = (await e.state())!.tab

      // Act
      await e.command('tabnew')
      const notesTab = await e.value('vim.api.nvim_get_current_tabpage()')
      await e.edit(file)
      await e.choose(1, 'Promote')
      await e.command('Lgtm promote')

      // Assert
      await e.comments(3)
      expect(await e.value('vim.api.nvim_get_current_tabpage()')).toBe(reviewTab)

      // Act
      await e.lua('vim.cmd(vim.api.nvim_tabpage_get_number(...).."tabclose")', notesTab)
      const before = (await e.state())!.conversation
      await e.choose('Save file')
      await e.command('Lgtm export review')

      // Assert
      await waitFor(
        'export file',
        () => readFile(join(r.workspace, 'review.md'), 'utf8'),
        (text) => text.includes('Branch old-side feedback'),
      )
      await e.lua(
        `_G.e2e.copied=nil; vim.g.clipboard={name="E2E clipboard",copy={["+"]=function(lines) _G.e2e.copied=table.concat(lines,"\\n") end,["*"]=function() end},paste={["+"]=function() return {{},"v"} end,["*"]=function() return {{},"v"} end}}`,
      )

      // Act
      await e.choose('Copy to clipboard')
      await e.command('Lgtm export review')

      // Assert
      await e.wait('_G.e2e.copied ~= nil', 'clipboard export')

      // Arrange
      const markdown = await e.value<string>('_G.e2e.copied')

      // Assert
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
    })
  })
  describe(`When reopening notes [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)
    it('The editor restores saved note cards and their thread modal after restart', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const file = await prepareNote(e, r, 'Edited standalone note')
      // Act
      await e.close()
      await r.restart()
      await e.start()
      await e.edit(file)

      // Assert
      await e.wait(
        'require("lgtm.notes").states[vim.api.nvim_get_current_buf()] and require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation ~= nil',
        'persisted notes loaded',
      )
      expect(
        await e.value('vim.api.nvim_buf_get_extmarks(0,require("lgtm.threads").namespace,0,-1,{})'),
      ).toHaveLength(1)

      // Act
      await e.command('Lgtm notes')

      // Assert
      await e.wait('require("lgtm.threads").last ~= nil', 'persisted note modal')
      expect(await e.text()).toContain('Edited standalone note')

      // Act
      await e.keys('q')
    })
  })
  describe(`When a note file moves [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)
    it('The note follows real renames, rejects copy events and survives restart', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      const original = join(r.workspace, '.difflab/move-case'),
        notePath = await r.write('.difflab/move-case/NOTE.md', 'before\nselected\nafter\n')
      const destination = join(r.workspace, '.difflab/moved-note.md')

      // Act
      await e.edit(notePath)
      await e.lua('vim.api.nvim_win_set_cursor(0,{2,0})')
      await e.command('Lgtm note')
      await e.compose('Note before moving')
      const noteId = await e.value(
        'require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation.comments[1].id',
      )
      await rename(notePath, destination)
      await e.lua('vim.cmd.file(vim.fn.fnameescape(...))', destination)

      // Assert
      await e.wait(
        'not require("lgtm.notes").states[vim.api.nvim_get_current_buf()].moving',
        'note rename settled',
      )
      const movedState = await e.value(
        'require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation',
      )
      expect(movedState.comments[0].id).toBe(noteId)
      expect(movedState.threads[0]).toMatchObject({
        sourcePath: notePath,
        currentPath: destination,
      })

      // Act
      await e.command('Lgtm note')
      await e.compose('New note after moving')

      // Assert
      const movedNotes = await e.value(
        'require("lgtm.notes").states[vim.api.nvim_get_current_buf()].conversation',
      )
      expect(movedNotes.comments).toHaveLength(2)
      expect(movedNotes.threads.some((t: Json) => t.sourcePath === destination)).toBe(true)

      // Arrange
      const copy = join(original, 'COPY.md')

      // Act
      await copyFile(destination, copy)

      // Assert
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
        (await r.rpc('GetConversation', { scope: { notes: { path: copy } } }, '')).comments ?? [],
      ).toHaveLength(0)

      // Act
      await e.close()
      await r.restart()
      await e.start()
      await e.edit(destination)
      await e.command('Lgtm notes')

      // Assert
      await e.wait('require("lgtm.threads").last ~= nil', 'moved notes modal')
      expect(await e.text()).toContain('Note before moving')

      // Act
      await e.keys('q')

      // Assert
      const restartedNotes = await r.rpc(
        'GetConversation',
        { scope: { notes: { path: destination } } },
        '',
      )
      expect(restartedNotes.comments).toHaveLength(2)
      expect(restartedNotes.comments.map((c: Json) => c.id)).toContain(noteId)
    })
  })
}
