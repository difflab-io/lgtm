/**
 * @module-tag e2e
 * @module-tag nvim
 */
import { describe, it, expect } from 'vitest'
import type { Editor } from './editor.js'
import type { TestHarness } from '../common/test-harness.js'
import { profiles, useEditor } from './fixtures.js'

async function seedWorkspacePullRequests(runtime: TestHarness, editor: Editor) {
  await runtime.git('init', '-b', 'feature/nvim-ui-scaffolding')
  await runtime.git('config', 'user.email', 'test@example.com')
  await runtime.git('config', 'user.name', 'Test User')
  await runtime.write(
    'src/review_target.lua',
    [
      'local Target = {}',
      'function Target.compute(x)',
      '  return x + 1',
      'end',
      'return Target',
      '',
    ].join('\n'),
  )
  await runtime.git('add', 'src/review_target.lua')
  await runtime.git('commit', '-m', 'feat: base implementation')
  const baseRevision = (await runtime.git('rev-parse', 'HEAD')).trim()

  await runtime.write(
    'src/review_target.lua',
    [
      'local Target = {}',
      'function Target.compute(x)',
      '  assert(type(x) == "number", "x must be a number")',
      '  return x + 2',
      'end',
      'return Target',
      '',
    ].join('\n'),
  )
  await runtime.git('add', 'src/review_target.lua')
  await runtime.git('commit', '-m', 'feat: validate input in review target')
  const headRevision = (await runtime.git('rev-parse', 'HEAD')).trim()
  const branch = (await runtime.git('rev-parse', '--abbrev-ref', 'HEAD')).trim()

  await editor.lua(
    `
    local args = ...
    require("lgtm").reset({
      seed_prs = {
        {
          number = 42,
          title = "feat(nvim): scaffold PR and local review UI",
          description = "## Summary\\nAdd reactive PR review UI scaffolding with CodeDiff.",
          author = "siddharth",
          source_branch = args.branch,
          target_branch = "main",
          base_revision = args.baseRevision,
          head_revision = args.headRevision,
          additions = 4,
          deletions = 1,
          files = {
            { path = "src/review_target.lua", status = "M", additions = 4, deletions = 1 },
          },
        },
        {
          number = 43,
          title = "chore(nvim): tune thread fold highlights",
          description = "## Summary\\nRefine virtual line highlights.",
          author = "reviewer",
          source_branch = args.branch,
          target_branch = "main",
          base_revision = args.baseRevision,
          head_revision = args.headRevision,
          additions = 2,
          deletions = 2,
          files = {
            { path = "src/review_target.lua", status = "M", additions = 2, deletions = 2 },
          },
        },
      },
    })
  `,
    { branch, baseRevision, headRevision },
  )
}

