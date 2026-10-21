import { defineConfig } from 'vitest/config'

export function suiteConfig(
  root: string,
  options: { include?: string[]; exclude?: string[] } = {},
) {
  return defineConfig({
    test: {
      root,
      include: options.include ?? [process.env.LGTM_E2E_SPEC ?? '**/*.spec.ts'],
      exclude: ['**/.cache/**', ...(options.exclude ?? [])],
      testTimeout: 120_000,
      hookTimeout: 30_000,
      fileParallelism: false,
      maxWorkers: 1,
      reporters: ['verbose'],
      retry: 0,
    },
  })
}
