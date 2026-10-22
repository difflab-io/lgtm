import { defineConfig } from 'vitest/config'

export function suiteConfig(
  root: string,
  options: { include?: string[]; exclude?: string[] } = {},
) {
  return defineConfig({
    test: {
      root,
      tags: [
        'e2e',
        'smoke',
        'backend',
        'cli',
        'nvim',
        'github',
        'pr-review',
        'local-review',
        'notes',
        'diffs',
        'required',
        'kak',
      ].map((name) => ({ name })),
      include: options.include ?? [process.env.LGTM_E2E_SPEC ?? '**/*.spec.ts'],
      exclude: ['**/.cache/**', ...(options.exclude ?? [])],
      testTimeout: 180_000,
      hookTimeout: 60_000,
      sequence: { hooks: 'stack' },
      fileParallelism: false,
      maxWorkers: 1,
      reporters: ['verbose'],
      retry: 0,
    },
  })
}
