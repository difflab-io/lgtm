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
import { prepareWorkingFeedback } from './local-review.fixtures.js'
import { waitFor, type Json } from '../common/test-utils.js'

for (const profile of profiles) {
  for (const flow of threadFlows) {
    describe(
      `When discussing ${flow} threads [${profile}]`,
      {
        tags: [
          profile,
          'threads',
          ...(flow === 'forge' ? ['github', 'pr-review'] : ['local-review']),
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

        if (flow === 'forge') {
          it('A lost anchor keeps the card compact and preserves evidence in the full thread view', async () => {
            // Arrange
            const discussion = await prepareThread(flow, fixture())
            const { editor } = discussion

            // Act
            await changeThreadSource(discussion)
            const marks = await editor.marks()

            // Assert
            expect((await getThreadPlacement(discussion)).location).toBeUndefined()
            expect(marks).toHaveLength(1)
            expect(marks[0][1]).toBe(0)
            for (const text of ['Outdated', 'anchor lost', 'Thread concern'])
              expect(JSON.stringify(marks)).toContain(text)
            expect(
              (await getThreadState(discussion)).conversation.threads.find(
                (thread: Json) => thread.id === discussion.threadId,
              ).anchor,
            ).toEqual(discussion.anchor)

            // Act
            await editor.pane('new', 1)
            await editor.popup('Thread concern')

            // Assert
            for (const text of [
              'OUTDATED',
              'Original lines 2',
              discussion.excerpt,
              'Thread concern',
              'Thread reply',
            ])
              expect(await editor.text()).toContain(text)
          })
        } else {
          it('Changing the workspace preserves the saved card and full thread discussion', async () => {
            // Arrange
            const discussion = await prepareThread(flow, fixture())
            const { editor } = discussion

            // Act
            await changeThreadSource(discussion)
            await editor.pane('new', 2)
            await editor.popup('Thread concern')

            // Assert
            expect(await getThreadPlacement(discussion)).toMatchObject({
              status: 'exact',
              location: { start: 2 },
            })
            for (const text of ['Thread concern', 'Thread reply'])
              expect(await editor.text()).toContain(text)
            expect(
              (await getThreadState(discussion)).conversation.threads.find(
                (thread: Json) => thread.id === discussion.threadId,
              ).anchor,
            ).toEqual(discussion.anchor)
          })
        }
      },
    )
  }

  describe(
    `When navigating saved revisions [${profile}]`,
    { tags: [profile, 'threads', 'local-review'] },
    () => {
      const fixture = useEditor(profile)
      it('Enter opens a historical thread at its original saved line after refresh', async () => {
        // Arrange
        const { editor, runtime } = fixture()
        const { workingId } = await prepareWorkingFeedback(editor, runtime)
        const first = (await editor.state())!
        await runtime.write('other.txt', 'replacement saved source\n')
        await editor.command('Lgtm refresh')
        await waitFor(
          'new revision ready',
          () => editor.state(),
          (state) => Boolean(state?.render_ready && state.snapshot.id !== first.snapshot.id),
        )
        await editor.command('Lgtm threads')
        await editor.lua(
          `local id=...; local panel=require("lgtm.features.threads_pane").last
        for _,entry in ipairs(panel.entries) do if entry.thread.id==id then vim.api.nvim_win_set_cursor(panel.win,{entry.line,0}); return end end`,
          workingId,
        )

        // Act
        await editor.keys('<CR>')
        await waitFor(
          'historical revision ready',
          () => editor.state(),
          (state) => Boolean(state?.render_ready && state.snapshot.id === first.snapshot.id),
        )
        const historical = (await editor.state())!

        // Assert
        expect(historical.review.id).toBe(first.review.id)
        expect(await editor.text(historical.panes.new.buf)).toContain('working selection')
        expect(JSON.stringify(await editor.marks())).toContain('Retain this uns')
        expect(await editor.value('vim.api.nvim_win_get_cursor(0)[1]')).toBe(1)
      })
    },
  )
}
