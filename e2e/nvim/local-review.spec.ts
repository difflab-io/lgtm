/**
 * @module-tag e2e
 * @module-tag nvim
 */
import { describe, it, expect } from 'vitest'
import { profiles, useEditor } from './fixtures.js'

for (const profile of profiles) {
  describe(
    `When creating and editing local review threads during normal editing [${profile}]`,
    { tags: ['e2e', 'nvim', profile] },
    () => {
      const fixture = useEditor(profile)

      it(
        'should_create_an_anchored_local_thread_via_comment_editor_and_render_overlay',
        { tags: ['smoke'] },
        async () => {
          // Arrange
          const { editor, runtime } = fixture()
          const file = await runtime.write(
            'src/service.lua',
            [
              'local M = {}',
              'function M.run(input)',
              '  return input.value',
              'end',
              'return M',
              '',
            ].join('\n'),
          )
          await editor.edit(file)
          await editor.command('3')

          // Act
          await editor.keys(' rc')
          await editor.wait(
            'require("lgtm.features.comment_editor").is_open()',
            'comment-editor opened for local comment',
          )
          await editor.wait(
            'vim.fn.screenstring(1, 1) ~= nil and (#vim.api.nvim_list_wins() > 1)',
            'comment-editor floating layout mounted',
          )
          const diagnosticsWhileOpen = await editor.diagnostics()
          await editor.lua(
            'require("lgtm.features.comment_editor").set_text("Validate nil guard before dispatch")',
          )
          await editor.lua('require("lgtm.features.comment_editor").submit()')
          await editor.wait(
            'not require("lgtm.features.comment_editor").is_open()',
            'comment-editor closed after submit',
          )

          // Assert
          expect(diagnosticsWhileOpen.screen).toContain('Submit')
          expect(diagnosticsWhileOpen.screen).toContain('Cancel')
          const state = await editor.lua<{
            mode: string
            count: number
            path: string
            line: number
            body: string
            extmarkCount: number
            lastEvent: string
            lastMessage: string
          }>(`
            local lgtm = require("lgtm")
            local Notifications = require("lgtm.ui.notifications")
            local services = lgtm.get_services()
            local threads = services.local_service:get_threads()
            local ns = vim.api.nvim_get_namespaces()["lgtm_threads_overlay"]
            local marks = vim.api.nvim_buf_get_extmarks(0, ns, 0, -1, { details = true })
            local last = Notifications.last() or {}
            return {
              mode = lgtm.get_mode(),
              count = #threads,
              path = threads[1] and threads[1].path or "",
              line = threads[1] and threads[1].line or 0,
              body = threads[1] and threads[1].comments[1].body or "",
              extmarkCount = #marks,
              lastEvent = last.event or "",
              lastMessage = last.message or "",
            }
          `)
          expect(state).toEqual({
            mode: 'local',
            count: 1,
            path: 'src/service.lua',
            line: 3,
            body: 'Validate nil guard before dispatch',
            extmarkCount: 1,
            lastEvent: 'thread_created',
            lastMessage: 'Opened local thread at src/service.lua:3',
          })
        },
      )

      it('should_cancel_comment_editor_without_creating_a_thread', async () => {
        // Arrange
        const { editor, runtime } = fixture()
        const file = await runtime.write('src/cancel.lua', 'local x = 1\nreturn x\n')
        await editor.edit(file)
        await editor.command('2')

        // Act
        await editor.keys(' rc')
        await editor.wait(
          'require("lgtm.features.comment_editor").is_open()',
          'comment-editor opened',
        )
        await editor.lua('require("lgtm.features.comment_editor").set_text("Discard me")')
        await editor.lua('require("lgtm.features.comment_editor").cancel()')
        await editor.wait(
          'not require("lgtm.features.comment_editor").is_open()',
          'comment-editor cancelled',
        )

        // Assert
        const count = await editor.value<number>(
          '#require("lgtm").get_services().local_service:get_threads()',
        )
        expect(count).toBe(0)
      })
    },
  )

  describe(
    `When inspecting and managing local threads in threads-pane [${profile}]`,
    { tags: ['e2e', 'nvim', profile] },
    () => {
      const fixture = useEditor(profile)

      it('should_open_threads_pane_from_anchored_thread_summary_into_detail_view', async () => {
        // Arrange
        const { editor, runtime } = fixture()
        const file = await runtime.write(
          'src/handler.lua',
          'local H = {}\nfunction H.handle() end\nreturn H\n',
        )
        await editor.edit(file)
        await editor.command('2')
        await editor.keys(' rc')
        await editor.wait(
          'require("lgtm.features.comment_editor").is_open()',
          'comment-editor opened',
        )
        await editor.lua(
          'require("lgtm.features.comment_editor").set_text("Extract handler options table")',
        )
        await editor.lua('require("lgtm.features.comment_editor").submit()')

        // Act
        await editor.keys('<CR>')
        await editor.wait(
          'require("lgtm.features.threads_pane").is_open()',
          'threads-pane opened from anchored thread summary',
        )

        // Assert
        const detailState = await editor.lua<{
          activeView: string
          filetype: string
          paneText: string
          lastEvent: string
        }>(`
          local lgtm = require("lgtm")
          local ThreadsPane = require("lgtm.features.threads_pane")
          local Notifications = require("lgtm.ui.notifications")
          local pane = ThreadsPane.get_active()
          local bufnr = pane.split.bufnr
          local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
          return {
            activeView = lgtm.get_services().local_service.state:get_value().active_view,
            filetype = vim.bo[bufnr].filetype,
            paneText = table.concat(lines, "\\n"),
            lastEvent = (Notifications.last() or {}).event or "",
          }
        `)
        expect(detailState.activeView).toBe('detail')
        expect(detailState.filetype).toBe('lgtm-thread-detail')
        expect(detailState.paneText).toContain('Thread Detail · [OPEN] src/handler.lua:2')
        expect(detailState.paneText).toContain('Extract handler options table')
        expect(detailState.lastEvent).toBe('thread_opened')
      })

      it('should_collapse_uncollapse_reply_edit_and_resolve_threads_in_threads_pane', async () => {
        // Arrange
        const { editor, runtime } = fixture()
        const file = await runtime.write(
          'src/workflow.lua',
          'local W = {}\nlocal a = 1\nlocal b = 2\nreturn W\n',
        )
        await editor.edit(file)
        await editor.command('4')
        await editor.keys(' rc')
        await editor.wait(
          'require("lgtm.features.comment_editor").is_open()',
          'comment-editor opened',
        )
        await editor.lua(
          'require("lgtm.features.comment_editor").set_text("Initial local review note")',
        )
        await editor.lua('require("lgtm.features.comment_editor").submit()')

        // Act: open threads-pane in summary view and position cursor on the thread line
        await editor.keys(' rt')
        await editor.wait(
          'require("lgtm.features.threads_pane").is_open()',
          'threads-pane opened in summary view',
        )
        await editor.command('5')
        await editor.keys('za')

        // Assert: thread is collapsed in summary view
        const collapsedAfterFold = await editor.lua<{ collapsed: boolean; text: string }>(`
          local service = require("lgtm").get_services().local_service
          local pane = require("lgtm.features.threads_pane").get_active()
          local lines = vim.api.nvim_buf_get_lines(pane.split.bufnr, 0, -1, false)
          return {
            collapsed = service:get_threads()[1].collapsed,
            text = table.concat(lines, "\\n"),
          }
        `)
        expect(collapsedAfterFold.collapsed).toBe(true)
        expect(collapsedAfterFold.text).toContain('▸ [OPEN] src/workflow.lua:4')
        expect(collapsedAfterFold.text).not.toContain('Initial local review note')

        // Act: uncollapse with za, then press <CR> to enter thread-detail-view
        await editor.keys('za')
        await editor.keys('<CR>')
        await editor.wait(
          'require("lgtm").get_services().local_service.state:get_value().active_view == "detail"',
          'switched to thread-detail-view',
        )

        // Act: reply to thread with c
        await editor.keys('c')
        await editor.wait(
          'require("lgtm.features.comment_editor").is_open()',
          'reply comment-editor opened',
        )
        await editor.lua(
          'require("lgtm.features.comment_editor").set_text("Follow-up reply in detail view")',
        )
        await editor.lua('require("lgtm.features.comment_editor").submit()')

        // Assert: reply added and notification emitted
        const afterReply = await editor.lua<{ commentCount: number; lastEvent: string }>(`
          local service = require("lgtm").get_services().local_service
          local Notifications = require("lgtm.ui.notifications")
          return {
            commentCount = #service:get_threads()[1].comments,
            lastEvent = (Notifications.last() or {}).event or "",
          }
        `)
        expect(afterReply).toEqual({
          commentCount: 2,
          lastEvent: 'reply_added',
        })

        // Act: edit comment with e
        await editor.keys('e')
        await editor.wait(
          'require("lgtm.features.comment_editor").is_open()',
          'edit comment-editor opened',
        )
        await editor.lua(
          'require("lgtm.features.comment_editor").set_text("Edited follow-up reply in detail view")',
        )
        await editor.lua('require("lgtm.features.comment_editor").submit()')

        // Act: resolve thread with x and return to summary view with <BS>
        await editor.keys('x')
        await editor.keys('<BS>')

        // Assert: comment edited, thread resolved, and view returned to summary
        const finalState = await editor.lua<{
          activeView: string
          resolved: boolean
          secondBody: string
          lastEvent: string
        }>(`
          local service = require("lgtm").get_services().local_service
          local Notifications = require("lgtm.ui.notifications")
          local thread = service:get_threads()[1]
          return {
            activeView = service.state:get_value().active_view,
            resolved = thread.resolved,
            secondBody = thread.comments[2].body,
            lastEvent = (Notifications.last() or {}).event or "",
          }
        `)
        expect(finalState).toEqual({
          activeView: 'summary',
          resolved: true,
          secondBody: 'Edited follow-up reply in detail view',
          lastEvent: 'thread_resolved',
        })
      })
    },
  )
}
