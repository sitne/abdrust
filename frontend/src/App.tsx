import { useEffect, useMemo, useState } from 'react'
import { getSessionId, getInstanceId, initDiscord } from './discord'
import { isToolingMode, makeLocalGuildId } from './dev'
import { WsClient } from './ws'

type VoiceUser = {
  user_id: string
  user_name?: string | null
  display_name?: string | null
  avatar_url?: string | null
  channel_id?: string | null
  speaking?: boolean
  ssrc?: number
  samples?: number
}

type BotStatus = {
  status?: string
  application_id?: string
  guild_id?: string | null
  commands?: string[]
  voice_capabilities?: {
    engine_name: string
    supports_dave: boolean
    max_dave_protocol_version: number
  }
}

type VoiceDiagnostics = {
  guild_id: string
  voice?: { guild_id: string; channel_id?: string | null } | null
  join_state?: { status: string; [key: string]: unknown }
  voice_capabilities?: { engine_name: string; supports_dave: boolean; max_dave_protocol_version: number }
  signal_trace?: { guild_id: string; stage: string; message: string; user_id?: string | null; channel_id?: string | null; ssrc?: number | null } | null
  receive_trace?: { guild_id: string; kind: string; message: string; user_id?: string | null; ssrc?: number | null; sequence?: number | null; timestamp?: number | null; payload_len?: number | null; payload_offset?: number | null; payload_end_pad?: number | null; has_dave_marker?: boolean | null; decoded_users?: number | null; silent_users?: number | null; audio_frames?: number | null; decoded_samples?: number | null } | null
}

type VoiceStateUpdateEvent = { type: 'VoiceStateUpdate'; guild_id: string; user_id: string; channel_id: string | null; user_name?: string | null; display_name?: string | null; avatar_url?: string | null }
type VoiceSpeakingEvent = { type: 'VoiceSpeaking'; guild_id: string; user_id: string; channel_id: string | null; user_name?: string | null; display_name?: string | null; avatar_url?: string | null; ssrc: number; speaking: boolean }
type VoiceAudioFrameEvent = { type: 'VoiceAudioFrame'; guild_id: string; user_id: string; ssrc: number; samples: number }
type VoiceStreamEvent = { type: 'VoiceStream'; guild_id: string; users: VoiceUser[]; audio_frames: number }
type VoiceReceiveTraceEvent = { type: 'VoiceReceiveTrace'; trace: { guild_id: string; kind: string; message: string; user_id?: string | null; ssrc?: number | null; sequence?: number | null; timestamp?: number | null; payload_len?: number | null; payload_offset?: number | null; payload_end_pad?: number | null; has_dave_marker?: boolean | null; decoded_users?: number | null; silent_users?: number | null; audio_frames?: number | null; decoded_samples?: number | null } }
type VoiceSignalTraceEvent = { type: 'VoiceSignalTrace'; trace: { guild_id: string; stage: string; message: string; user_id?: string | null; channel_id?: string | null; ssrc?: number | null } }
type VoiceJoinStateEvent = { type: 'VoiceJoinState'; state: { status: 'idle'; guild_id: string } | { status: 'joining'; guild_id: string; user_id: string; channel_id: string; message: string } | { status: 'joined'; guild_id: string; user_id: string; channel_id: string; message: string } | { status: 'unsupported'; guild_id: string; user_id: string; channel_id: string; message: string; failure_kind: string; causes: string[]; dave_required?: boolean; engine_name?: string; max_dave_protocol_version?: number } | { status: 'failed'; guild_id: string; user_id: string; channel_id: string; message: string; causes: string[] } }
type VoiceJoinRequestedEvent = { type: 'VoiceJoinRequested'; guild_id: string; user_id: string; channel_id: string }
type VoiceJoinResultEvent = { type: 'VoiceJoinResult'; guild_id: string; user_id: string; ok: boolean; message: string }
type CustomEvent = { type: 'Custom'; name: string; payload: { message?: string; causes?: string[]; failure_kind?: string } & Record<string, unknown> }
type MessageCreateEvent = { type: 'MessageCreate'; guild_id: string; content: string; author: string }
type BackendEvent = VoiceStateUpdateEvent | VoiceSpeakingEvent | VoiceAudioFrameEvent | VoiceStreamEvent | VoiceReceiveTraceEvent | VoiceSignalTraceEvent | VoiceJoinStateEvent | VoiceJoinRequestedEvent | VoiceJoinResultEvent | CustomEvent | MessageCreateEvent
type WsMessage = { type: 'bot_ready'; data: BotStatus } | { type: 'event'; data: BackendEvent }

