import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

export default defineConfig(({ mode }) => ({
  base: mode === 'production' ? '/bamviz/' : '/',
  plugins: [svelte()],
  resolve: {
    alias: {
      '@bamviz-wasm': new URL('./src/lib/wasm-pkg/bamviz_wasm.js', import.meta.url).pathname,
    },
  },
  test: { exclude: ['tests/**', 'node_modules/**'] },
}))
