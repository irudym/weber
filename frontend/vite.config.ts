import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vite'

const weber = 'http://127.0.0.1:3000';
// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte()],
  server: {
    proxy: {
      '/api': weber,
      '/login': weber,
      '/logout': weber,
      '/assets/vt323.woff2': weber, // font used by the (Askama) login page
    }
  }
})
