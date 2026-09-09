import { resolve } from 'node:path'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

// https://vite.dev/config/
export default defineConfig({
  base: '/ALAS-site/',
  plugins: [react(), tailwindcss()],
  build: {
    rollupOptions: {
      // Multi-page: the landing page, plus acknowledgements on its own URL so
      // the landing page stays focused on getting people to a download.
      input: {
        main: resolve(__dirname, 'index.html'),
        acknowledgements: resolve(__dirname, 'acknowledgements/index.html'),
      },
    },
  },
})
