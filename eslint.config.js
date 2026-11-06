import eslint from '@eslint/js'
import tseslint from 'typescript-eslint'
import globals from 'globals'

export default tseslint.config(
  {
    ignores: [
      '**/node_modules/**',
      'e2e/nvim/.cache/**',
      '**/dist/**',
      '**/dist-e2e/**',
      '**/.output/**',
      '**/.svelte-kit/**',
      '**/build/**',
      '**/coverage/**',
      '**/test-results/**',
      '**/playwright-report/**',
      '**/routeTree.gen.ts',
      '**/src/lib/gen/**',
    ],
  },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.bun,
        ...globals.node,
      },
    },
  },
)
