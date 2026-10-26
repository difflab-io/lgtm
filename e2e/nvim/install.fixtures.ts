import { beforeEach } from 'vitest'
import { cp, mkdir, readFile, symlink, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { command, ROOT, type TestHarness } from '../common/test-harness.js'
import { useTestHarness } from '../common/fixtures.js'

type Installation = {
  runtime: TestHarness
  repository: string
  managerRoot: string
  init: string
  report: string
  sourceConfig: string
}

type InstallationOptions = {
  customizeProviders?: boolean
  source?: 'metadata' | 'development' | 'released'
}

/** Install the current plugin source into an empty manager root, outside the runtime cache. */
export function usePluginInstallation() {
  const harness = useTestHarness('cli')
  let installation: Installation

  beforeEach(async () => {
    const { runtime } = harness()
    installation = {
      runtime,
      repository: join(runtime.directory, 'plugin-source'),
      managerRoot: join(runtime.home, 'lazy'),
      init: join(runtime.home, 'install-init.lua'),
      report: join(runtime.home, 'install-report.json'),
      sourceConfig: join(runtime.env.XDG_CONFIG_HOME!, 'nvim/lua/plugins/lgtm.lua'),
    }
    await createPluginSnapshot(installation)
    await configureDevelopmentSource(installation)
  })

  return () => ({
    runtime: installation.runtime,
    managerRoot: installation.managerRoot,
    start: (options: InstallationOptions = {}) => startInstallation(installation, options),
  })
}

// Helpers ---------------------------------------------------------------------

async function createPluginSnapshot(installation: Installation) {
  const { repository } = installation
  await mkdir(join(repository, 'apps'), { recursive: true })
  await cp(join(ROOT, 'apps/lgtm.nvim'), join(repository, 'apps/lgtm.nvim'), { recursive: true })
  await cp(join(ROOT, 'lazy.lua'), join(repository, 'lazy.lua'))
  await symlink('apps/lgtm.nvim/doc', join(repository, 'doc'))
  await runSnapshotGit(installation, [
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
  ])
}

async function configureDevelopmentSource(installation: Installation) {
  const { runtime, sourceConfig } = installation
  await runtime.write('note.txt', 'A saved file for the installation journey.\n')
  await mkdir(join(sourceConfig, '..'), { recursive: true })
  await writeFile(
    sourceConfig,
    `return {{opts={identity={name="One-declaration reviewer",email="installer@example.test"},
endpoint_url=${JSON.stringify(runtime.endpoint)},ui={border="double"}}}}`,
  )
  await switchPluginSource(installation, 'worktree')
}

async function startInstallation(
  installation: Installation,
  { customizeProviders = false, source = 'metadata' }: InstallationOptions,
) {
  await writeFile(
    installation.init,
    buildEditorInitScript(installation, source, customizeProviders),
  )
  if (source === 'released') await configureReleasedSource(installation)
  return verifyInstallation(installation, customizeProviders)
}

async function configureReleasedSource(installation: Installation) {
  const { runtime, init } = installation
  // Exercise main's missing-package-metadata case with the real source-switch command.
  await runSnapshotGit(installation, [
    ['rm', 'lazy.lua'],
    [
      '-c',
      'user.name=E2E',
      '-c',
      'user.email=e2e@example.test',
      'commit',
      '-qm',
      'No package metadata',
    ],
  ])
  await writeFile(
    join(runtime.env.XDG_CONFIG_HOME!, 'nvim/init.lua'),
    `dofile(${JSON.stringify(init)})`,
  )
  await switchPluginSource(installation, 'main')
}

async function switchPluginSource(installation: Installation, source: 'worktree' | 'main') {
  const switched = await command('bash', [join(ROOT, 'apps/lgtm.nvim/mise-tasks/install-plugin')], {
    env: { ...installation.runtime.env, usage_source: source },
    timeout: 150_000,
  })
  if (switched.code !== 0)
    throw new Error(`Plugin source switch failed\n${switched.stdout}\n${switched.stderr}`)
}

async function runSnapshotGit({ runtime, repository }: Installation, commands: string[][]) {
  for (const args of commands) {
    const result = await command('git', ['-C', repository, ...args], { env: runtime.env })
    if (result.code !== 0) throw new Error(result.stderr)
  }
}

async function verifyInstallation(installation: Installation, customizeProviders: boolean) {
  const { runtime, init, report } = installation
  const verify = join(runtime.home, 'verify-install.lua')
  await writeFile(verify, installationVerification(installation, customizeProviders))
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
}

function buildEditorInitScript(
  { runtime, repository, sourceConfig, managerRoot }: Installation,
  source: NonNullable<InstallationOptions['source']>,
  customizeProviders: boolean,
) {
  const providerSpecs = customizeProviders
    ? `
      { "folke/which-key.nvim", opts = { delay = 17 } },
      { "rcarriga/nvim-notify", opts = { timeout = 4321, stages = "static" } },
      { "esmuellert/codediff.nvim", opts = { explorer = { auto_refresh = false } } },`
    : ''
  return `vim.opt.runtimepath:prepend(${JSON.stringify(join(ROOT, 'e2e/nvim/.cache/lazy.nvim'))})
vim.o.columns, vim.o.lines, vim.o.swapfile, vim.o.shadafile = 120, 40, false, "NONE"
local lgtm = assert(loadfile(${JSON.stringify(sourceConfig)}))()[1]
${
  source === 'released'
    ? 'assert(lgtm[1] == "difflab-io/lgtm" and lgtm.branch == "main", "Released installer did not select main")'
    : `assert(lgtm.dir == ${JSON.stringify(ROOT)}, "Development installer did not select the checkout root")`
}
${
  source === 'development'
    ? ''
    : `lgtm.dir, lgtm.url = nil, ${JSON.stringify('file://' + repository)}`
}
${source === 'metadata' ? 'lgtm.config, lgtm.dependencies, lgtm.lazy = nil, nil, nil' : ''}
-- Reproduce an existing lazy.nvim cache that has never discovered LGTM metadata.
local cache = ${JSON.stringify(join(runtime.home, 'pkg-cache.lua'))}
if vim.fn.filereadable(cache) == 0 then
  vim.fn.writefile({"return {pkgs={},version=" .. require("lazy.pkg").VERSION .. "}"}, cache)
end
require("lazy").setup({
  lgtm,
  ${providerSpecs}
}, {
  root = ${JSON.stringify(managerRoot)},
  lockfile = ${JSON.stringify(join(runtime.home, 'lazy-lock.json'))},
  pkg = { sources = { "lazy" }, cache = cache }, rocks = { enabled = false },
  checker = { enabled = false }, change_detection = { enabled = false },
  install = { colorscheme = { "default" } },
})
`
}

function installationVerification({ runtime, report }: Installation, customizeProviders: boolean) {
  return `local plugins = require("lazy.core.config").plugins
local required = { "lgtm", "nui.nvim", "nui-components.nvim", "codediff.nvim", "which-key.nvim", "nvim-notify" }
for _, name in ipairs(required) do
  assert(plugins[name] and plugins[name]._.installed, "Missing managed dependency: " .. name)
end
assert(vim.v.errmsg == "", vim.v.errmsg)
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
`
}
