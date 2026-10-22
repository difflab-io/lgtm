/**
 * @module-tag e2e
 * @module-tag nvim
 * @module-tag diffs
 */
import { join } from 'node:path'
import { describe, it, expect } from 'vitest'
import { profiles, useEditor } from './fixtures.js'
import { type TestHarness } from '../common/test-harness.js'
import { waitFor } from '../common/test-utils.js'
import { expectFileSeen } from './assertions.js'

async function prepareNativeCheckout(runtime: TestHarness) {
  await runtime.git('init', '-q', '-b', 'main')
  await runtime.write('review.txt', 'old\n')
  await runtime.write('src/lib.rs', 'pub fn old() {}\n// second line\n// third line\n')
  const base = await runtime.commit('base')
  await runtime.git('checkout', '-qb', 'feature')
  await runtime.write('review.txt', 'new\n')
  await runtime.write(
    'src/lib.rs',
    'pub fn new() {}\n// second line\n// inserted context\n// third line\n',
  )
  const head = await runtime.commit('head')
  return { base, head }
}

for (const profile of profiles) {
  describe(`When opening a local diff [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)

    it(
      'A branch review opens native revision buffers and its checkmark can be toggled',
      { tags: profile === 'required' ? ['smoke'] : [] },
      async () => {
        // Arrange
        const { editor, runtime } = fixture()
        const { base, head } = await prepareNativeCheckout(runtime)

        // Act
        const state = await editor.openLocal('branch')
        await editor.pane()
        await editor.keys(' rm')
        await expectFileSeen(editor, state.file.path, true)
        await editor.keys(' rm')
        await expectFileSeen(editor, state.file.path, false)
        await editor.command('Lgtm complete')
        await editor.wait(
          'require("lgtm.context").current.review.state == "completed"',
          'branch completed',
        )

        // Assert
        expect(state).toMatchObject({ kind: 'git', base, head })
        expect(state.native_explorer).toBe(true)
        expect(
          await editor.value(
            'vim.api.nvim_buf_get_name(...)',
            (await editor.state())!.panes.new.buf,
          ),
        ).toMatch(/^codediff:\/\//)
      },
    )

    it.each(['working', 'staged'])(
      'An empty %s comparison reports its state and remains completable',
      async (mode) => {
        // Arrange
        const { editor, runtime } = fixture()
        await prepareNativeCheckout(runtime)

        // Act
        const state = await editor.openLocal(mode)
        const notices = await editor.notices()
        await editor.command('Lgtm complete')
        await editor.wait(
          'require("lgtm.context").current.review.state == "completed"',
          'empty review completed',
        )

        // Assert
        expect(state.files ?? []).toHaveLength(0)
        expect(
          notices.some(
            (notice) =>
              notice.message ===
              (mode === 'working' ? 'No working-tree changes' : 'No staged changes'),
          ),
        ).toBe(true)
      },
    )
  })

  describe(`When marking local files as reviewed [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)

    it('Old-side working comments and replies remain attached when switching CodeDiff layouts', async () => {
      // Arrange
      const { editor, runtime } = fixture()
      await prepareNativeCheckout(runtime)
      await runtime.write('review.txt', 'working replacement\n')
      await editor.openLocal('working')
      await editor.show('review.txt')
      await editor.pane('old', 1)

      // Act
      await editor.keys('c')
      await editor.compose('Explain original source')
      await editor.comments(1)
      await editor.popup('Explain original source')
      await editor.keys('c')
      await editor.compose('Old-side reply')
      await editor.comments(2)
      await editor.command('Lgtm layout')
      await editor.wait(
        'require("lgtm.context").current.layout == "inline"',
        'inline CodeDiff layout',
      )
      await editor.command('Lgtm layout')
      await editor.wait(
        'require("lgtm.context").current.layout == "side-by-side"',
        'side-by-side CodeDiff layout',
      )
      const state = await editor.loaded()

      // Assert
      expect(state.conversation.threads[0].anchor.side).toBe('old')
      expect(state.conversation.comments.map((comment: { body: string }) => comment.body)).toEqual([
        'Explain original source',
        'Old-side reply',
      ])
      expect(state.native_explorer).toBe(true)
    })

    it('Working-file marks survive restart and changes invalidate the checkmark', async () => {
      // Arrange
      const { editor, runtime } = fixture()
      await prepareNativeCheckout(runtime)
      await runtime.write('review.txt', 'unstaged\n')
      await editor.openLocal('working')
      await editor.show('review.txt')
      await editor.pane()
      await editor.keys(' rm')
      await expectFileSeen(editor, 'review.txt', true)
      await editor.keys(' rm')
      await expectFileSeen(editor, 'review.txt', false)
      await editor.keys(' rm')

      await editor.wait(
        'next(require("lgtm.context").current.reviewed_pending or {}) == nil',
        'file mark saved by backend',
      )

      // Act
      await editor.close()
      await runtime.restart()
      await editor.start()
      const state = await editor.openLocal('working')
      await editor.show('review.txt')
      await editor.wait(
        'require("lgtm.context").current.reviewed["review.txt"] ~= nil',
        'persisted local file mark',
      )

      // Assert
      await expectFileSeen(editor, 'review.txt', true)
      expect((await editor.notices()).some((notice) => /File marked/.test(notice.message))).toBe(
        false,
      )

      // Act
      await runtime.write('review.txt', 'working content changed after marking seen\n')
      await editor.command('Lgtm refresh')
      await editor.wait(
        'require("lgtm.context").current.render_ready and not require("lgtm.context").current.reviewed["review.txt"]',
        'working checkmark invalidated',
      )

      // Assert
      expect(await editor.text(state.panes.new.buf)).toContain('working content changed')
      await expectFileSeen(editor, 'review.txt', false)
    })

    it.each(['working', 'staged'])(
      'A changed %s file loses its checkmark while another file keeps it',
      async (mode) => {
        // Arrange
        const { editor, runtime } = fixture()
        await prepareNativeCheckout(runtime)
        await runtime.write('review.txt', 'reviewed content\n')
        await runtime.write('src/lib.rs', 'pub fn hidden_change() {}\n')
        if (mode === 'staged') await runtime.git('add', 'review.txt', 'src/lib.rs')
        const state = await editor.openLocal(mode)
        for (const path of ['review.txt', 'src/lib.rs']) {
          await editor.show(path)
          await editor.pane()
          await editor.keys(' rm')
          await expectFileSeen(editor, path, true)
        }
        await editor.show('review.txt')

        // Act
        await runtime.write('src/lib.rs', 'pub fn changed_after_seen() {}\n')
        if (mode === 'staged') await runtime.git('add', 'src/lib.rs')
        await editor.command('Lgtm refresh')
        await editor.wait(
          'require("lgtm.context").current.render_ready and not require("lgtm.context").current.reviewed["src/lib.rs"]',
          'hidden file checkmark invalidated',
        )

        // Assert
        expect(state.kind).toBe('git')
        expect((await editor.state())!.files).toHaveLength(2)
        if (mode === 'staged')
          expect(
            await editor.value(
              'vim.api.nvim_buf_get_name(...)',
              (await editor.state())!.panes.new.buf,
            ),
          ).toMatch(/^codediff:\/\//)
        await expectFileSeen(editor, 'src/lib.rs', false)
        await expectFileSeen(editor, 'review.txt', true)
      },
    )
  })

  describe(
    `When a working buffer is shared with ordinary editing [${profile}]`,
    { tags: [profile] },
    () => {
      const fixture = useEditor(profile)

      it('Ordinary tabs keep note cards and original keys while review tabs keep review comments and keys', async () => {
        // Arrange
        const { editor, runtime } = fixture()
        await prepareNativeCheckout(runtime)
        await runtime.write('review.txt', 'working edit\n')
        await editor.edit(join(runtime.workspace, 'review.txt'))
        await editor.command('Lgtm note')
        await editor.compose('Standalone note outside the review')
        const mapping = await editor.value('vim.fn.maparg("c","n",false,true).desc')
        const state = await editor.openLocal('working')
        await editor.show('review.txt')
        const reviewTab = state.tab
        const buffer = state.panes.new.buf

        // Act
        await editor.command(`tabnew ${join(runtime.workspace, 'review.txt')}`)
        const ordinary = {
          buffer: await editor.value('vim.api.nvim_get_current_buf()'),
          mapping: await editor.value('vim.fn.maparg("c","n",false,true).desc'),
          review: await editor.value('vim.b.lgtm'),
        }
        await editor.wait(
          'vim.inspect(vim.api.nvim_buf_get_extmarks(0,require("lgtm.threads").namespace,0,-1,{details=true})):find("Standalone note outside",1,true) ~= nil',
          'ordinary note card displayed',
        )
        await editor.lua('vim.api.nvim_exec_autocmds("ColorScheme",{modeline=false})')
        await editor.command('tabclose')
        await waitFor(
          'review keys restored',
          () =>
            editor.lua<string>(
              'return vim.api.nvim_buf_call(...,function() return vim.fn.maparg("c","n",false,true).desc end)',
              buffer,
            ),
          (value) => value === 'Add review comment',
        )

        // Assert
        expect(ordinary.buffer).toBe(buffer)
        expect(ordinary.mapping).toBe(mapping)
        expect(ordinary.review).toBeFalsy()
        expect(await editor.value('vim.api.nvim_get_current_tabpage()')).toBe(reviewTab)
        expect(JSON.stringify(await editor.marks())).not.toContain('Standalone note outside')
      })
    },
  )

  describe(`When changing the saved comparison [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)

    it('User can resume the saved comparison or explicitly replace it with the current branch comparison', async () => {
      // Arrange
      const { editor, runtime } = fixture()
      const { base } = await prepareNativeCheckout(runtime)
      await runtime.write('review.txt', 'staged replacement\n')
      await runtime.git('add', 'review.txt')
      const staged = await editor.openLocal('staged')
      const saved = staged.review
      await editor.command('Lgtm close')
      await runtime.git(
        '-c',
        'user.name=E2E',
        '-c',
        'user.email=e2e@example.test',
        'commit',
        '-qm',
        'another change',
      )
      const head = await runtime.git('rev-parse', 'HEAD')

      // Act
      await editor.choose('branch', 'Resume saved review')
      await editor.command('Lgtm review')
      const resumed = await editor.loaded()
      await editor.command('Lgtm close')
      const replaced = await editor.openLocal('branch', true)
      await editor.command('Lgtm complete')
      await editor.wait(
        'require("lgtm.context").current.review.state == "completed"',
        'replacement completed',
      )

      // Assert
      expect(resumed.review.id).toBe(saved.id)
      expect(resumed.comparison).toEqual(saved.comparison)
      expect(replaced.review.id).not.toBe(saved.id)
      expect(replaced).toMatchObject({ base, head })
    })
  })

  describe(`When the last working change is restored [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)

    it('Refresh clears the selected file and stale source observers while keeping the native diff session open', async () => {
      // Arrange
      const { editor, runtime } = fixture()
      await prepareNativeCheckout(runtime)
      await runtime.write('review.txt', 'temporary working edit\n')
      await editor.openLocal('working')

      // Act
      await runtime.git('restore', 'review.txt')
      await editor.command('Lgtm refresh')
      await editor.wait(
        'require("lgtm.context").current.empty and #require("lgtm.context").current.files == 0 and require("lgtm.context").current.file == nil',
        'last change restored',
      )

      // Assert
      expect(await editor.value('vim.tbl_isempty(require("lgtm.context").current.views)')).toBe(
        true,
      )
      expect(await editor.value('require("lgtm.context").current.observe_sources ~= nil')).toBe(
        false,
      )
      expect(
        await editor.value(
          'require("codediff.ui.lifecycle").get_session(require("lgtm.context").current.tab) ~= nil',
        ),
      ).toBe(true)
    })
  })
}
