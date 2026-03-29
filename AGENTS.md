# AGENTS.md

## Purpose

This repository exists to keep the Discord bot and the embedded Activity in one Rust project for the long term.
The goal is not a quick prototype: it is a maintainable platform where the Activity UI and the bot voice layer can evolve together.

## Working Principles

- Prefer changes that are easy to understand after a fresh clone.
- Keep bot and Activity code aligned when behavior crosses the boundary.
- Favor small, local edits over broad rewrites unless a rewrite clearly reduces complexity.
- Optimize for future maintainability, not just the shortest path to a passing build.
- Assume Discord APIs and voice behavior will continue to change; keep the architecture flexible.

## Repository Rules

- Treat the root `.env` as the single source of truth for local configuration.
- Do not introduce hidden configuration files or duplicate env sources.
- Preserve the integrated bot + Activity workflow unless the user explicitly requests a split.
- When changing voice behavior, verify the bot side and the Activity side together.
- If a change affects Discord integration, keep logs and debugging output useful.

## GitHub Research

- Use `gh search repos`, `gh search issues`, `gh search prs`, and `gh search code` when choosing Rust or Discord libraries.
- Prefer official repository sources, README files, Cargo manifests, and upstream issues over secondary summaries.
- Treat search results as pointers; verify important details in the actual repository contents before deciding.
- When comparing implementations, collect a small set of candidate repos first, then inspect their code patterns and maintenance state.

## Validation

- Backend changes must pass `cargo test`.
- Frontend changes must pass `npm run build`.
- If a change touches both halves, run both validations.

## Editing Guidance

- Keep code readable for someone who has only seen the project once before.
- Prefer explicit names and clear data flow over clever abstractions.
- Add debug-only instrumentation when a bug is unclear, then remove it once the cause is confirmed.
- Keep temporary fixes isolated and documented so they can be replaced cleanly later.

## Development Focus

- The long-term objective is an integrated Discord experience: Activity visuals plus bot-driven voice interaction.
- The project should remain easy to clone, understand, and extend by a single developer.
- Avoid locking the implementation too tightly to one Discord library if a future change would improve longevity.
