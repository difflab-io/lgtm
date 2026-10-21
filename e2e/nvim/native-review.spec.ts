import { describe, expect } from 'vitest'
import { join } from 'node:path'
import { profiles, suite } from './fixtures.js'
import { step, waitFor, type Json } from '../common/runtime.js'
import type { Editor } from '../common/editor.js'

// fixtures --------------------------------------------------------------------

async function seen(e: Editor, path: string, expected: boolean) {
  // Assert --------------------------------------------------------------------
  expect(Boolean((await e.state())!.reviewed?.[path])).toBe(expected)
  const visible = await e.lua(
    `local path=...; local s=require("lgtm.context").current
    local p=require("codediff.ui.lifecycle").get_panel_view(s.tab)
    for _,mark in ipairs(vim.api.nvim_buf_get_extmarks(p.bufnr,vim.api.nvim_create_namespace("lgtm.reviewed"),0,-1,{details=true})) do
      local node=p.tree:get_node(mark[2]+1)
      if node and node.data.path==path then return mark[4].virt_text[1][1]:find("✓",1,true) ~= nil end
    end; return false`,
    path,
  )
  expect(visible, `visible checkmark for ${path}`).toBe(expected)
}

// tests -----------------------------------------------------------------------

for (const profile of profiles) {
  const it = suite(profile)
  describe(`Neovim · native CodeDiff reviews · ${profile}`, () => {
    it('opens branch PRs immediately, renders professional comment cards, follows pushes and preserves native local review behavior', async ({
      editor: e,
      runtime: r,
      github: gh,
    }) => {
      // Arrange ---------------------------------------------------------------
      await r.git('init', '-q', '-b', 'main')

      await r.git('remote', 'add', 'origin', 'git@github.com:acme/repo.git')
      await r.write('review.txt', 'old\n')
      await r.write('src/lib.rs', 'pub fn old() {}\n// second line\n// third line\n')
      const base = await r.commit('base')
      await r.git('checkout', '-qb', 'feature')
      await r.write('review.txt', 'new\n')
      await r.write(
        'src/lib.rs',
        'pub fn new() {}\n// second line\n// inserted context\n// third line\n',
      )
      let head = await r.commit('head')
      gh.control({ base, head })
      await e.cd(r.workspace)

      await step(
        'Given a provider failure after PR selection, the native diff remains usable and refresh ends the failed loading state',
        async () => {
          // Act ---------------------------------------------------------------
          await e.command('Lgtm prs')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.picker").last and not require("lgtm.picker").last.closed',
            'PR picker',
          )

          // Act ---------------------------------------------------------------
          gh.control({ mode: 'denied' })
          await e.keys('<Down><CR>')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current and require("lgtm.context").current.load_error and require("lgtm.context").current.render_ready',
            'failed PR load',
          )
          const state = (await e.state())!
          expect(Number(state.target.pullRequest.number)).toBe(2)
          expect(state.initializing).toBeFalsy()
          expect(state.review).toBeUndefined()

          // Act ---------------------------------------------------------------
          await e.pane()
          for (const action of ['c', ' rs']) {
            // Act -------------------------------------------------------------
            await e.keys(action)

            // Assert ----------------------------------------------------------
            const message = (await e.notices()).at(-1)!.message

            expect(message).toContain('could not load')
            expect(message).toContain(':Lgtm refresh')
            expect(message).not.toContain('still loading')
          }
          await e.command('Lgtm reconcile')

          // Assert ------------------------------------------------------------
          expect((await e.notices()).at(-1)!.message).toContain(':Lgtm refresh')

          // Act ---------------------------------------------------------------
          gh.control({ mode: 'normal' })
          await e.command('Lgtm refresh')
          await e.command('Lgtm refresh')

          // Arrange -----------------------------------------------------------
          const recovered = await e.loaded()

          // Assert ------------------------------------------------------------
          expect(recovered.load_error).toBeFalsy()
          expect(recovered.tab).toBe(state.tab)

          // Act ---------------------------------------------------------------
          await e.pane()
          await e.keys('c')
          await e.composer()
          await e.keys('q')
          await e.command('Lgtm close')
        },
      )

      const reads = gh.contentReads
      await step(
        'When the branch PR picker opens, arrows move its highlighted selection and CodeDiff renders before slow comments finish',
        async () => {
          // Arrange -----------------------------------------------------------
          gh.control({ comments_delay: 1.5 })

          // Act ---------------------------------------------------------------
          await e.command('Lgtm prs')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.picker").last and not require("lgtm.picker").last.closed',
            'PR picker',
          )
          expect(await e.value('vim.api.nvim_win_get_width(require("lgtm.picker").last.win)')).toBe(
            76,
          )

          // Act ---------------------------------------------------------------
          await e.keys('<Down>')

          // Assert ------------------------------------------------------------
          expect(await e.value('require("lgtm.picker").last.index')).toBe(2)

          // Act ---------------------------------------------------------------
          await e.keys('<Up>')

          // Assert ------------------------------------------------------------
          expect(await e.value('require("lgtm.picker").last.index')).toBe(1)
          expect(
            await e.value(
              'vim.api.nvim_buf_get_extmarks(require("lgtm.picker").last.list_buf,vim.api.nvim_create_namespace("lgtm.picker.selection"),0,-1,{})',
            ),
          ).toHaveLength(1)

          // Act ---------------------------------------------------------------
          await e.keys('<CR>')

          // Arrange -----------------------------------------------------------
          const state = await e.ready()

          // Assert ------------------------------------------------------------
          expect(state.git_root).toBe(r.workspace)
          expect(state.temp_dir).toBeUndefined()
          expect(state.files).toHaveLength(2)
          expect(state.initializing).toBeTruthy()
          expect(
            await e.value(
              'require("codediff.ui.lifecycle").get_panel_name(require("lgtm.context").current.tab)',
            ),
          ).toBe('explorer')
          for (const pane of Object.values(state.panes) as Json[]) {
            // Assert ----------------------------------------------------------
            expect(await e.value('vim.api.nvim_buf_get_name(...)', pane.buf)).toMatch(
              /^codediff:\/\//,
            )
            expect(await e.value('vim.bo[...].buftype', pane.buf)).toBe('nowrite')
            expect(await e.value('vim.lsp.get_clients({bufnr=...})', pane.buf)).toEqual([])
          }
          expect(gh.contentReads).toBe(reads)

          const search = gh.requests.find((request) => request.path === '/search/issues')

          expect(search).toBeDefined()
          expect(search!.search).toContain('repo:acme/repo')
          expect(search!.search).toContain('head:feature')
        },
      )

      await step(
        'Then marking a tree selection loads and checks the correct file, survives redraw and toggles cleanly',
        async () => {
          // Assert ------------------------------------------------------------
          await e.loaded()

          // Act ---------------------------------------------------------------
          await e.pane()
          const previous = (await e.state())!.file.path
          await e.keys(' rm')
          await seen(e, previous, true)
          await e.keys(' rm')
          await seen(e, previous, false)

          // Arrange -----------------------------------------------------------
          const selected = previous === 'review.txt' ? 'src/lib.rs' : 'review.txt'

          // Act ---------------------------------------------------------------
          await e.lua(
            `local path=...; local p=require("codediff.ui.lifecycle").get_panel_view(require("lgtm.context").current.tab)
          vim.api.nvim_set_current_win(p.winid)
          for line=1,vim.api.nvim_buf_line_count(p.bufnr) do local n=p.tree:get_node(line)
            if n and n.data.path==path then vim.api.nvim_win_set_cursor(p.winid,{line,0}); return end end`,
            selected,
          )

          // Assert ------------------------------------------------------------
          expect((await e.state())!.file.path).toBe(previous)

          // Act ---------------------------------------------------------------
          await e.keys('m')

          // Assert ------------------------------------------------------------
          await waitFor(
            'selected tree file marked',
            () => e.state(),
            (s) => Boolean(s?.file.path === selected && s?.reviewed[selected]),
          )
          await seen(e, selected, true)
          await seen(e, previous, false)

          await e.lua(
            'require("codediff.ui.lifecycle").get_panel_view(require("lgtm.context").current.tab).tree:render()',
          )
          await seen(e, selected, true)

          // Act ---------------------------------------------------------------
          await e.keys('m')
          await seen(e, selected, false)
        },
      )

      let original: Json, threadId: string
      await step(
        'When lowercase c adds feedback, full-width collapsed cards show status and one preview line without changing read-only code',
        async () => {
          // Act ---------------------------------------------------------------
          await e.show('src/lib.rs')
          await e.pane()
          await e.keys('c')

          // Arrange -----------------------------------------------------------
          const body =
            'Native CodeDiff comment with optional Kak\nSecond line is shown only after expansion\nThird detail line'

          // Act ---------------------------------------------------------------
          await e.compose(body)

          // Arrange -----------------------------------------------------------
          const state = (await e.comments(1))!

          // Assert ------------------------------------------------------------
          expect((await e.text(state.panes.new.buf)).split('\n')[0]).toBe('pub fn new() {}')
          const marks = await e.marks()
          expect(marks.length).toBeGreaterThan(0)
          for (const mark of marks) {
            // Arrange ---------------------------------------------------------
            const details = mark[3]

            // Assert ----------------------------------------------------------
            expect(details.virt_text).toBeUndefined()
            expect(details.virt_lines_leftcol).toBe(true)
            expect(details.virt_lines).toHaveLength(3)
            expect(JSON.stringify(details.virt_lines)).toContain('GitHub draft')
            expect(JSON.stringify(details.virt_lines)).toContain('Native CodeDiff')
            expect(JSON.stringify(details.virt_lines)).not.toContain('Second line')
            for (const row of details.virt_lines) {
              // Arrange -------------------------------------------------------
              const text = row.map((chunk: [string, string]) => chunk[0]).join('')

              // Assert --------------------------------------------------------
              expect(await e.value('vim.fn.strdisplaywidth(...)', text)).toBe(
                await e.value('vim.api.nvim_win_get_width(...)', state.panes.new.win),
              )
            }
          }

          // Act ---------------------------------------------------------------
          await e.command('redraw')

          // Assert ------------------------------------------------------------
          const alignment = await e.lua(
            'local s=require("lgtm.context").current; return {vim.fn.screenpos(s.panes.old.win,3,1).row,vim.fn.screenpos(s.panes.new.win,4,1).row}',
          )

          expect(alignment[0]).toBeGreaterThan(0)
          expect(alignment[0]).toBe(alignment[1])

          // Act ---------------------------------------------------------------
          const popup = await e.popup()

          // Assert ------------------------------------------------------------
          expect(await e.text()).toContain('Third detail line')
          const bounds = await e.value('vim.api.nvim_win_get_config(...)', popup.win)
          expect(bounds.row).toBe(Math.floor((40 - bounds.height - 2) / 2))
          expect(bounds.col).toBe(Math.floor((120 - bounds.width - 2) / 2))

          // Act ---------------------------------------------------------------
          await e.keys('q')
          await e.keys(profile === 'kak' ? 'VJ' : 'Vj')
          await e.keys('c')
          await e.compose('Range feedback from the visual comment shortcut')

          // Arrange -----------------------------------------------------------
          const current = (await e.comments(2))!
          const range = current.conversation.threads.find(
            (t: Json) => t.anchor.start === '1' && t.anchor.end === '2',
          )

          // Assert ------------------------------------------------------------
          expect(range.anchor).toMatchObject({
            side: 'new',
            source: { commit: { sha: head } },
            evidence: { selectedText: 'pub fn new() {}\n// second line' },
          })
          threadId = state.conversation.threads[0].id
          original = state.conversation.threads[0].anchor
          expect(gh.contentReads).toBe(reads)
        },
      )

      await step(
        'Then provider-outdated comments float above the file with original evidence even when their old text still exists',
        async () => {
          // Arrange -----------------------------------------------------------
          gh.control({ outdated_pr: 1, outdated: true, comments_delay: 0 })

          // Act ---------------------------------------------------------------
          await e.command('Lgtm refresh')

          // Assert ------------------------------------------------------------
          await waitFor(
            'provider outdated',
            () => e.state(),
            (s) =>
              Boolean(
                s?.render_ready &&
                s?.conversation.threads.find((t: Json) => t.id === threadId)?.forgeObservation
                  ?.outdated,
              ),
          )
          expect(
            (await e.state())!.conversation.threads.find((t: Json) => t.id === threadId).anchor,
          ).toEqual(original)
          for (const mark of await e.marks()) {
            // Assert ----------------------------------------------------------
            expect(mark[1]).toBe(0)
            expect(mark[3].virt_lines_above).toBe(true)
            expect(JSON.stringify(mark[3].virt_lines)).toContain('Outdated')
          }

          // Act ---------------------------------------------------------------
          await e.pane()
          await e.popup()

          // Assert ------------------------------------------------------------
          expect(await e.text()).toContain('OUTDATED')
          expect(await e.text()).toContain('Original lines')

          // Act ---------------------------------------------------------------
          await e.keys('q')
          gh.control({ outdated_pr: 1, outdated: false })
          await e.command('Lgtm refresh')

          // Assert ------------------------------------------------------------
          await waitFor(
            'current provider location',
            () => e.state(),
            (s) =>
              Boolean(
                s?.render_ready &&
                !s?.conversation.threads.find((t: Json) => t.id === threadId)?.forgeObservation
                  ?.outdated,
              ),
          )
          expect(
            (await e.state())!.conversation.threads.find((t: Json) => t.id === threadId).anchor,
          ).toEqual(original)
          expect(gh.contentReads).toBe(reads)
        },
      )

      await step(
        'When commits arrive on reopen or during composition, the same PR and pending review follow the selected text',
        async () => {
          // Arrange -----------------------------------------------------------
          let state = (await e.state())!
          const id = state.review.id,
            pending = state.review.forgeReviewId
          const writes = gh.writes

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
          await r.write(
            'src/lib.rs',
            '// added during review\npub fn new() {}\n// second line\n// inserted context\n// third line\n',
          )
          head = await r.commit('push during review')
          gh.control({ head, shift_current_lines: 1 })
          await e.command('Lgtm prs')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.picker").last and not require("lgtm.picker").last.closed',
            'reopen picker',
          )

          // Act ---------------------------------------------------------------
          await e.keys('<CR>')
          state = await e.loaded()

          // Assert ------------------------------------------------------------
          expect(state.review).toMatchObject({ id, forgeReviewId: pending })
          expect(gh.writes).toBe(writes)

          // Act ---------------------------------------------------------------
          await e.show('src/lib.rs')

          // Assert ------------------------------------------------------------
          expect(await e.text(state.panes.new.buf)).toContain('// added during review')

          // Act ---------------------------------------------------------------
          await e.pane('new', 2)
          await e.keys('c')
          await e.composer()
          await r.write(
            'src/lib.rs',
            '// another push\n// added during review\npub fn new() {}\n// second line\n// inserted context\n// third line\n',
          )
          head = await r.commit('push while composing')
          gh.control({ head, shift_current_lines: 1 })
          await e.compose('Comment survives a push while its composer is open')

          // Assert ------------------------------------------------------------
          await waitFor(
            'latest native source',
            () => e.state(),
            (s) => Boolean(s?.head === head && s.render_ready && !s.initializing),
          )
          state = (await e.state())!
          expect(state.review).toMatchObject({ id, forgeReviewId: pending })

          const remote = gh.threads[1].find((t) =>
            t.comments.nodes[0].body.includes('Comment survives a push'),
          )!

          expect(remote).toMatchObject({ path: 'src/lib.rs', line: 3 })
          expect(gh.contentReads).toBe(reads)
          expect(await e.text(state.panes.new.buf)).toContain('// another push')

          // Arrange -----------------------------------------------------------
          const after = gh.writes

          gh.control({ head: base })

          // Act ---------------------------------------------------------------
          await e.command('Lgtm refresh')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.empty and require("lgtm.context").current.render_ready',
            'empty pushed comparison',
          )
          for (const thread of (await e.state())!.conversation.threads)
            if (thread.forgeObservation) expect(thread.forgeObservation.head).toBe(base)

          // Act ---------------------------------------------------------------
          gh.control({ head })
          await e.command('Lgtm refresh')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.render_ready and not require("lgtm.context").current.empty',
            'files restored',
          )

          // Act ---------------------------------------------------------------
          gh.control({ head: 'e'.repeat(40) })
          await e.command('Lgtm refresh')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.render_ready and not require("lgtm.context").current.native_explorer',
            'unavailable commit uses remote snapshot',
          )
          expect(gh.contentReads).toBeGreaterThan(reads)

          // Act ---------------------------------------------------------------
          gh.control({ head })
          await e.command('Lgtm refresh')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.render_ready and require("lgtm.context").current.native_explorer',
            'native source restored',
          )
          expect((await e.state())!.review).toMatchObject({ id, forgeReviewId: pending })
          expect(gh.writes).toBe(after)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
        },
      )

      await step(
        'Then branch review uses the merge base locally and clean working/staged views have clear empty states',
        async () => {
          // Act ---------------------------------------------------------------
          let state = await e.openLocal('branch')

          // Assert ------------------------------------------------------------
          expect(state.kind).toBe('git')
          expect(state.base).toBe(base)
          expect(state.head).toBe(head)

          // Act ---------------------------------------------------------------
          await e.pane()

          // Arrange -----------------------------------------------------------
          const path = state.file.path

          // Act ---------------------------------------------------------------
          await e.keys(' rm')
          await seen(e, path, true)
          await e.keys(' rm')
          await seen(e, path, false)
          await e.command('Lgtm complete')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.review.state=="completed"',
            'branch completed',
          )

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
          for (const mode of ['working', 'staged']) {
            // Act -------------------------------------------------------------
            state = await e.openLocal(mode)

            // Assert ----------------------------------------------------------
            expect(state.files ?? []).toHaveLength(0)
            expect(
              (await e.notices()).some(
                (n) =>
                  n.message ===
                  (mode === 'working' ? 'No working-tree changes' : 'No staged changes'),
              ),
            ).toBe(true)

            // Act -------------------------------------------------------------
            await e.command('Lgtm complete')

            // Assert ----------------------------------------------------------
            await e.wait(
              'require("lgtm.context").current.review.state=="completed"',
              'empty review completed',
            )

            // Act -------------------------------------------------------------
            await e.command('Lgtm close')
          }
        },
      )

      await step(
        'When working buffers are shared with ordinary tabs, review mappings and overlays follow the active tab',
        async () => {
          // Act ---------------------------------------------------------------
          await r.write('review.txt', 'unstaged\n')
          let state = await e.openLocal('working')
          await e.show('review.txt')

          // Assert ------------------------------------------------------------
          expect(await e.text(state.panes.new.buf)).toContain('unstaged')

          // Act ---------------------------------------------------------------
          await e.pane()
          await e.keys(' rm')
          await seen(e, 'review.txt', true)
          await e.keys(' rm')
          await seen(e, 'review.txt', false)
          await e.keys(' rm')
          await r.write('review.txt', 'working content changed after marking seen\n')
          await e.command('Lgtm refresh')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.render_ready and not require("lgtm.context").current.reviewed["review.txt"]',
            'working mark invalidated',
          )
          await seen(e, 'review.txt', false)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
          await e.edit(join(r.workspace, 'review.txt'))
          await e.command('Lgtm note')
          await e.compose('Standalone note outside the review')
          const mapping = await e.value('vim.fn.maparg("c","n",false,true).desc')
          state = await e.openLocal('working')
          await e.show('review.txt')

          // Arrange -----------------------------------------------------------
          const reviewTab = state.tab,
            buffer = state.panes.new.buf

          // Act ---------------------------------------------------------------
          await e.command(`tabnew ${join(r.workspace, 'review.txt')}`)

          // Assert ------------------------------------------------------------
          expect(await e.value('vim.api.nvim_get_current_buf()')).toBe(buffer)
          expect(await e.value('vim.fn.maparg("c","n",false,true).desc')).toBe(mapping)
          expect(await e.value('vim.b.lgtm')).toBeFalsy()
          await e.wait(
            'vim.inspect(vim.api.nvim_buf_get_extmarks(0,require("lgtm.threads").namespace,0,-1,{details=true})):find("Standalone note outside",1,true) ~= nil',
            'ordinary notes shown',
          )

          await e.lua('vim.api.nvim_exec_autocmds("ColorScheme",{modeline=false})')

          // Act ---------------------------------------------------------------
          await e.command('tabclose')

          // Assert ------------------------------------------------------------
          await waitFor(
            'review mapping restored',
            () =>
              e.lua<string>(
                'return vim.api.nvim_buf_call(...,function() return vim.fn.maparg("c","n",false,true).desc end)',
                buffer,
              ),
            (value) => value === 'Add review comment',
          )
          expect(await e.value('vim.api.nvim_get_current_tabpage()')).toBe(reviewTab)
          expect(JSON.stringify(await e.marks())).not.toContain('Standalone note outside')
        },
      )

      await step(
        'Then hidden working and staged source changes invalidate only their own reviewed checkmarks',
        async () => {
          // Act ---------------------------------------------------------------
          await r.write('src/lib.rs', 'pub fn hidden_change() {}\n')
          await e.command('Lgtm refresh')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.render_ready and #require("lgtm.context").current.files==2',
            'new file metadata',
          )

          // Act ---------------------------------------------------------------
          await e.show('src/lib.rs')
          await e.pane()
          await e.keys(' rm')
          await seen(e, 'src/lib.rs', true)
          await e.show('review.txt')
          await e.pane()
          await e.keys(' rm')
          await seen(e, 'review.txt', true)
          await r.write('src/lib.rs', 'pub fn changed_after_seen() {}\n')
          await e.command('Lgtm refresh')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.render_ready and not require("lgtm.context").current.reviewed["src/lib.rs"]',
            'hidden working invalidated',
          )
          await seen(e, 'src/lib.rs', false)
          await seen(e, 'review.txt', true)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
          await r.git('add', 'review.txt', 'src/lib.rs')
          let state = await e.openLocal('staged', true)

          // Assert ------------------------------------------------------------
          expect(await e.value('vim.api.nvim_buf_get_name(...)', state.panes.new.buf)).toMatch(
            /^codediff:\/\//,
          )
          for (const path of ['review.txt', 'src/lib.rs']) {
            // Act -------------------------------------------------------------
            await e.show(path)
            await e.pane()
            await e.keys(' rm')
            await seen(e, path, true)
          }

          // Act ---------------------------------------------------------------
          await r.write('review.txt', 'staged again after marking seen\n')
          await r.git('add', 'review.txt')
          await e.command('Lgtm refresh')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.render_ready and not require("lgtm.context").current.reviewed["review.txt"]',
            'hidden index invalidated',
          )
          await seen(e, 'review.txt', false)
          await seen(e, 'src/lib.rs', true)

          state = (await e.state())!

          // Arrange -----------------------------------------------------------
          const saved = state.review

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
          await r.git(
            '-c',
            'user.name=E2E',
            '-c',
            'user.email=e2e@example.test',
            'commit',
            '-qm',
            'another change',
          )
          const nextHead = await r.git('rev-parse', 'HEAD')
          await e.choose('branch', 'Resume saved review')
          await e.command('Lgtm review')
          state = await e.loaded()

          // Assert ------------------------------------------------------------
          expect(state.review.id).toBe(saved.id)
          expect(state.comparison).toEqual(saved.comparison)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
          state = await e.openLocal('branch', true)

          // Assert ------------------------------------------------------------
          expect(state.review.id).not.toBe(saved.id)
          expect(state.head).toBe(nextHead)
          expect(state.base).toBe(base)

          // Act ---------------------------------------------------------------
          await e.command('Lgtm complete')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.review.state=="completed"',
            'replacement completed',
          )

          // Act ---------------------------------------------------------------
          await e.command('Lgtm close')
          await r.write('review.txt', 'temporary working edit\n')
          await e.openLocal('working')
          await r.git('restore', 'review.txt')
          await e.command('Lgtm refresh')

          // Assert ------------------------------------------------------------
          await e.wait(
            'require("lgtm.context").current.empty and #require("lgtm.context").current.files==0 and require("lgtm.context").current.file==nil',
            'last change restored',
          )
          expect(await e.value('vim.tbl_isempty(require("lgtm.context").current.views)')).toBe(true)
          expect(await e.value('require("lgtm.context").current.observe_sources ~= nil')).toBe(
            false,
          )
          expect(
            await e.value(
              'require("codediff.ui.lifecycle").get_session(require("lgtm.context").current.tab) ~= nil',
            ),
          ).toBe(true)
        },
      )
    })
  })
}
