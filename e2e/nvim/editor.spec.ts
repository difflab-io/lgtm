import { isolatedEnv } from '../common/test-utils.js'
/**
 * @module-tag e2e
 * @module-tag nvim
 */
import { describe, it, expect } from 'vitest'
import { cp, mkdir, mkdtemp, readFile, readlink, rm, symlink, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { command, ROOT } from '../common/test-harness.js'
import { profiles, useEditor } from './fixtures.js'
import { waitFor } from '../common/test-utils.js'

for (const profile of profiles) {
  describe(`When checking editor dependencies [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)
    it('Health reports the installed native diff library and a reachable compatible backend', async () => {
      // Arrange
      const { editor: e } = fixture()
      // Act
      await e.command('checkhealth lgtm')

      // Assert
      const report = await waitFor(
        'health response',
        () => e.text(),
        (text) => text.includes('Backend reachable, API 1'),
      )
      expect(report).toContain('CodeDiff native library loaded')
      expect(report).not.toContain('ERROR')
    })
  })
  describe(`When cancelling backend requests [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)
    it('Cancelled requests do not call their callbacks and later requests still succeed', async () => {
      // Arrange
      const { editor: e } = fixture()
      // Act
      await e.lua(`local t=require("lgtm.services.connect"); _G.e2e.callbacks=0
          local function done() _G.e2e.callbacks=_G.e2e.callbacks+1 end
          local cancel=t.call("IdentityService","GetCapabilities",{},nil,done); cancel()
          t.call("IdentityService","GetCapabilities",{},nil,done)
          t.call("IdentityService","GetCapabilities",{},nil,done); t.cancel_all()
          _G.e2e.cancel_settled=false; vim.defer_fn(function() _G.e2e.cancel_settled=true end,250)`)

      // Assert
      await e.wait('_G.e2e.cancel_settled', 'cancelled curl processes settled')
      expect(await e.value('_G.e2e.callbacks')).toBe(0)

      // Act
      await e.lua(`require("lgtm.services.connect").call("IdentityService","GetCapabilities",{},nil,function(err,result)
          _G.e2e.transport_result={error=err,result=result} end)`)

      // Assert
      await e.wait('_G.e2e.transport_result ~= nil', 'later real request')
      expect(await e.value('_G.e2e.transport_result.error')).toBeNull()
      expect(await e.value('_G.e2e.transport_result.result.apiVersion')).toBe('1')
    })
  })
  describe(`When applying the notification theme [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)
    it('should_preserve_notification_messages_and_user_highlight_overrides', async () => {
      // Arrange
      const { editor: e } = fixture()
      await e.lua('vim.api.nvim_set_hl(0,"LgtmInfo",{link="Special"})')

      // Act
      await e.lua(
        'require("lgtm.ui.notifications").show("Saved UI evidence"); require("lgtm.ui.theme").highlights()',
      )

      // Assert
      expect((await e.notices()).at(-1)?.message).toBe('Saved UI evidence')
      expect(await e.value('vim.api.nvim_get_hl(0,{name="LgtmInfo",link=true}).link')).toBe(
        'Special',
      )
    })
  })

  describe(`When saving a draft [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)
    it('should_retain_failed_draft_without_duplicate_save', async () => {
      // Arrange
      const { editor: e } = fixture()
      await e.lua(`_G.e2e.saves={}
        require("lgtm.ui.composer").open("Draft retry","",function(body,done)
          table.insert(_G.e2e.saves,body); _G.e2e.finish_save=done
        end)`)
      const draft = await e.composer()
      await e.body('Keep this exact draft\nWith a second line')

      // Act
      await e.keys('<C-s>')
      await e.lua('require("lgtm.ui.composer").last.save()')

      // Assert
      expect(await e.value('#_G.e2e.saves')).toBe(1)

      // Act
      await e.lua('_G.e2e.finish_save({message="Save rejected"})')
      await e.composer()

      // Assert
      expect(await e.text(draft.buf)).toBe('Keep this exact draft\nWith a second line')
      expect(await e.value('require("lgtm.ui.composer").last.closed')).toBe(false)

      // Act
      await e.keys('<C-s>')
      await e.lua('_G.e2e.finish_save(nil)')

      // Assert
      expect(await e.value('_G.e2e.saves')).toEqual([
        'Keep this exact draft\nWith a second line',
        'Keep this exact draft\nWith a second line',
      ])
      expect(await e.value('require("lgtm.ui.composer").last.closed')).toBe(true)
    })
  })
  describe(`When cancelling an empty search [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)
    it('The session picker shows no matches and Escape closes it without reopening a review', async () => {
      // Arrange
      const { editor: e, runtime: r } = fixture()
      await r.git('init', '-q', '-b', 'main')
      await r.write('review.txt', 'base\n')
      await r.commit('base')
      await r.write('review.txt', 'working edit\n')
      await e.openLocal('working')
      await e.command('Lgtm close')

      // Act
      await e.command('Lgtm sessions')
      await e.wait(
        'require("lgtm.ui.search").last and vim.api.nvim_get_current_buf() == require("lgtm.ui.search").last.buf and vim.fn.mode():sub(1,1) == "i"',
        'session picker',
      )
      await e.keys('not present')
      await e.wait('#require("lgtm.ui.search").last.filtered == 0', 'empty picker result')
      const text = await e.value(
        'vim.api.nvim_buf_get_lines(require("lgtm.ui.search").last.list_buf,0,1,false)[1]',
      )
      await e.keys('<Esc>')

      // Assert
      expect(text).toBe('(no matches)')
      expect(await e.value('require("lgtm.ui.search").last.closed')).toBe(true)
      expect(await e.state()).toBeNull()
    })
  })
}

describe('When installing the monorepo as a native Neovim package', { tags: ['required'] }, () => {
  it('The documented setup loads the plugin command and discovers its help from the package root', async () => {
    // Arrange
    const home = await mkdtemp(join(tmpdir(), 'lgtm-package-'))
    try {
      const env = isolatedEnv(home)
      const root = join(env.XDG_DATA_HOME!, 'nvim/site/pack/plugins/start/lgtm')
      await mkdir(join(root, 'apps'), { recursive: true })
      await cp(join(ROOT, 'apps/lgtm.nvim'), join(root, 'apps/lgtm.nvim'), { recursive: true })
      await symlink(await readlink(join(ROOT, 'doc')), join(root, 'doc'))
      const readme = await readFile(join(ROOT, 'apps/lgtm.nvim/README.md'), 'utf8')
      const setup = readme.split('### Native Git packages')[1].match(/```lua\n([\s\S]*?)```/)![1]
      const dependencies = [
        'nui.nvim',
        'nui-components.nvim',
        'codediff.nvim',
        'which-key.nvim',
        'nvim-notify',
      ]
        .map(
          (name) =>
            `vim.opt.runtimepath:append(${JSON.stringify(join(ROOT, 'e2e/nvim/.cache', name))})`,
        )
        .join('\n')
      const init = join(home, 'init.lua')
      await writeFile(init, dependencies + '\n' + setup)
      const verify = join(home, 'verify.lua')
      await writeFile(
        verify,
        `
        assert(vim.fn.exists(":Lgtm") == 2, "LGTM command missing")
        assert(require("lgtm.config").options.identity.name == "Your Name", "Setup options lost")
        assert(debug.getinfo(require("lgtm").setup).source:find(${JSON.stringify(root)}, 1, true), "Loaded another installation")
        vim.cmd("helptags " .. vim.fn.fnameescape(${JSON.stringify(join(root, 'doc'))}))
        vim.cmd("help lgtm.txt")
        assert(vim.api.nvim_buf_get_name(0):find(${JSON.stringify(root)}, 1, true), "Help did not load from package")
      `,
      )

      // Act
      const result = await command(
        'nvim',
        [
          '--headless',
          '-u',
          init,
          '-i',
          'NONE',
          '-c',
          `lua local ok, err = pcall(dofile, ${JSON.stringify(verify)}); if not ok then vim.api.nvim_err_writeln(tostring(err)); vim.cmd("cquit 1") end`,
          '-c',
          'qa!',
        ],
        { env, cwd: home },
      )

      // Assert
      expect(result.code, result.stderr).toBe(0)
      expect(result.stderr).not.toContain('Error detected')
    } finally {
      await rm(home, { recursive: true, force: true })
    }
  })
})
