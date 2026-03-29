# Embedded Activity Landscape

## Similar projects

- `discord/embedded-app-sdk`: official SDK and examples for Activities.
- `jayxdcode/dcma`: Discord Music Activity with a separate bot/backend split.
- `colyseus/discord-activity`: multiplayer Activity template with client/server separation.
- `Furnyr/Dissonity`: Unity-based Activity SDK wrapper and template.
- `ShabbirHasan1/discord-egui`: native UI experimentation inside Activities.

## What this repo does differently

- Keeps Discord bot, Activity frontend, and backend in one Rust repo.
- Reuses one auth/session flow for bot and Activity state.
- Keeps voice diagnostics, traces, and join state in the same event stream.
- Targets both local Activity testing and Discord-embedded production behavior.

## Potentially unique capabilities

- Bot voice events can be traced directly into Activity UI without another service.
- DAVE/voice diagnostics can be surfaced live while testing in Discord.
- The same repo can evolve from a tutorial template into a voice-aware platform.

## Template guidance

- Treat `voice-diag` as the canonical developer command for the voice stack.
- Keep `README.md` and `LOCAL_TEMPLATE.md` focused on the shortest path to a working Activity.
- Use the docs under `docs/` for deeper operational and review notes.
