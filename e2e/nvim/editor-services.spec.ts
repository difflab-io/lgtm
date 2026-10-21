/**
 * @module-tag e2e
 * @module-tag nvim
 */
import { describe, it, expect } from 'vitest'
import { profiles, useEditor } from './fixtures.js'
import { waitFor } from '../common/runtime.js'

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
      await e.lua(`local t=require("lgtm.transport.connect"); _G.e2e.callbacks=0
          local function done() _G.e2e.callbacks=_G.e2e.callbacks+1 end
          local cancel=t.call("IdentityService","GetCapabilities",{},nil,done); cancel()
          t.call("IdentityService","GetCapabilities",{},nil,done)
          t.call("IdentityService","GetCapabilities",{},nil,done); t.cancel_all()
          _G.e2e.cancel_settled=false; vim.defer_fn(function() _G.e2e.cancel_settled=true end,250)`)

      // Assert
      await e.wait('_G.e2e.cancel_settled', 'cancelled curl processes settled')
      expect(await e.value('_G.e2e.callbacks')).toBe(0)

      // Act
      await e.lua(`require("lgtm.transport.connect").call("IdentityService","GetCapabilities",{},nil,function(err,result)
          _G.e2e.transport_result={error=err,result=result} end)`)

      // Assert
      await e.wait('_G.e2e.transport_result ~= nil', 'later real request')
      expect(await e.value('_G.e2e.transport_result.error')).toBeNull()
      expect(await e.value('_G.e2e.transport_result.result.apiVersion')).toBe('1')
    })
  })
  describe(`When applying the notification theme [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)
    it('Notifications use the plugin accent, compact geometry and user highlight overrides', async () => {
      // Arrange
      const { editor: e } = fixture()
      // Act
      await e.lua('require("lgtm.notifications").show("Saved UI evidence")')

      // Assert
      const win = await waitFor(
        'notification window',
        () =>
          e.lua<number>(`for _,w in ipairs(vim.api.nvim_list_wins()) do
          local b=vim.api.nvim_win_get_buf(w)
          if vim.bo[b].filetype=="notify" and table.concat(vim.api.nvim_buf_get_lines(b,0,-1,false),"\\n"):find("Saved UI evidence",1,true) then return w end end; return 0`),
        Boolean,
      )
      expect(await e.value('vim.api.nvim_win_get_width(...)', win)).toBe(50)
      const height = await e.value<number>('vim.api.nvim_win_get_height(...)', win)
      expect(height).toBeGreaterThanOrEqual(1)
      expect(height).toBeLessThanOrEqual(4)
      const namespace = await e.value<number>('vim.api.nvim_get_hl_ns({winid=...})', win)
      expect(namespace).toBe(
        await e.value('vim.api.nvim_create_namespace("lgtm.notification-theme")'),
      )
      const accent = await e.lua<string>(
        `local w,ns=...; for _,m in ipairs(vim.api.nvim_buf_get_extmarks(vim.api.nvim_win_get_buf(w),require("notify.render.base").namespace(),0,-1,{details=true})) do
          local name=m[4].hl_group
          if name and vim.api.nvim_get_hl(ns,{name=name,link=true}).link=="LgtmInfo" then return name end end; return ""`,
        win,
        namespace,
      )
      expect(accent).not.toBe('')

      // Act
      await e.lua(
        'vim.api.nvim_set_hl(0,"LgtmInfo",{link="Special"}); require("lgtm.cards").highlights()',
      )

      // Assert
      expect(await e.value('vim.api.nvim_get_hl(0,{name="LgtmCommentStatus",link=false}).fg')).toBe(
        await e.value('vim.api.nvim_get_hl(0,{name="Special",link=false}).fg'),
      )
      expect(
        await e.lua(
          'local ns,name=...; return vim.api.nvim_get_hl(ns,{name=name,link=true}).link',
          namespace,
          accent,
        ),
      ).toBe('LgtmInfo')
    })
  })
  describe(`When customizing modal style [${profile}]`, { tags: [profile] }, () => {
    const fixture = useEditor(profile)
    it('Composer borders and comment cards follow the same user configuration', async () => {
      // Arrange
      const { editor: e } = fixture()
      // Act
      await e.lua(
        'require("lgtm.config").options.ui.border="single"; require("lgtm.composer").open("Theme example","",function(_,done) done() end)',
      )

      // Assert
      const composer = await e.composer()
      expect(await e.value('vim.api.nvim_win_get_config(...).border[1]', composer.win)).toBe('┌')
      expect(await e.value('vim.wo[...].winhighlight', composer.win)).toContain(
        'FloatBorder:LgtmBorder',
      )
      const card = await e.lua(
        'return require("lgtm.cards").render({anchor={kind="line"},state="open"},{{body="Preview",author={name="Reviewer"}}},"GitHub draft",40,false)',
      )
      expect(card[0][0][0]).toMatch(/^┌/)

      // Act
      await e.keys('q')
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
        'require("lgtm.picker").last and not require("lgtm.picker").last.closed',
        'session picker',
      )
      await e.keys('not present')
      await e.wait('#require("lgtm.picker").last.filtered == 0', 'empty picker result')
      const text = await e.value(
        'vim.api.nvim_buf_get_lines(require("lgtm.picker").last.list_buf,0,1,false)[1]',
      )
      await e.keys('<Esc>')

      // Assert
      expect(text).toBe('(no matches)')
      expect(await e.value('require("lgtm.picker").last.closed')).toBe(true)
      expect(await e.state()).toBeNull()
    })
  })
}
