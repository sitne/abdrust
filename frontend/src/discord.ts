import { DiscordSDK } from '@discord/embedded-app-sdk'
import { isToolingMode } from './dev'

let discordSdk: DiscordSDK | null = null
let initPromise: Promise<DiscordSDK> | null = null

function getClientId() {
  const clientId = import.meta.env.DISCORD_CLIENT_ID
  if (!clientId) {
    throw new Error('DISCORD_CLIENT_ID is missing')
  }
  return clientId as string
}

async function getDiscordSdk() {
  if (discordSdk) return discordSdk
  if (!initPromise) {
    initPromise = (async () => {
      const clientId = getClientId()
      discordSdk = new DiscordSDK(clientId)
      await discordSdk.ready()
      return discordSdk
    })()
  }

  return initPromise
}

export async function initDiscord() {
  if (isToolingMode()) {
    sessionStorage.setItem('abdrust-session-id', 'tooling-session')
    sessionStorage.setItem('abdrust-instance-id', 'tooling-instance')
    return 'tooling-session'
  }

  const discordSdk = await getDiscordSdk()

  // Store instanceId — this uniquely identifies this Activity session
  // Format: i-{launch_id}-gc-{guild_id}-{channel_id}
  sessionStorage.setItem('abdrust-instance-id', discordSdk.instanceId)

  const { code } = await discordSdk.commands.authorize({
    client_id: getClientId(),
    response_type: 'code',
    state: '',
    prompt: 'none',
    scope: ['identify', 'guilds'],
  })

  const response = await fetch('/api/token', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ code }),
  })

  if (!response.ok) {
    const detail = await response.text().catch(() => '')
    throw new Error(`token exchange failed: ${response.status} ${detail}`.trim())
  }

  const payload = await response.json().catch(() => null)
  if (!payload || typeof payload !== 'object' || typeof (payload as { access_token?: unknown }).access_token !== 'string' || typeof (payload as { session_id?: unknown }).session_id !== 'string') {
    throw new Error('token exchange returned no access token')
  }

  const { access_token, session_id } = payload as { access_token: string; session_id: string }
  await discordSdk.commands.authenticate({ access_token })
  sessionStorage.setItem('abdrust-session-id', session_id)
  return access_token
}

export function getSessionId() {
  return sessionStorage.getItem('abdrust-session-id') ?? ''
}

export function getInstanceId() {
  return sessionStorage.getItem('abdrust-instance-id') ?? ''
}
