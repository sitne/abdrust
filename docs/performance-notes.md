# Performance Notes

## Backend

- Throttle stream snapshots to avoid spamming voice updates on every tick.
- Keep voice state grouped by guild to reduce accidental cross-guild churn.
- Emit decoded-voice logs at `info` only once per second; keep repeats at `debug`.

## Frontend

- Reduce derived state churn by using a compact `VoiceStreamDigest` instead of recomputing counters repeatedly.
- Keep large event payloads in compact rolling buffers (`slice(0, N)`) rather than unbounded arrays.

## Possible future migrations

- If voice UI grows, move event rendering to virtualized lists.
- If activity pages grow, consider lighter state management and less JSON mirroring.
