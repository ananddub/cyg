import path from 'node:path'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// Axum serves the built app under /dashboard/, so every asset URL must carry
// that prefix; relative URLs only from here on — the control plane's port
// varies with control.api_port and the frontend must not hard-code it.
export default defineConfig({
  base: '/dashboard/',
  plugins: [react(), tailwindcss()],
  build: {
    outDir: 'dist',
  },
  resolve: {
    alias: {
      '@': path.resolve(import.meta.dirname, './src'),
    },
  },
})
