# Review Checklist

## 1. Correctness

- State transitions are explicit.
- Success and failure paths both update state.
- Guild/user/channel relationships stay consistent.

## 2. Auth and Isolation

- API/WS access is session-bound.
- Guild-scoped data never crosses guild boundaries.
- Discord mode and local mode behave differently on purpose.

## 3. Maintainability

- Each module has one clear job.
- Shared helpers stay small and reusable.
- Avoid duplicated logic across bot, backend, and UI.
- Prefer AppState methods over direct `state.bot.*` access from feature code.

## 4. Observability

- Important transitions emit logs or traces.
- Failures show root causes, not just generic errors.
- UI mirrors the backend’s current state clearly.

## 5. Docs and UX

- README matches reality.
- Commands explain what the template can do.
- New contributors can find the main path quickly.

## Review loop

1. Pick one checklist item.
2. Inspect the related code paths.
3. Fix the smallest useful issue.
4. Run backend tests and frontend build if touched.
5. Record what changed and move to the next item.
