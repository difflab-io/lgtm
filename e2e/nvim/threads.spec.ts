/**
 * @module-tag e2e
 * @module-tag nvim
 * @module-tag threads
 */
import { describe, it, expect } from 'vitest'
import { profiles, useEditor } from './fixtures.js'
import { usePrEditor } from './pr-review.fixtures.js'
import {
  threadFlows,
  prepareThread,
  getThreadState,
  getThreadPlacement,
  openThreadsPane,
  changeThreadSource,
} from './threads.fixtures.js'
import { prepareWorkingFeedback, prepareOrphanedWorkingFeedback } from './local-review.fixtures.js'
import { waitFor, type Json } from '../common/test-utils.js'
import type { Editor } from './editor.js'

async function placement(editor: Editor, id: string) {
  return editor.lua(
    `local id=...; local s=require("lgtm.context").current
    for _,t in ipairs(s.conversation.threads) do
      if t.id==id then return require("lgtm.threads").placement(s,t,s.file.path,"new") end
    end`,
    id,
  )
}

for (const profile of profiles) {
  for (const flow of threadFlows) {
    describe(
      `When discussing ${flow} threads [${profile}]`,
      {
        tags: [
          profile,
          'threads',
          ...(flow === 'forge'
            ? ['github', 'pr-review']
            : [flow === 'notes' ? 'notes' : 'local-review']),
        ],
      },
      () => {
        const fixture = flow === 'forge' ? usePrEditor(profile) : useEditor(profile)

        it('Resolving and reopening refreshes the same threads pane and preserves its selection', async () => {
          // Arrange
          const discussion = await prepareThread(flow, fixture())
          const { editor, github } = discussion
          const panel = await openThreadsPane(discussion)

          for (const resolved of [true, false]) {
            // Act
            await editor.keys('x')
            const state = await waitFor(
              resolved ? 'thread resolved' : 'thread reopened',
              () => getThreadState(discussion),
              (state) =>
                state.conversation.threads.find((thread: Json) => thread.id === discussion.threadId)
                  ?.state === (resolved ? 'resolved' : 'open'),
              editor.timeoutMs,
            )

            // Assert
            expect(await editor.value('vim.api.nvim_get_current_win()')).toBe(panel.win)
            expect(await editor.value('vim.api.nvim_win_get_buf(...)', panel.win)).toBe(panel.buf)
            expect(await editor.value('vim.api.nvim_win_get_cursor(...)', panel.win)).toEqual(
              panel.cursor,
            )
            expect(await editor.text(panel.buf)).toContain(resolved ? 'RESOLVED' : 'OPEN')
            expect(await editor.text(panel.buf)).toContain('Thread concern')
            expect(await editor.text(panel.buf)).toContain('Thread reply')
            expect(await editor.text(panel.buf)).toContain('1 reply')
            if (github) {
              const thread = state.conversation.threads.find(
                (thread: Json) => thread.id === discussion.threadId,
              )
              const remote = await github.api('graphql', 'POST', {
                query: 'query($id:ID!){node(id:$id){... on PullRequestReviewThread{isResolved}}}',
                variables: { id: thread.forgeThreadId },
              })
              expect(remote.data.node.isResolved).toBe(resolved)
            }
          }
        })

        it('A lost anchor keeps the card compact and preserves evidence in the full thread view', async () => {
          // Arrange
          const discussion = await prepareThread(flow, fixture())
          const { editor } = discussion

          // Act
          await changeThreadSource(discussion)
          const marks =
            flow === 'notes'
              ? await editor.value(
                  'vim.api.nvim_buf_get_extmarks(...,require("lgtm.threads").namespace,0,-1,{details=true})',
                  discussion.buf,
                )
              : await editor.marks()

          // Assert
          const label = flow === 'local' ? 'Stale' : 'Outdated'
          expect((await getThreadPlacement(discussion)).location).toBeUndefined()
          expect(marks).toHaveLength(1)
          expect(marks[0][1]).toBe(0)
          expect(marks[0][3].virt_lines_above).toBe(true)
          expect(marks[0][3].virt_lines).toHaveLength(3)
          for (const text of [label, 'anchor lost', 'Thread concern'])
            expect(JSON.stringify(marks)).toContain(text)
          for (const text of ['Original lines', '> ' + discussion.excerpt])
            expect(JSON.stringify(marks)).not.toContain(text)
          expect(
            (await getThreadState(discussion)).conversation.threads.find(
              (thread: Json) => thread.id === discussion.threadId,
            ).anchor,
          ).toEqual(discussion.anchor)

          // Act
          if (flow === 'notes') await editor.lua('vim.api.nvim_win_set_cursor(0,{1,0})')
          else await editor.pane('new', 1)
          await editor.popup('Thread concern')

          // Assert
          for (const text of [
            label.toUpperCase(),
            'Original lines 2',
            discussion.excerpt,
            'Thread concern',
            'Thread reply',
          ])
            expect(await editor.text()).toContain(text)
        })
      },
    )
  }

  describe(
    `When navigating working-tree threads [${profile}]`,
    { tags: [profile, 'threads', 'local-review'] },
    () => {
      const fixture = useEditor(profile)
      it('Resolving and reopening a stale working-tree thread refreshes the threads pane without closing it', async () => {
        // Arrange
        const { editor: e, runtime: r } = fixture()
        const { workingId } = await prepareOrphanedWorkingFeedback(e, r)
        await e.command('Lgtm threads')
        const panel = await e.lua(
          `local id = ...; local panel = require("lgtm.threads").last
        for _, entry in ipairs(panel.entries) do
          if entry.thread.id == id then
            vim.api.nvim_win_set_cursor(panel.win, {entry.line, 0})
            return {win = panel.win, buf = panel.buf, cursor = vim.api.nvim_win_get_cursor(panel.win)}
          end
        end
        error("Working-tree thread not found")`,
          workingId,
        )

        // Act
        await e.keys('x')
        await waitFor(
          'stale working-tree thread resolved',
          () => e.state(),
          (state) =>
            state?.conversation.threads.find((thread: Json) => thread.id === workingId)?.state ===
            'resolved',
        )

        // Assert
        expect(await e.value('vim.api.nvim_get_current_win()')).toBe(panel.win)
        expect(await e.value('vim.api.nvim_win_get_buf(...)', panel.win)).toBe(panel.buf)
        expect(await e.value('vim.api.nvim_win_get_cursor(...)', panel.win)).toEqual(panel.cursor)
        expect(await e.text(panel.buf)).toContain('RESOLVED')
        expect(await e.text(panel.buf)).toContain('Retain this unstaged feedback')
        expect(await e.text(panel.buf)).toContain('STALE')

        // Act
        await e.keys('x')
        await waitFor(
          'stale working-tree thread reopened',
          () => e.state(),
          (state) =>
            state?.conversation.threads.find((thread: Json) => thread.id === workingId)?.state ===
            'open',
        )

        // Assert
        expect(await e.value('vim.api.nvim_get_current_win()')).toBe(panel.win)
        expect(await e.value('vim.api.nvim_win_get_buf(...)', panel.win)).toBe(panel.buf)
        expect(await e.value('vim.api.nvim_win_get_cursor(...)', panel.win)).toEqual(panel.cursor)
        expect(await e.text(panel.buf)).not.toContain('RESOLVED')
        expect(await e.text(panel.buf)).toContain('Retain this unstaged feedback')
      })

      it.each(['exact', 'stale'])(
        'Navigating to %s working-tree feedback preserves the threads pane',
        async (status) => {
          // Arrange
          const { editor: e, runtime: r } = fixture()
          const { workingId } =
            status === 'stale'
              ? await prepareOrphanedWorkingFeedback(e, r)
              : await prepareWorkingFeedback(e, r)
          await r.write('second.txt', 'Another changed file\n')
          await e.command('Lgtm refresh')
          await waitFor(
            'second changed file available',
            () => e.state(),
            (state) => state?.files.some((file: Json) => file.path === 'second.txt'),
          )
          await e.show('second.txt')
          await e.command('Lgtm threads')
          const panel = await e.lua(
            `local id = ...; local panel = require("lgtm.threads").last
          for _, entry in ipairs(panel.entries) do
            if entry.thread.id == id then
              vim.api.nvim_win_set_cursor(panel.win, {entry.line, 0})
              return {win = panel.win, buf = panel.buf, cursor = vim.api.nvim_win_get_cursor(panel.win)}
            end
          end
          error("Working-tree thread not found")`,
            workingId,
          )
          const discussion = await e.text(panel.buf)

          // Act
          await e.keys('<CR>')
          const { state } = await waitFor(
            'thread file selected and code focused',
            async () => ({
              state: await e.state(),
              win: await e.value('vim.api.nvim_get_current_win()'),
            }),
            ({ state, win }) =>
              Boolean(
                state?.render_ready &&
                state.file?.path === 'other.txt' &&
                win === state.panes.new.win,
              ),
          )

          // Assert
          expect(await e.value('vim.api.nvim_win_is_valid(...)', panel.win)).toBe(true)
          expect(await e.value('vim.api.nvim_win_get_buf(...)', panel.win)).toBe(panel.buf)
          expect(await e.value('vim.api.nvim_win_get_cursor(...)', panel.win)).toEqual(panel.cursor)
          expect(await e.text(panel.buf)).toBe(discussion)
          expect((await placement(e, workingId)).status).toBe(status)
          if (status === 'exact')
            expect(await e.value('vim.api.nvim_win_get_cursor(...)', state!.panes.new.win)).toEqual(
              [1, 0],
            )
        },
      )

      it('A file leaving and rejoining the working-tree diff updates the threads pane status', async () => {
        // Arrange
        const { editor: e, runtime: r } = fixture()
        await r.git('init', '-q', '-b', 'main')
        await r.write('review.txt', 'original\n')
        await r.write('other.txt', 'original\n')
        await r.commit('base')
        await r.write('review.txt', 'reviewed change\n')
        await r.write('other.txt', 'another change\n')
        await e.openLocal('working')
        await e.show('review.txt')
        await e.pane()
        await e.keys('c')
        await e.compose('Keep this discussion outside the diff')
        const original = (await e.comments(1))!.conversation.threads[0]

        // Act
        await r.write('review.txt', 'original\n')
        await e.command('Lgtm refresh')
        await waitFor(
          'reviewed file outside diff',
          () => e.state(),
          (state) =>
            state?.render_ready && !state.files.some((file: Json) => file.path === 'review.txt'),
        )
        await e.show('other.txt')
        await e.command('Lgtm threads')
        const panel = await e.value('require("lgtm.threads").last')

        // Assert
        expect(await e.text(panel.buf)).toContain('OUTSIDE DIFF')
        expect(await e.text(panel.buf)).toContain('Keep this discussion outside the diff')
        expect(await e.text(panel.buf)).not.toContain('ORPHANED')
        expect((await e.state())!.conversation.threads[0].state).toBe('open')
        expect((await e.state())!.conversation.threads[0].anchor).toEqual(original.anchor)

        // Act
        await e.keys('<CR>')

        // Assert
        expect(await e.value('vim.api.nvim_get_current_win()')).toBe(panel.win)
        expect(await e.value('vim.api.nvim_win_is_valid(...)', panel.win)).toBe(true)
        expect(
          (await e.notices()).some((notice) => notice.message.includes('outside the current diff')),
        ).toBe(true)

        // Act
        await e.keys('q')
        await r.write('review.txt', 'reviewed change\n')
        await e.command('Lgtm refresh')
        await waitFor(
          'reviewed file back in diff',
          () => e.state(),
          (state) => state?.files.some((file: Json) => file.path === 'review.txt'),
        )
        await e.show('review.txt')
        await e.command('Lgtm threads')

        // Assert
        expect(await e.text()).not.toContain('OUTSIDE DIFF')
        expect(await e.text()).toContain('Keep this discussion outside the diff')
        expect((await placement(e, original.id)).status).toBe('exact')
      })

      it('Navigating to a stale working-tree thread reveals its card after scrolling down the file', async () => {
        // Arrange
        const { editor: e, runtime: r } = fixture()
        const { workingId } = await prepareOrphanedWorkingFeedback(e, r)
        const content = 'changed working selection\n' + 'another line\n'.repeat(120)
        await r.write('other.txt', content)
        await e.command('Lgtm refresh')
        await waitFor('long working file rendered', () =>
          e.lua(
            `local s = require("lgtm.context").current
              return s.render_ready and s.views.new and s.views.new.content == ...
                and vim.api.nvim_buf_line_count(s.panes.new.buf) > 100`,
            content,
          ),
        )
        await e.pane('new', 100)
        await e.command('Lgtm threads')
        const panel = await e.lua(
          `local id = ...; local panel = require("lgtm.threads").last
        for _, entry in ipairs(panel.entries) do
          if entry.thread.id == id then
            vim.api.nvim_win_set_cursor(panel.win, {entry.line, 0})
            return {win = panel.win, buf = panel.buf}
          end
        end
        error("Working-tree thread not found")`,
          workingId,
        )
        expect(
          await e.lua(
            'local win = require("lgtm.context").current.panes.new.win; return vim.api.nvim_win_call(win, vim.fn.winsaveview).topline',
          ),
        ).toBeGreaterThan(1)

        // Act
        await e.keys('<CR>')
        const screen = await waitFor(
          'stale card visible in the code window',
          () =>
            e.lua(
              `vim.cmd("redraw")
          local win = require("lgtm.context").current.panes.new.win
          local info = vim.fn.getwininfo(win)[1]
          local lines = {}
          for row = info.winrow, info.winrow + info.height - 1 do
            local cells = {}
            for col = info.wincol, info.wincol + info.width - 1 do
              cells[#cells + 1] = vim.fn.screenstring(row, col)
            end
            lines[#lines + 1] = table.concat(cells)
          end
          return table.concat(lines, "\\n")`,
            ),
          (text) => text.includes('Stale') && text.includes('Retain this'),
        )

        // Assert
        expect(screen).toContain('Stale')
        expect(screen).toContain('Retain this')
        expect(screen).not.toContain('Original lines')
        expect(screen).not.toContain('Unstaged source changed')
        expect(await e.value('vim.api.nvim_win_is_valid(...)', panel.win)).toBe(true)
        expect(await e.value('vim.api.nvim_get_current_win()')).toBe(
          (await e.state())!.panes.new.win,
        )
        expect(await e.value('vim.api.nvim_win_get_cursor(0)')).toEqual([1, 0])
      })
    },
  )
}
