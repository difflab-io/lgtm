import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'
import { fileURLToPath } from 'node:url'

// Compile the actual application shell and its CSS; this entry exists only in the test build.
export default defineConfig({
  root: fileURLToPath(new URL('../..', import.meta.url)),
  plugins: [tailwindcss(), svelte()],
  resolve: {
    alias: {
      $lib: fileURLToPath(new URL('../../apps/lgtm-web/src/lib', import.meta.url)),
    },
  },
  build: {
    outDir: 'e2e/desktop/dist-e2e',
    rolldownOptions: { input: 'e2e/desktop/fixture.html' },
  },
  preview: { host: '127.0.0.1', port: 1421, strictPort: true },
})
