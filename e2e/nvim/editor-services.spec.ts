import { describe, expect } from 'vitest'
import { profiles, suite } from './fixtures.js'
import { step, waitFor } from '../common/runtime.js'

// tests -----------------------------------------------------------------------

for (const profile of profiles) {
  const it = suite(profile)
  describe(`Neovim · dependency, transport and theme contracts · ${profile}`, () => {
    it('reports real dependency health, cancels transport safely and gives notifications, cards and modals one overridable style', async ({
      editor: e,
    }) => {
      // Arrange ---------------------------------------------------------------
      await step(
        'Given the required plugins and real backend, health reports a loaded CodeDiff library and a reachable API',
        async () => {
          // Act ---------------------------------------------------------------
          await e.command('checkhealth lgtm')

          // Assert ------------------------------------------------------------
          const report = await waitFor(
            'health response',
            () => e.text(),
            (text) => text.includes('Backend reachable, API 1'),
          )
          expect(report).toContain('CodeDiff native library loaded')
          expect(report).not.toContain('ERROR')
        },
      )

      await step(
        'When individual and global transport cancellation race with responses, callbacks are suppressed and later requests still work',
        async () => {
          // Act ---------------------------------------------------------------
          await e.lua(`local t=require("lgtm.transport.connect"); _G.e2e.callbacks=0
          local function done() _G.e2e.callbacks=_G.e2e.callbacks+1 end
          local cancel=t.call("IdentityService","GetCapabilities",{},nil,done); cancel()
          t.call("IdentityService","GetCapabilities",{},nil,done)
          t.call("IdentityService","GetCapabilities",{},nil,done); t.cancel_all()
          _G.e2e.cancel_settled=false; vim.defer_fn(function() _G.e2e.cancel_settled=true end,250)`)

          // Assert ------------------------------------------------------------
          await e.wait('_G.e2e.cancel_settled', 'cancelled curl processes settled')
          expect(await e.value('_G.e2e.callbacks')).toBe(0)

          // Act ---------------------------------------------------------------
          await e.lua(`require("lgtm.transport.connect").call("IdentityService","GetCapabilities",{},nil,function(err,result)
          _G.e2e.transport_result={error=err,result=result} end)`)

          // Assert ------------------------------------------------------------
          await e.wait('_G.e2e.transport_result ~= nil', 'later real request')
          expect(await e.value('_G.e2e.transport_result.error')).toBeNull()
          expect(await e.value('_G.e2e.transport_result.result.apiVersion')).toBe('1')
        },
      )

      await step(
        'Then the real notification renderer uses fixed dimensions and LGTM semantic colors, and custom borders apply to cards and modals',
        async () => {
          // Act ---------------------------------------------------------------
          await e.lua('require("lgtm.notifications").show("Saved UI evidence")')

          // Assert ------------------------------------------------------------
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

          // Act ---------------------------------------------------------------
          await e.lua(
            'vim.api.nvim_set_hl(0,"LgtmInfo",{link="Special"}); require("lgtm.cards").highlights()',
          )

          // Assert ------------------------------------------------------------
          expect(
            await e.value('vim.api.nvim_get_hl(0,{name="LgtmCommentStatus",link=false}).fg'),
          ).toBe(await e.value('vim.api.nvim_get_hl(0,{name="Special",link=false}).fg'))
          expect(
            await e.lua(
              'local ns,name=...; return vim.api.nvim_get_hl(ns,{name=name,link=true}).link',
              namespace,
              accent,
            ),
          ).toBe('LgtmInfo')

          // Act ---------------------------------------------------------------
          await e.lua(
            'require("lgtm.config").options.ui.border="single"; require("lgtm.composer").open("Theme example","",function(_,done) done() end)',
          )

          // Assert ------------------------------------------------------------
          const composer = await e.composer()

          expect(await e.value('vim.api.nvim_win_get_config(...).border[1]', composer.win)).toBe(
            '┌',
          )
          expect(await e.value('vim.wo[...].winhighlight', composer.win)).toContain(
            'FloatBorder:LgtmBorder',
          )
          const card = await e.lua(
            'return require("lgtm.cards").render({anchor={kind="line"},state="open"},{{body="Preview",author={name="Reviewer"}}},"GitHub draft",40,false)',
          )
          expect(card[0][0][0]).toMatch(/^┌/)

          // Act ---------------------------------------------------------------
          await e.keys('q')
        },
      )

      await step(
        'And a searchable picker has a clear no-results state and Escape cancels without selecting an item',
        async () => {
          // Arrange -----------------------------------------------------------
          await e.lua(
            'require("lgtm.picker").searchable("Cancellation",{"one","two"},tostring,function() _G.e2e.cancel_selected=true end)',
          )

          // Act ---------------------------------------------------------------
          await e.keys('not present')

          // Assert ------------------------------------------------------------
          await e.wait('#require("lgtm.picker").last.filtered==0', 'empty picker result')
          expect(
            await e.value(
              'vim.api.nvim_buf_get_lines(require("lgtm.picker").last.list_buf,0,1,false)[1]',
            ),
          ).toBe('(no matches)')

          // Act ---------------------------------------------------------------
          await e.keys('<Esc>')

          // Assert ------------------------------------------------------------
          expect(await e.value('require("lgtm.picker").last.closed')).toBe(true)
          expect(await e.value('_G.e2e.cancel_selected')).toBeNull()
        },
      )
    })
  })
}
