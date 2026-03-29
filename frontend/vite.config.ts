import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, '..', '')
  const allowedHosts = (env.VITE_ALLOWED_HOSTS ?? '.trycloudflare.com')
    .split(',')
    .map((host) => host.trim())
    .filter(Boolean)

  return {
    envDir: '..',
    plugins: [react()],
    define: {
      'import.meta.env.DISCORD_CLIENT_ID': JSON.stringify(env.DISCORD_CLIENT_ID ?? ''),
      'import.meta.env.DISCORD_GUILD_ID': JSON.stringify(env.DISCORD_GUILD_ID ?? ''),
    },
    server: {
      host: true,
      port: 5173,
      allowedHosts,
      proxy: {
        '/api': 'http://localhost:3000',
        '/ws': { target: 'ws://localhost:3000', ws: true },
      },
    },
  }
})
