# Security and Multi-Guild Notes

- WebSocket events are only trusted after `session_ok`, and guild-scoped events are filtered by the session's guild allowlist.
- Activity API responses must use the private `/api/private/guild/{guild_id}/voice` path with an `x-abdrust-session-id` header.
- Voice state, metadata, and join state are tracked per guild to avoid cross-guild leaks.
- `voice` commands remain guild-scoped; DM activity is for entry only, not voice control.
- Default CORS is enabled only for local dev; Discord mode falls back to explicit origins.
