import { usePrFixture } from '../common/pr.fixtures.js'
import { useEditorProcess, type profiles } from './fixtures.js'

export function usePrEditor(profile: (typeof profiles)[number]) {
  return useEditorProcess(usePrFixture('cli'), profile, {
    timeoutMs: 40_000,
    prepare: async ({ runtime, github }) => {
      await github.checkout(runtime)
    },
  })
}
