import { mkdir, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { homedir } from 'node:os'
import { useTestHarness } from '../common/fixtures.js'
import { ROOT } from '../common/test-harness.js'
import { useEditorProcess } from './fixtures.js'

/** Own an isolated user profile; specifications run the public installation tasks. */
export function usePluginInstallation() {
  return useEditorProcess(useTestHarness('cli'), 'required', {
    start: false,
    prepare: async ({ runtime }) => {
      const config = join(runtime.env.XDG_CONFIG_HOME!, 'nvim')
      await mkdir(join(config, 'lua/plugins'), { recursive: true })
      runtime.env.LGTM_TEST_LAZY_DIR = join(ROOT, 'e2e/nvim/.cache/lazy.nvim')
      runtime.env.MISE_TRUSTED_CONFIG_PATHS = ROOT
      // Reuse runner tools while isolating the editor's configuration and data.
      runtime.env.MISE_DATA_DIR =
        process.env.MISE_DATA_DIR ??
        join(process.env.XDG_DATA_HOME ?? join(homedir(), '.local/share'), 'mise')
      await writeFile(
        join(config, 'init.lua'),
        `dofile(${JSON.stringify(join(ROOT, 'e2e/nvim/install.init.lua'))})`,
      )
      await writeFile(
        join(config, 'lua/plugins/lgtm.lua'),
        `return {{opts={identity={name="Installation reviewer",email="installer@example.test"},
endpoint_url=${JSON.stringify(runtime.endpoint)},ui={border="double"}}}}`,
      )
    },
  })
}
