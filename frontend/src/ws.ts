import { isToolingMode } from './dev'

type Listener<T> = (event: T) => void
type StatusListener<T> = (event: T) => void
type ParsedMessage = { type?: string; data?: unknown }

export class WsClient<TStatus = unknown, TEvent = unknown> {
  private socket?: WebSocket
  private reconnectTimer?: number
  private listeners = new Set<Listener<TEvent>>()
  private statusListeners = new Set<StatusListener<TStatus>>()
  private reconnectAttempts = 0
  private guildId?: string
  private instanceId?: string
  private shouldReconnect = true
  private pendingSessionId?: string
  private readonly toolingMode = isToolingMode()

  constructor(private url: string) {}

  on(listener: Listener<TEvent>) {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  onStatus(listener: StatusListener<TStatus>) {
    this.statusListeners.add(listener)
    return () => this.statusListeners.delete(listener)
  }

  connect(guildId: string, instanceId?: string) {
    this.guildId = guildId
    this.instanceId = instanceId
    if (this.toolingMode) return
    this.shouldReconnect = true
    this.open()
  }

  setSessionId(sessionId: string) {
    this.pendingSessionId = sessionId
    if (this.toolingMode) return
    this.send({ type: 'session', session_id: sessionId })
  }

  close() {
    this.shouldReconnect = false
    if (this.reconnectTimer) {
      window.clearTimeout(this.reconnectTimer)
      this.reconnectTimer = undefined
    }
    if (this.toolingMode) return
    this.socket?.close()
  }

  send(message: unknown) {
    if (this.toolingMode) return
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(JSON.stringify(message))
    }
  }

  private open() {
    if (!this.shouldReconnect || this.toolingMode) return
    this.socket = new WebSocket(this.url)
    this.socket.onopen = () => {
      this.reconnectAttempts = 0
      if (this.pendingSessionId) this.send({ type: 'session', session_id: this.pendingSessionId })
      if (this.guildId) this.send({ type: 'subscribe', guild_id: this.guildId, instance_id: this.instanceId })
    }
    this.socket.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data) as ParsedMessage
        if (data?.type === 'bot_ready') {
          this.statusListeners.forEach((listener) => listener(data.data as TStatus))
        }
        this.listeners.forEach((listener) => listener(data as TEvent))
      } catch {}
    }
    this.socket.onclose = () => this.reconnect()
    this.socket.onerror = () => this.socket?.close()
  }

  private reconnect() {
    if (!this.shouldReconnect) return
    const delay = Math.min(1000 * 2 ** this.reconnectAttempts++, 10000)
    this.reconnectTimer = window.setTimeout(() => this.open(), delay)
  }
}
