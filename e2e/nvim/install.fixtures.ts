import { beforeEach } from 'vitest'
import { cp, mkdir, readFile, symlink, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { command, ROOT } from '../common/test-harness.js'
import { useTestHarness } from '../common/fixtures.js'

/** Install the current plugin source into an empty manager root, outside the runtime cache. */
export function usePluginInstallation() {
  const harness = useTestHarness('cli')
  let repository: string
  let managerRoot: string
  let init: string
  let report: string
  let sourceConfig: string

  beforeEach(async () => {
    const { runtime } = harness()
    repository = join(runtime.directory, 'plugin-source')
    managerRoot = join(runtime.home, 'lazy')
    init = join(runtime.home, 'install-init.lua')
    report = join(runtime.home, 'install-report.json')
    sourceConfig = join(runtime.env.XDG_CONFIG_HOME!, 'nvim/lua/plugins/lgtm.lua')
    await mkdir(join(repository, 'apps'), { recursive: true })
    await cp(join(ROOT, 'apps/lgtm.nvim'), join(repository, 'apps/lgtm.nvim'), { recursive: true })
    await cp(join(ROOT, 'lazy.lua'), join(repository, 'lazy.lua'))
    await symlink('apps/lgtm.nvim/doc', join(repository, 'doc'))
    for (const args of [
      ['init', '-q', '-b', 'main'],
      ['add', '.'],
      [
        '-c',
        'user.name=E2E',
        '-c',
        'user.email=e2e@example.test',
        'commit',
        '-qm',
        'plugin snapshot',
      ],
    ]) {
      const result = await command('git', ['-C', repository, ...args], { env: runtime.env })
      if (result.code !== 0) throw new Error(result.stderr)
    }
    await runtime.write('note.txt', 'A saved file for the installation journey.\n')
    await mkdir(join(sourceConfig, '..'), { recursive: true })
    await writeFile(
      sourceConfig,
      `return {{opts={identity={name="One-declaration reviewer",email="installer@example.test"},
endpoint_url=${JSON.stringify(runtime.endpoint)},ui={border="double"}}}}`,
    )
    const switched = await command(
      'bash',
      [join(ROOT, 'apps/lgtm.nvim/mise-tasks/install-plugin')],
      { env: { ...runtime.env, usage_source: 'worktree' } },
    )
    if (switched.code !== 0) throw new Error(switched.stderr)
  })

  return () => {
    const { runtime } = harness()
    return {
      runtime,
      managerRoot,
      async start(customizeProviders = false) {
        const providerSpecs = customizeProviders
          ? `
            { "folke/which-key.nvim", opts = { delay = 17 } },
            { "rcarriga/nvim-notify", opts = { timeout = 4321, stages = "static" } },
            { "esmuellert/codediff.nvim", opts = { explorer = { auto_refresh = false } } },`
          : ''
        await writeFile(
          init,
          `vim.opt.runtimepath:prepend(${JSON.stringify(join(ROOT, 'e2e/nvim/.cache/lazy.nvim'))})
vim.o.columns, vim.o.lines, vim.o.swapfile, vim.o.shadafile = 120, 40, false, "NONE"
local lgtm = assert(loadfile(${JSON.stringify(sourceConfig)}))()[1]
assert(lgtm.dir == ${JSON.stringify(ROOT)}, "Development installer did not select the checkout root")
-- Clone the uncommitted source snapshot rather than loading the development directory.
lgtm.dir, lgtm.url = nil, ${JSON.stringify('file://' + repository)}
require("lazy").setup({
  lgtm,
  ${providerSpecs}
}, {
  root = ${JSON.stringify(managerRoot)},
  lockfile = ${JSON.stringify(join(runtime.home, 'lazy-lock.json'))},
  pkg = { sources = { "lazy" } }, rocks = { enabled = false },
  checker = { enabled = false }, change_detection = { enabled = false },
  install = { colorscheme = { "default" } },
})
`,
        )
        const verify = join(runtime.home, 'verify-install.lua')
        await writeFile(
          verify,
          `local plugins = require("lazy.core.config").plugins
local required = { "lgtm", "nui.nvim", "nui-components.nvim", "codediff.nvim", "which-key.nvim", "nvim-notify" }
for _, name in ipairs(required) do
  assert(plugins[name] and plugins[name]._.installed, "Missing managed dependency: " .. name)
end
assert(vim.fn.exists(":Lgtm") == 2, "LGTM command missing")
assert(require("lgtm.config").options.identity.name == "One-declaration reviewer", "Identity option lost")
assert(require("lgtm.config").options.ui.border == "double", "UI option lost")
assert(require("nui-components") and require("nui.split"), "UI dependencies cannot load")
vim.cmd.edit(vim.fn.fnameescape(${JSON.stringify(join(runtime.workspace, 'note.txt'))}))
vim.cmd("Lgtm note")
local opened = vim.wait(10000, function()
  for _, win in ipairs(vim.api.nvim_list_wins()) do
    local buf = vim.api.nvim_win_get_buf(win)
    if vim.bo[buf].filetype == "lgtm" and vim.api.nvim_win_get_config(win).relative ~= "" then return true end
  end
  return false
end, 20)
assert(opened, "The installed plugin did not open its note composer: " .. vim.api.nvim_exec2("messages", {output=true}).output)
${
  customizeProviders
    ? `assert(require("which-key.config").options.delay == 17, "WhichKey options reset")
assert(require("notify")._config().default_timeout() == 4321, "Notification options reset")
assert(require("codediff.config").options.explorer.auto_refresh == false, "CodeDiff options reset")`
    : ''
}
vim.fn.writefile({vim.json.encode({plugins=required, composer_open=opened, customized=${customizeProviders}})}, ${JSON.stringify(report)})
`,
        )
        const result = await command(
          process.env.LGTM_E2E_NVIM_BIN ?? 'nvim',
          [
            '--headless',
            '-u',
            init,
            '-i',
            'NONE',
            '-n',
            '-c',
            `lua local ok,err=pcall(dofile,${JSON.stringify(verify)}); if not ok then vim.api.nvim_err_writeln(tostring(err)); vim.cmd("cquit 1") end`,
            '-c',
            'qa!',
          ],
          { env: runtime.env, cwd: runtime.workspace, timeout: 150_000 },
        )
        if (result.code !== 0)
          throw new Error(
            `Installation editor failed (${result.code})\n${result.stdout}\n${result.stderr}`,
          )
        return JSON.parse(await readFile(report, 'utf8')) as {
          plugins: string[]
          composer_open: boolean
          customized: boolean
        }
      },
    }
  }
}
