import { defineConfig } from 'vitest/config'
export default defineConfig({
  test: {
    root: import.meta.dirname,
    include: ['*.spec.ts'],
    tags: ['e2e', 'sync', 'notes', 'local-review', 'pr-review'].map((name) => ({ name })),
    testTimeout: 90000,
    hookTimeout: 60000,
    fileParallelism: false,
    maxWorkers: 1,
    reporters: ['verbose'],
  },
})
