import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  base: '/portal/',
  server: {
    proxy: {
      '/api': 'http://localhost:31000',
      '/auth': 'http://localhost:31000',
      '/v1': 'http://localhost:31000',
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: false,
  },
})
