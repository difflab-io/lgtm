import { sveltekit } from '@sveltejs/kit/vite'
import { defineConfig } from 'vite'

const rpcProxy = {
  target: process.env.LGTM_BACKEND_URL ?? 'http://127.0.0.1:8080',
  rewrite: (path: string) => path.replace(/^\/rpc/, ''),
}

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    host: '127.0.0.1',
    port: Number(process.env.LGTM_WEB_PORT ?? 5173),
    strictPort: true,
    proxy: { '/rpc': rpcProxy },
  },
  preview: { proxy: { '/rpc': rpcProxy } },
})
