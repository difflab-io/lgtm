import eslint from '@eslint/js'
import tseslint from 'typescript-eslint'
import globals from 'globals'

export default tseslint.config(
  {
    ignores: [
      '**/node_modules/**',
      'e2e/nvim/.cache/**',
      '**/dist/**',
      '**/.output/**',
      '**/.svelte-kit/**',
      '**/build/**',
      '**/coverage/**',
      '**/routeTree.gen.ts',
      '**/src/lib/gen/**',
      'apps/lgtm-cli/bundle/**',
      'apps/lgtm-cli/.bundle-stage/**',
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