for (const profile of profiles) {
  describe(
    `When selecting an open PR from pr-select-modal [${profile}]`,
    { tags: ['e2e', 'nvim', profile] },
    () => {
      const fixture = useEditor(profile)

      it(
        'should_list_open_branch_prs_preview_description_on_the_right_and_open_codediff_on_selection',
        { tags: ['smoke'] },
        async () => {
          // Arrange
          const { editor, runtime } = fixture()
          await seedWorkspacePullRequests(runtime, editor)

          // Act
          await editor.keys(' rp')
          await editor.wait(
            'require("lgtm.features.pr_select_modal").is_open()',
            'pr-select-modal opened',
          )
          await editor.wait(
            '#vim.api.nvim_list_wins() >= 4',
            'pr-select-modal dual-pane layout mounted',
          )
          const modalDiagnostics = await editor.diagnostics()
          await editor.lua('require("lgtm.features.pr_select_modal").confirm_selection()')
          await editor.wait(
            'require("lgtm").get_mode() == "pr"',
            'switched to PR review mode in CodeDiff',
          )

          // Assert
          expect(modalDiagnostics.screen).toContain('#42')
          expect(modalDiagnostics.screen).toContain('Open Review')
          expect(modalDiagnostics.screen).toContain('Cancel')
          expect(modalDiagnostics.screen).toContain('Pull Request Description')
          expect(modalDiagnostics.screen).toContain('Add reactive PR review UI scaffolding')

          const prSession = await editor.lua<{
            isOpen: boolean
            mode: string
            selectedNumber: number
            winCount: number
            lastEvent: string
          }>(`
            local lgtm = require("lgtm")
            local PrSelectModal = require("lgtm.features.pr_select_modal")
            local Notifications = require("lgtm.ui.notifications")
            local pr_service = lgtm.get_services().pr_service
            local selected = pr_service.state:get_value().selected_pr
            return {
              isOpen = PrSelectModal.is_open(),
              mode = lgtm.get_mode(),
              selectedNumber = selected and selected.number or 0,
              winCount = #vim.api.nvim_tabpage_list_wins(0),
              lastEvent = (Notifications.last() or {}).event or "",
            }
          `)
          expect(prSession.isOpen).toBe(false)
          expect(prSession.mode).toBe('pr')
          expect(prSession.selectedNumber).toBe(42)
          expect(prSession.winCount).toBeGreaterThanOrEqual(2)
          expect(prSession.lastEvent).toBe('pr_selected')
        },
      )
    },
  )

  describe(
    `When reviewing a PR in CodeDiff and submitting an overall review [${profile}]`,
    { tags: ['e2e', 'nvim', profile] },
    () => {
      const fixture = useEditor(profile)

      it('should_create_pr_threads_in_codediff_and_display_only_pr_threads_in_threads_pane', async () => {
        // Arrange
        const { editor, runtime } = fixture()
        await seedWorkspacePullRequests(runtime, editor)
        await editor.lua(`
          require("lgtm").get_services().local_service:create_thread({
            path = "src/review_target.lua",
            line = 1,
            body = "Local-only note that stays in local mode",
          })
        `)
        await editor.keys(' rp')
        await editor.wait(
          'require("lgtm.features.pr_select_modal").is_open()',
          'pr-select-modal opened',
        )
        await editor.lua('require("lgtm.features.pr_select_modal").confirm_selection()')
        await editor.wait('require("lgtm").get_mode() == "pr"', 'PR mode active')

        // Act: focus the right-hand diff window in CodeDiff, draft a PR comment on line 3, and open threads-pane
        await editor.lua(`
          local ok, lifecycle = pcall(require, "codediff.ui.lifecycle")
          if ok then
            local session = lifecycle.get_session(vim.api.nvim_get_current_tabpage())
            if session and session.modified_win and vim.api.nvim_win_is_valid(session.modified_win) then
              vim.api.nvim_set_current_win(session.modified_win)
            end
          end
          require("lgtm.features.codediff_session").sync_current_tabpage()
          vim.api.nvim_win_set_cursor(0, { 3, 0 })
        `)
        await editor.keys('c')
        await editor.wait(
          'require("lgtm.features.comment_editor").is_open()',
          'PR comment-editor opened from diff buffer',
        )
        await editor.lua(
          'require("lgtm.features.comment_editor").set_text("Use immutable signal update in PR mode")',
        )
        await editor.lua('require("lgtm.features.comment_editor").submit()')
        await editor.keys(' rt')
        await editor.wait(
          'require("lgtm.features.threads_pane").is_open()',
          'threads-pane opened in PR mode',
        )

        // Assert
        const prThreadState = await editor.lua<{
          prThreadCount: number
          localThreadCount: number
          prBody: string
          paneText: string
          lastEvent: string
        }>(`
          local services = require("lgtm").get_services()
          local pane = require("lgtm.features.threads_pane").get_active()
          local Notifications = require("lgtm.ui.notifications")
          local lines = vim.api.nvim_buf_get_lines(pane.split.bufnr, 0, -1, false)
          local pr_threads = services.pr_service:get_threads()
          local local_threads = services.local_service:get_threads()
          return {
            prThreadCount = #pr_threads,
            localThreadCount = #local_threads,
            prBody = pr_threads[1] and pr_threads[1].comments[1].body or "",
            paneText = table.concat(lines, "\\n"),
            lastEvent = (Notifications.last() or {}).event or "",
          }
        `)
        expect(prThreadState.prThreadCount).toBe(1)
        expect(prThreadState.localThreadCount).toBe(1)
        expect(prThreadState.prBody).toBe('Use immutable signal update in PR mode')
        expect(prThreadState.paneText).toContain('PR Review Threads (1)')
        expect(prThreadState.paneText).toContain('Use immutable signal update in PR mode')
        expect(prThreadState.paneText).not.toContain('Local-only note that stays in local mode')
        expect(prThreadState.lastEvent).toBe('thread_created')
      })

      it('should_submit_overall_pr_review_via_pr_submit_editor_with_submit_and_cancel_buttons', async () => {
        // Arrange
        const { editor, runtime } = fixture()
        await seedWorkspacePullRequests(runtime, editor)
        await editor.keys(' rp')
        await editor.wait(
          'require("lgtm.features.pr_select_modal").is_open()',
          'pr-select-modal opened',
        )
        await editor.lua('require("lgtm.features.pr_select_modal").confirm_selection()')
        await editor.wait('require("lgtm").get_mode() == "pr"', 'PR mode active')

        // Act: open pr-submit-editor with <leader>rs, verify Submit/Cancel buttons, then cancel
        await editor.keys(' rs')
        await editor.wait(
          'require("lgtm.features.pr_submit_editor").is_open()',
          'pr-submit-editor opened',
        )
        await editor.wait('#vim.api.nvim_list_wins() >= 4', 'pr-submit-editor layout mounted')
        const submitModalDiagnostics = await editor.diagnostics()
        await editor.lua('require("lgtm.features.pr_submit_editor").cancel()')
        await editor.wait(
          'not require("lgtm.features.pr_submit_editor").is_open()',
          'pr-submit-editor closed on cancel',
        )

        // Assert: cancel left submissions empty
        expect(submitModalDiagnostics.screen).toContain('Submit Review')
        expect(submitModalDiagnostics.screen).toContain('Cancel')
        expect(
          await editor.value<number>(
            '#require("lgtm").get_services().pr_service.state:get_value().submissions',
          ),
        ).toBe(0)

        // Act: open pr-submit-editor for Approve (<leader>ra), enter review summary, and submit
        await editor.keys(' ra')
        await editor.wait(
          'require("lgtm.features.pr_submit_editor").is_open()',
          'pr-submit-editor reopened with approve outcome',
        )
        await editor.lua(
          'require("lgtm.features.pr_submit_editor").set_body("LGTM - clean UI scaffolding and signal isolation")',
        )
        await editor.lua('require("lgtm.features.pr_submit_editor").submit()')
        await editor.wait(
          'not require("lgtm.features.pr_submit_editor").is_open()',
          'pr-submit-editor closed after submit',
        )

        // Assert
        const submissionState = await editor.lua<{
          count: number
          outcome: string
          body: string
          lastEvent: string
          lastMessage: string
        }>(`
          local pr_service = require("lgtm").get_services().pr_service
          local Notifications = require("lgtm.ui.notifications")
          local submissions = pr_service.state:get_value().submissions
          local last = Notifications.last() or {}
          return {
            count = #submissions,
            outcome = submissions[1] and submissions[1].outcome or "",
            body = submissions[1] and submissions[1].body or "",
            lastEvent = last.event or "",
            lastMessage = last.message or "",
          }
        `)
        expect(submissionState).toEqual({
          count: 1,
          outcome: 'approve',
          body: 'LGTM - clean UI scaffolding and signal isolation',
          lastEvent: 'review_submitted',
          lastMessage: 'Submitted Approve review on PR #42',
        })
      })
    },
  )
}
