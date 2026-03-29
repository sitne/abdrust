function isLocalHost(hostname: string) {
  return hostname === 'localhost' || hostname === '127.0.0.1' || hostname === '::1' || hostname.endsWith('.localhost')
}

export function isToolingMode() {
  if (typeof window === 'undefined') return false
  const url = new URL(window.location.href)
  return url.searchParams.get('tooling') === '1' && isLocalHost(url.hostname)
}

export function makeLocalGuildId() {
  return import.meta.env.DISCORD_GUILD_ID || import.meta.env.VITE_GUILD_ID || '123456789012345678'
}