function isBackendEvent(value: unknown): value is BackendEvent {
  if (!value || typeof value !== 'object') return false
  const event = value as { type?: unknown }
  return typeof event.type === 'string'
}

function isWsMessage(value: unknown): value is WsMessage {
  if (!value || typeof value !== 'object') return false
  const message = value as { type?: unknown; data?: unknown }
  if (message.type === 'bot_ready') return !!message.data && typeof message.data === 'object'
  if (message.type === 'event') return isBackendEvent(message.data)
  return false
}

function renderVoiceLabel(user: VoiceUser) {
  return user.display_name ?? user.user_name ?? user.user_id
}

const panel = {
  border: '1px solid rgba(255,255,255,0.12)',
  borderRadius: 20,
  background: 'rgba(8, 12, 24, 0.72)',
  backdropFilter: 'blur(18px)',
  boxShadow: '0 24px 80px rgba(0,0,0,0.35)',
} as const

export default function App() {
  const toolingMode = isToolingMode()
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [botStatus, setBotStatus] = useState<BotStatus>(() => (toolingMode ? { status: 'tooling' } : { status: 'starting' }))
  const [events, setEvents] = useState<WsMessage[]>([])
  const [voiceUsers, setVoiceUsers] = useState<Record<string, VoiceUser>>({})
  const [lastFrame, setLastFrame] = useState<{ userId: string; samples: number; at: number } | null>(null)
  const [joinStatus, setJoinStatus] = useState<'idle' | 'joining' | 'joined' | 'unsupported' | 'error'>('idle')
  const [joinMessage, setJoinMessage] = useState<string>('')
  const [joinCauses, setJoinCauses] = useState<string[]>([])
  const [signalTraces, setSignalTraces] = useState<VoiceSignalTraceEvent['trace'][]>([])
  const [receiveTraces, setReceiveTraces] = useState<VoiceReceiveTraceEvent['trace'][]>([])
  const [voiceDiagnostics, setVoiceDiagnostics] = useState<VoiceDiagnostics | null>(null)
  const [voiceStreamDigest, setVoiceStreamDigest] = useState<{ users: number; speaking: number; frames: number } | null>(null)
  const ws = useMemo(() => new WsClient<BotStatus, WsMessage>('/ws'), [])
  const guildId = makeLocalGuildId()

  useEffect(() => {
    let disposed = false
    let unsubscribeStatus = () => {}
    let unsubscribeEvents = () => {}
    initDiscord()
      .then(() => {
        if (disposed) return
        if (toolingMode) {
          setBotStatus({ status: 'tooling' })
          return
        }
        ws.connect(guildId, getInstanceId() || undefined)
        const sessionId = getSessionId()
        if (sessionId) {
          ws.setSessionId(sessionId)
        }
        unsubscribeStatus = ws.onStatus((status) => setBotStatus(status ?? { status: 'ready' }))
        unsubscribeEvents = ws.on((event) => {
          if (!isWsMessage(event)) return
          const typed = event
          setEvents((prev) => [typed, ...prev].slice(0, 30))
          if (typed.type !== 'event') return

          const payload = typed.data
          if (payload.type === 'VoiceStream') {
            const nextUsers: Record<string, VoiceUser> = {}
            payload.users.forEach((user) => {
              nextUsers[user.user_id] = user
            })
            setVoiceUsers(nextUsers)
            setVoiceStreamDigest({
              users: payload.users.length,
              speaking: payload.users.filter((user) => user.speaking).length,
              frames: payload.audio_frames,
            })
          } else if (payload.type === 'VoiceStateUpdate' || payload.type === 'VoiceSpeaking') {
            const userId = payload.user_id
            if (payload.type === 'VoiceStateUpdate' && payload.channel_id === null) {
              setVoiceUsers((prev) => {
                const next = { ...prev }
                delete next[userId]
                return next
              })
              return
            }

            setVoiceUsers((prev) => ({
              ...prev,
              [userId]: {
                ...prev[userId],
                user_id: userId,
                user_name: payload.user_name ?? prev[userId]?.user_name,
                display_name: payload.display_name ?? prev[userId]?.display_name,
                avatar_url: payload.avatar_url ?? prev[userId]?.avatar_url,
                channel_id: payload.channel_id === undefined ? prev[userId]?.channel_id : payload.channel_id,
                speaking: payload.type === 'VoiceSpeaking' ? payload.speaking : prev[userId]?.speaking,
                ssrc: payload.type === 'VoiceSpeaking' ? payload.ssrc : prev[userId]?.ssrc,
              },
            }))
          } else if (payload.type === 'VoiceAudioFrame') {
            setLastFrame({ userId: payload.user_id, samples: payload.samples, at: Date.now() })
          } else if (payload.type === 'VoiceReceiveTrace') {
            setReceiveTraces((prev) => [payload.trace, ...prev].slice(0, 20))
          } else if (payload.type === 'VoiceSignalTrace') {
            setSignalTraces((prev) => [payload.trace, ...prev].slice(0, 20))
          } else if (payload.type === 'VoiceJoinRequested') {
            setJoinStatus('joining')
            setJoinMessage('joining voice channel')
          } else if (payload.type === 'VoiceJoinState') {
            if (payload.state.status === 'idle') {
              setJoinStatus('idle')
              setJoinMessage('')
              setJoinCauses([])
            } else if (payload.state.status === 'joining') {
              setJoinStatus('joining')
              setJoinMessage(payload.state.message)
              setJoinCauses([])
            } else if (payload.state.status === 'joined') {
              setJoinStatus('joined')
              setJoinMessage(payload.state.message)
              setJoinCauses([])
            } else if (payload.state.status === 'unsupported') {
              setJoinStatus('unsupported')
              const version = payload.state.max_dave_protocol_version ?? 0
              const engine = payload.state.engine_name ?? 'unknown'
              setJoinMessage(payload.state.dave_required ? `${payload.state.message} (${engine} / DAVE v${version})` : payload.state.message)
              setJoinCauses(payload.state.causes)
            } else if (payload.state.status === 'failed') {
              setJoinStatus('error')
              setJoinMessage(payload.state.message)
              setJoinCauses(payload.state.causes)
            }
          } else if (payload.type === 'VoiceJoinResult') {
            setJoinStatus((current) => (payload.ok || current === 'unsupported' ? (payload.ok ? 'joined' : current) : 'error'))
            setJoinMessage(payload.message)
          } else if (payload.type === 'Custom' && payload.name === 'voice_joining') {
            setJoinStatus('joining')
            setJoinMessage(String((payload.payload as { message?: unknown }).message ?? 'joining'))
            setJoinCauses([])
          } else if (payload.type === 'Custom' && payload.name === 'voice_joined') {
            setJoinStatus('joined')
            setJoinMessage('voice joined')
            setJoinCauses([])
          } else if (payload.type === 'Custom' && payload.name === 'voice_join_error') {
            setJoinMessage(String(payload.payload.message ?? 'join failed'))
            setJoinCauses(Array.isArray(payload.payload.causes) ? payload.payload.causes.filter((cause): cause is string => typeof cause === 'string') : [])
            setJoinStatus(payload.payload.failure_kind === 'RequiresDave' ? 'unsupported' : 'error')
          }
        })
        fetch(`/api/private/guild/${guildId}/voice`, { headers: sessionId ? { 'x-abdrust-session-id': sessionId } : undefined }).then(async (res) => {
          if (!res.ok) return
          const data = await res.json() as VoiceDiagnostics
          if (!disposed) {
            setVoiceDiagnostics(data)
          if (data.signal_trace) setSignalTraces([data.signal_trace])
          if (data.receive_trace) setReceiveTraces([data.receive_trace])
        }
      }).catch(() => {})
      })
      .catch((err) => !disposed && setError(err instanceof Error ? err.message : 'failed to initialize'))
      .finally(() => !disposed && setLoading(false))
    return () => {
      disposed = true
      unsubscribeStatus()
      unsubscribeEvents()
      ws.close()
    }
  }, [ws, guildId])

  const activeUsers = Object.values(voiceUsers).filter((u) => u.channel_id)

  const userCount = activeUsers.length
  const capabilities = botStatus.voice_capabilities

  return (
    <main style={{ minHeight: '100vh', color: '#f5f7fb', background: 'radial-gradient(circle at top, #1e2947 0, #0a1020 50%, #050814 100%)', padding: 24, fontFamily: 'Inter, system-ui, sans-serif' }}>
      <div style={{ maxWidth: 1200, margin: '0 auto' }}>
        <header style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'end', gap: 16, marginBottom: 24 }}>
          <div>
            <div style={{ fontSize: 12, letterSpacing: 2.4, textTransform: 'uppercase', opacity: 0.72 }}>abdrust activity</div>
            <h1 style={{ margin: '8px 0 0', fontSize: 44, lineHeight: 1.05 }}>Voice diagnostics</h1>
              <p style={{ margin: '10px 0 0', color: 'rgba(245,247,251,0.7)' }}>発話状態と受信音声フレームを Activity 内で確認できます。</p>
          </div>
          <div style={{ ...panel, padding: '14px 18px', minWidth: 240 }}>
            <div style={{ fontSize: 12, opacity: 0.7 }}>bot</div>
            <div style={{ fontSize: 18, fontWeight: 700 }}>{botStatus.status ?? 'ready'}</div>
            <div style={{ fontSize: 12, opacity: 0.7, marginTop: 4 }}>guild {guildId}</div>
            <div style={{ fontSize: 12, opacity: 0.7, marginTop: 4 }}>
              engine {capabilities?.engine_name ?? 'unknown'} · dave {capabilities?.supports_dave ? 'yes' : 'no'} · v{capabilities?.max_dave_protocol_version ?? 0}
            </div>
            <div style={{ fontSize: 12, opacity: 0.7, marginTop: 4 }}>join {joinStatus}{joinMessage ? ` · ${joinMessage}` : ''}</div>
          </div>
        </header>

        {loading ? <section style={{ ...panel, padding: 24 }}>initializing...</section> : null}
        {error ? <section style={{ ...panel, padding: 24, marginBottom: 16, borderColor: 'rgba(255,120,120,0.35)' }}>error: {error}</section> : null}
        {toolingMode ? <section style={{ ...panel, padding: 16, marginBottom: 16, borderColor: 'rgba(92, 202, 255, 0.35)', background: 'rgba(14, 28, 48, 0.72)' }}>
          browser tooling mode: Discord auth, websocket, and private API calls are disabled.
        </section> : null}
        {joinStatus === 'error' || joinStatus === 'unsupported' ? <section style={{ ...panel, padding: 16, marginBottom: 16, borderColor: joinStatus === 'unsupported' ? 'rgba(255,190,90,0.45)' : 'rgba(255,120,120,0.35)', background: joinStatus === 'unsupported' ? 'rgba(72, 48, 12, 0.55)' : 'rgba(68, 12, 20, 0.55)' }}>
          <div style={{ fontWeight: 700, marginBottom: 6 }}>{joinStatus === 'unsupported' ? 'voice requires DAVE' : 'voice join failed'}</div>
          <div style={{ fontSize: 13, opacity: 0.9 }}>{joinMessage}</div>
          {joinCauses.length > 0 ? <ul style={{ margin: '10px 0 0', paddingLeft: 18, fontSize: 12, opacity: 0.8 }}>{joinCauses.map((cause) => <li key={cause}>{cause}</li>)}</ul> : null}
        </section> : null}

        <section style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1.4fr) minmax(320px, 0.9fr)', gap: 16 }}>
          <div style={{ ...panel, padding: 20 }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
              <h2 style={{ margin: 0, fontSize: 20 }}>Voice stream</h2>
              <div style={{ fontSize: 12, opacity: 0.7 }}>{userCount} active</div>
            </div>
            <div style={{ display: 'grid', gap: 12 }}>
              {activeUsers.length === 0 ? (
                <div style={{ padding: 20, borderRadius: 16, background: 'rgba(255,255,255,0.04)', color: 'rgba(245,247,251,0.65)' }}>
                  まだ発話がありません。`/voice join` して話すとここに表示されます。
                </div>
              ) : activeUsers.map((user) => (
                <article key={user.user_id} style={{ display: 'flex', alignItems: 'center', gap: 14, padding: 14, borderRadius: 18, background: 'rgba(255,255,255,0.05)', border: user.speaking ? '1px solid rgba(92, 202, 255, 0.5)' : '1px solid rgba(255,255,255,0.08)' }}>
                  <div style={{ width: 48, height: 48, borderRadius: 999, overflow: 'hidden', background: 'linear-gradient(135deg, #4f7cff, #7ee0ff)', flex: '0 0 auto' }}>
                    {user.avatar_url ? <img src={user.avatar_url} alt="avatar" style={{ width: '100%', height: '100%', objectFit: 'cover' }} /> : null}
                  </div>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                      <strong style={{ fontSize: 16 }}>{renderVoiceLabel(user)}</strong>
                      <span style={{ fontSize: 11, padding: '4px 8px', borderRadius: 999, background: user.speaking ? 'rgba(74, 222, 128, 0.18)' : 'rgba(255,255,255,0.08)' }}>{user.speaking ? 'speaking' : 'listening'}</span>
                    </div>
                    <div style={{ marginTop: 4, fontSize: 13, opacity: 0.72 }}>{user.user_name ?? user.user_id} · channel {user.channel_id ?? '-'}</div>
                  </div>
                  <div style={{ textAlign: 'right', fontSize: 12, opacity: 0.78 }}>
                    <div>ssrc {user.ssrc ?? '-'}</div>
                    <div>{user.samples ? `${user.samples} samples` : ''}</div>
                  </div>
                </article>
              ))}
            </div>
          </div>

          <div style={{ display: 'grid', gap: 16 }}>
            <section style={{ ...panel, padding: 20 }}>
              <h2 style={{ margin: '0 0 10px', fontSize: 20 }}>Audio frames</h2>
              <div style={{ fontSize: 38, fontWeight: 800, lineHeight: 1 }}>{lastFrame?.samples ?? 0}</div>
              <div style={{ marginTop: 6, opacity: 0.72, fontSize: 13 }}>last frame samples</div>
              <div style={{ marginTop: 12, fontSize: 12, opacity: 0.6 }}>{lastFrame ? `user ${lastFrame.userId} · ${new Date(lastFrame.at).toLocaleTimeString()}` : 'waiting for voice data'}</div>
              {voiceStreamDigest ? <div style={{ marginTop: 12, fontSize: 12, opacity: 0.7 }}>stream {voiceStreamDigest.users} users · {voiceStreamDigest.speaking} speaking · {voiceStreamDigest.frames} frames</div> : null}
            </section>

            <section style={{ ...panel, padding: 20 }}>
              <h2 style={{ margin: '0 0 12px', fontSize: 20 }}>Recent events</h2>
              <div style={{ display: 'grid', gap: 10, maxHeight: 320, overflow: 'auto' }}>
                {events.length === 0 ? <div style={{ opacity: 0.6, fontSize: 13 }}>no events yet</div> : events.map((event, idx) => <pre key={idx} style={{ margin: 0, padding: 12, borderRadius: 14, background: 'rgba(255,255,255,0.04)', fontSize: 11, whiteSpace: 'pre-wrap' }}>{JSON.stringify(event, null, 2)}</pre>)}
              </div>
            </section>

            <section style={{ ...panel, padding: 20 }}>
              <h2 style={{ margin: '0 0 12px', fontSize: 20 }}>Signal trace</h2>
              {voiceDiagnostics?.signal_trace ? <div style={{ marginBottom: 10, fontSize: 12, opacity: 0.8 }}>last: {voiceDiagnostics.signal_trace.stage} · {voiceDiagnostics.signal_trace.message}</div> : null}
              <div style={{ display: 'grid', gap: 10, maxHeight: 240, overflow: 'auto' }}>
                {signalTraces.length === 0 ? <div style={{ opacity: 0.6, fontSize: 13 }}>no signal traces yet</div> : signalTraces.map((trace, idx) => <pre key={idx} style={{ margin: 0, padding: 12, borderRadius: 14, background: 'rgba(255,255,255,0.04)', fontSize: 11, whiteSpace: 'pre-wrap' }}>{JSON.stringify(trace, null, 2)}</pre>)}
              </div>
            </section>

            <section style={{ ...panel, padding: 20 }}>
              <h2 style={{ margin: '0 0 12px', fontSize: 20 }}>Receive trace</h2>
              {voiceDiagnostics?.receive_trace ? <div style={{ marginBottom: 10, fontSize: 12, opacity: 0.8 }}>last: {voiceDiagnostics.receive_trace.kind} · {voiceDiagnostics.receive_trace.message}</div> : null}
              <div style={{ display: 'grid', gap: 10, maxHeight: 240, overflow: 'auto' }}>
                {receiveTraces.length === 0 ? <div style={{ opacity: 0.6, fontSize: 13 }}>no receive traces yet</div> : receiveTraces.map((trace, idx) => <pre key={idx} style={{ margin: 0, padding: 12, borderRadius: 14, background: 'rgba(255,255,255,0.04)', fontSize: 11, whiteSpace: 'pre-wrap' }}>{JSON.stringify(trace, null, 2)}</pre>)}
              </div>
            </section>
          </div>
        </section>
      </div>
    </main>
  )
}
