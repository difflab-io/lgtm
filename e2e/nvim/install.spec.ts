/**
 * @module-tag e2e
 * @module-tag nvim
 */
import { describe, it, expect } from 'vitest'
import { join } from 'node:path'
import { command, ROOT } from '../common/test-harness.js'
import { usePluginInstallation } from './install.fixtures.js'

// fixtures --------------------------------------------------------------------

const sources = [
  { name: 'development', task: 'nvim:install-dev-plugin' },
  { name: 'released', task: 'nvim:install-released-plugin' },
] as const

// tests -----------------------------------------------------------------------

for (const source of sources) {
  describe(`When installing the ${source.name} plugin`, { tags: ['required'] }, () => {
    const fixture = usePluginInstallation()

    it('Loads LGTM with its dependencies and preserves user options', async () => {
      // Arrange
      const { runtime, editor } = fixture()
      const init = join(runtime.env.XDG_CONFIG_HOME!, 'nvim/init.lua')

      // Act
      const installed = await command('mise', ['run', source.task], {
        cwd: ROOT,
        env: runtime.env,
        timeout: 150_000,
      })
      expect(installed.code, installed.stdout + installed.stderr).toBe(0)
      await editor.start(init)

      // Assert
      expect(await editor.value('vim.v.errmsg')).toBe('')
      expect(await editor.value('vim.fn.exists(":Lgtm")')).toBe(2)
      expect(
        await editor.value(`(function()
          local plugins = require("lazy.core.config").plugins
          for _,name in ipairs({"lgtm", "nui.nvim", "nui-components.nvim", "codediff.nvim", "which-key.nvim", "nvim-notify"}) do
            if not plugins[name] or not plugins[name]._.installed then return false end
          end
          return true
        end)()`),
      ).toBe(true)
      expect(await editor.value('require("lgtm.config").options.identity.name')).toBe(
        'Installation reviewer',
      )
      expect(await editor.value('require("lgtm.config").options.ui.border')).toBe('double')
      expect(await editor.value('require("which-key.config").options.delay')).toBe(17)
      expect(await editor.value('require("notify")._config().default_timeout()')).toBe(4321)
      expect(await editor.value('require("codediff.config").options.explorer.auto_refresh')).toBe(
        false,
      )
      const plugin = await editor.value(
        '(function() local p=require("lazy.core.config").plugins.lgtm; return {dir=p.dir,branch=p.branch} end)()',
      )
      if (source.name === 'development') expect(plugin.dir).toBe(ROOT)
      else expect(plugin.branch).toBe('main')

      // Act
      await editor.command(`edit ${join(runtime.workspace, 'note.txt')}`)
      await editor.command('Lgtm note')

      // Assert
      await editor.wait(
        '(function() for _,win in ipairs(vim.api.nvim_list_wins()) do if vim.bo[vim.api.nvim_win_get_buf(win)].filetype == "lgtm" and vim.api.nvim_win_get_config(win).relative ~= "" then return true end end return false end)()',
        'installed plugin opens the note composer',
      )
    })
  })
}
