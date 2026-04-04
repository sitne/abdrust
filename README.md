# abdrust

A foundation for running a Discord Bot and Embedded Activity together in a single Rust project. Built for long-term maintenance — the Activity UI and the bot voice layer evolve in one codebase, with one token, one repo.

## What You Get

- Develop bot and Activity in the same project
- `/voice join` / `/voice leave` / `/voice status` slash commands
- Activity displays bot status and real-time voice events
- Runs as a Discord Embedded App

## Key Features

- Bot, backend, and Activity in one Rust repo
- Voice receive, decrypt, and diagnostics piped directly to the Activity UI
- `/voice-diag` for one-stop DAVE / join / receive status checks
- Abstracted voice layer — swap implementations without touching the rest

## Quick Start

```bash
git clone <this-repo> abdrust
cd abdrust
cp .env.example .env
```

Edit `.env` with your Discord credentials, then start:

```bash
make dev
```

Or run each half separately:

```bash
cd backend && cargo run -p abdrust
cd frontend && npm run dev
```

## Required Configuration

| Variable | Description |
|---|---|
| `DISCORD_TOKEN` | Bot token |
| `DISCORD_CLIENT_ID` | Application client ID |
| `DISCORD_CLIENT_SECRET` | OAuth client secret |
| `DISCORD_REDIRECT_URI` | OAuth redirect URI |
| `DISCORD_GUILD_ID` | Development guild ID |
| `ACTIVITY_MODE` | `local` for local dev, `discord` for production |

## Running the Activity

Due to Discord's CSP, the Activity requires a `cloudflared` tunnel for local testing:

```bash
make tunnel
```

Set the displayed `https://*.trycloudflare.com` URL in the Discord Developer Portal under URL Mapping `/`. Launch from the Discord Activity Shelf — do not use URL Override.

## Verification Checklist

- backend: `GET /api/health` responds
- bot: connected to Discord Gateway
- bot: `/abdrust-debug` responds
- voice: `/voice join` succeeds
- activity: `initDiscord()` → `POST /api/token` → `ws` → `bot: ready` all pass

## Running Tests

```bash
# Backend
cargo test

# Frontend
npm run build

# Both
make check
```

## Browser Tooling

See `docs/browser-tooling-playbook.md` for the full workflow.

```bash
cd frontend && npm run test:e2e     # Playwright
cd frontend && npm run test:a11y    # axe accessibility
cd frontend && npm run lighthouse   # Lighthouse CI
```

These run in `?tooling=1` local mode — Discord auth, WS, and private APIs are disabled. Only available on localhost.

## `.env` Handling

- Only the root `.env` is used
- Backend reads `../.env`
- Frontend reads the root `.env` via Vite at build time

## Additional Commands

```bash
make check            # cargo check + npm run build
make cleanup-commands # clean up registered slash commands
```

## Project Structure

```
abdrust/
├── backend/app/src/    # Rust backend (bot, voice engine, HTTP server)
├── frontend/src/       # React + TypeScript Activity UI
├── docs/               # Architecture, ADRs, dev log
├── scripts/            # Utility scripts
├── AGENTS.md           # AI agent instructions
└── .env                # Single source of truth for config
```

## Design Principles

- Prefer changes that are easy to understand after a fresh clone
- Keep bot and Activity code aligned when behavior crosses the boundary
- Optimize for future maintainability, not the shortest path to a passing build
- Assume Discord APIs will change — keep the architecture flexible

## License

- Repository code and original documentation: MIT, see `LICENSE`
- `protocol.md`: separate CC BY-NC-SA 4.0 notice, see `THIRD_PARTY_NOTICES.md`

## Languages

- [English](README.md)
- [日本語](README.ja.md)
- [中文](README.zh.md)
- [Español](README.es.md)
