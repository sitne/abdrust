# Voice Stack Plan

## Goal

Rebuild the bot/voice boundary so this project can survive Discord platform changes while still delivering the current `/voice join` / Activity diagnostics workflow.

## Current State

- The backend still uses `twilight` for gateway/HTTP and `songbird` for voice (`backend/app/src/main.rs`, `backend/app/src/voice.rs`).
- Discord voice channels now require DAVE/E2EE support. Discord's voice docs and `protocol.md` both require clients to present `max_dave_protocol_version` and support MLS/DAVE transitions.
- The current stack fails on `4017`, which is the voice close code for `E2EE/DAVE protocol required`.
- `songbird` has an open upstream issue for this exact gap (`E2EE/DAVE protocol required`).

## Research Results

### Candidate libraries

- `serenity` + `poise`
  - Strong bot framework choice.
  - Good fit for slash commands and long-term maintenance.
  - Does not solve DAVE voice by itself.
- `Snazzah/davey`
  - Rust DAVE implementation using OpenMLS.
  - Better aligned with Discord's current voice requirements than `songbird`.
  - Still early-stage and focused on DAVE primitives rather than full bot + voice productization.
- `Karmanya03/Sigil`
  - Also targets DAVE/E2EE, but is WIP and currently more experimental.
- `OpenMLS`
  - Good MLS building block if we need to implement protocol pieces ourselves.
  - Not a voice gateway solution on its own.

### Verdict

- For bot commands: `serenity + poise` is the safest mainstream choice.
- For voice: `davey` is the best current Rust candidate to evaluate first.
- If `davey` cannot cover the full flow, the fallback is to keep a thin custom voice layer and use `OpenMLS` as a lower-level dependency only where needed.

## Proposed Architecture

### 1. Separate the layers

- `bot-core`: Discord gateway, slash commands, guild/voice-state tracking.
- `voice-core`: join/leave/connect/reconnect, DAVE negotiation, audio receive/send, future protocol upgrades.
- `activity-api`: HTTP + WS event stream for the embedded Activity.

### 2. Introduce local traits

Hide Discord-specific crates behind local traits so we can swap implementations later without rewriting the app.

- `BotGateway`
  - current implementation can wrap Twilight/Poise/Serenity.
- `VoiceEngine`
  - methods for `join`, `leave`, `publish_speaking`, `on_gateway_event`, `on_voice_event`.
- `ActivityTransport`
  - methods for `subscribe`, `broadcast`, `publish_join_result`, `publish_stream_snapshot`.

### 3. Make DAVE the explicit contract

- Treat `4017` as a first-class unsupported state, not a transient join failure.
- Add a preflight check for DAVE-required channels.
- Surface a clear UI state: `unsupported`, `joining`, `joined`, `failed`.
- Keep DAVE protocol version and supported capabilities in the backend state model.

## Fix Plan

### Phase 1: stop the current pain

- Detect `4017` earlier and fail fast with a clear message.
- Make join state deterministic so the UI never stays on `thinking` forever.
- Keep leave working as-is.

### Phase 2: decouple the stack

- Move current `twilight` and `songbird` usage behind local traits.
- Add capability structs for voice protocol support and current session state.
- Keep Activity payloads versioned.

### Phase 3: evaluate `davey`

- Prototype a small adapter around `davey` for join/leave and DAVE negotiation.
- Confirm whether it can cover the needed voice lifecycle and receive flow.
- If it works, replace the current voice engine.

### Phase 4: migrate bot framework if needed

- If the Twilight gateway surface becomes a maintenance burden, move command handling to `serenity + poise`.
- Do this only after the voice path is stable.

## Practical Risks

- DAVE is still evolving, so any direct protocol implementation needs a clean upgrade path.
- `davey` is promising but not yet a fully proven production voice stack.
- `OpenMLS` is a building block, not a complete voice solution.

## Recommended Next Step

Prototype a `VoiceEngine` abstraction and wire a `davey`-backed experimental implementation behind it, while keeping the current stack as the fallback until parity is reached.
