import { join } from 'node:path'
import { mergeConfig } from 'vitest/config'
import { suiteConfig } from '../common/config.js'

export default mergeConfig(suiteConfig(import.meta.dirname), {
  test: { globalSetup: join(import.meta.dirname, 'dependencies.ts') },
})
