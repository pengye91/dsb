import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  // Set VITE_BASE_PATH (e.g., /sandboxes) when deploying behind a reverse proxy that strips a URL prefix.
  base: process.env.VITE_BASE_PATH || '/dashboard/',
  server: {
    port: 3000,
    proxy: {
      // Only proxy paths that START WITH /api/ (note the trailing slash requirement)
      // This prevents /api-keys from being proxied
      '^/api(/|$)': {
        target: 'http://localhost:8080',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api/, ''),
        configure: (proxy, _options) => {
          // Only log errors, not every request (too noisy)
          proxy.on('error', (err, req, res) => {
            console.error('[Proxy] Error:', err.message, req.url);
          });
        },
      },
    },
  },
})
