/**
 * @module-tag e2e
 * @module-tag nvim
 */
import { describe, it, expect } from 'vitest'
import { cp, mkdir, mkdtemp, readFile, readlink, rm, symlink, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { command, ROOT } from '../common/test-harness.js'
import { isolatedEnv } from '../common/test-utils.js'
import { profiles, useEditor } from './fixtures.js'

for (const profile of profiles) {
  describe(`When loading the plugin shell [${profile}]`, { tags: ['e2e', 'nvim', profile] }, () => {
    const fixture = useEditor(profile)

    it(
      'should_load_lgtm_register_commands_and_whichkey_mappings_without_opening_windows',
      { tags: ['smoke'] },
      async () => {
        // Arrange
        const { editor } = fixture()
        const windows = await editor.value<number[]>('vim.api.nvim_list_wins()')

        // Act
        await editor.lua('require("lgtm").setup(); require("lgtm").setup({})')

        // Assert
        expect(await editor.value('vim.g.loaded_lgtm')).toBe(true)
        expect(await editor.value('vim.v.errmsg')).toBe('')
        expect(await editor.value('vim.fn.exists(":Lgtm")')).toBe(2)
        expect(await editor.value('vim.fn.maparg("<leader>rc", "n") ~= ""')).toBe(true)
        expect(await editor.value('vim.fn.maparg("<leader>rp", "n") ~= ""')).toBe(true)
        expect(await editor.value('vim.fn.maparg("<leader>rt", "n") ~= ""')).toBe(true)
        expect(await editor.value('vim.api.nvim_list_wins()')).toEqual(windows)
        expect(windows).toHaveLength(1)
      },
    )

    it(
      'Runs Lua, Ex commands and key input against a fixture file',
      { tags: ['smoke'] },
      async () => {
        // Arrange
        const { editor, runtime } = fixture()
        const file = await runtime.write('shell [literal] ü.txt', 'original\n')

        // Act
        await editor.edit(file)
        const roundTrip = await editor.lua('return ...', { text: 'Lua round trip', count: 2 })
        await editor.command('let g:lgtm_shell_test = 42')

        // Assert
        expect(roundTrip).toEqual({ text: 'Lua round trip', count: 2 })
        expect(await editor.value('vim.g.lgtm_shell_test')).toBe(42)
        expect(await editor.text()).toBe('original')

        // Act
        // i enters insertion in both the required and Kak profiles.
        await editor.keys('iinserted <Esc>')
        await editor.wait(
          'vim.api.nvim_get_current_line() == "inserted original"',
          'key input applied',
        )

        // Assert
        expect(await editor.text()).toBe('inserted original')
      },
    )

    it('Collects diagnostics and closes the editor before runtime cleanup', async () => {
      // Arrange
      const { editor, runtime } = fixture()
      await editor.command('echomsg "shell diagnostics"')

      // Act
      const diagnostics = await editor.diagnostics()

      // Assert
      expect(diagnostics.messages).toContain('shell diagnostics')
      expect(diagnostics.windows).toHaveLength(1)
      expect(diagnostics.current).toMatchObject({
        buf: expect.any(Number),
        win: expect.any(Number),
      })
      expect(diagnostics.stderr).toBe('')

      // Act
      await editor.close()

      // Assert
      expect(await editor.diagnostics()).toMatchObject({ editor: 'stopped' })
      expect(runtime.child?.exitCode).toBeNull()
    })
  })
}

describe(
  'When installing the monorepo as a native Neovim package',
  { tags: ['e2e', 'nvim', 'required'] },
  () => {
    it('The documented setup loads the shell and discovers its help from the package root', async () => {
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
        const init = join(home, 'init.lua')
        const dependencies = [
          'codediff.nvim',
          'which-key.nvim',
          'nvim-notify',
          'nui.nvim',
          'nui-components.nvim',
        ]
          .map(
            (name) =>
              `vim.opt.runtimepath:append(${JSON.stringify(join(ROOT, 'e2e/nvim/.cache', name))})`,
          )
          .join('\n')
        await writeFile(init, dependencies + '\n' + setup)
        const verify = join(home, 'verify.lua')
        await writeFile(
          verify,
          `
        assert(vim.g.loaded_lgtm, "LGTM runtime entry missing")
        for _, name in ipairs({"codediff", "which-key", "notify", "nui-components", "nui.popup"}) do
          assert(package.loaded[name], "Plugin setup did not require " .. name)
        end
        assert(vim.fn.exists(":Lgtm") == 2, "Expected :Lgtm command to be registered")
        assert(vim.v.errmsg == "", vim.v.errmsg)
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
  },
)

for (const profile of profiles) {
  describe(
    `When installing the plugin with lazy.nvim [${profile}]`,
    { tags: ['e2e', 'nvim', profile] },
    () => {
      it('Loads the shell and its dependencies using the documented plugin spec', async () => {
        // Arrange
        const home = await mkdtemp(join(tmpdir(), 'lgtm-lazy-package-'))
        try {
          const env = isolatedEnv(home)
          const init = join(home, 'init.lua')
          const readme = await readFile(join(ROOT, 'apps/lgtm.nvim/README.md'), 'utf8')
          const setup = readme.split('### lazy.nvim')[1].match(/```lua\n([\s\S]*?)```/)![1]
          await writeFile(
            init,
            `
          vim.opt.runtimepath:prepend(${JSON.stringify(join(ROOT, 'e2e/nvim/.cache/lazy.nvim'))})
          local plugin = ${setup}
          plugin[1] = nil
          plugin.dir = ${JSON.stringify(ROOT)}
          plugin.name = "lgtm"
          local spec = {plugin}
          if ${profile === 'kak'} then table.insert(spec, {"mirlge/kak.nvim"}) end
          require("lazy").setup(spec, {
            root=${JSON.stringify(join(ROOT, 'e2e/nvim/.cache'))},
            lockfile=${JSON.stringify(join(home, 'lazy-lock.json'))},
            install={missing=false}, checker={enabled=false},
            change_detection={enabled=false}, rocks={enabled=false},
            pkg={enabled=false},
          })
        `,
          )
          const verify = join(home, 'verify.lua')
          await writeFile(
            verify,
            `
          assert(vim.g.loaded_lgtm and package.loaded.lgtm, "Plugin package failed to load")
          assert(vim.fn.exists(":Lgtm") == 2, "Expected :Lgtm command to be registered")
          assert(vim.v.errmsg == "", vim.v.errmsg)
          assert(require("nui-components") and require("nui.popup"))
          assert(require("codediff") and require("which-key") and require("notify"))
          local plugins = require("lazy.core.config").plugins
          assert((plugins["kak.nvim"] ~= nil) == ${profile === 'kak'}, "Optional Kak selection changed")
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
    },
  )
}
