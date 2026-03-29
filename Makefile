.PHONY: dev dev-front dev-back tunnel build check cleanup-commands

dev-back:
	cd backend && cargo run -p abdrust --bin abdrust

dev-front:
	cd frontend && npm run dev

tunnel:
	./scripts/run-tunnel.sh

dev:
	make dev-back &
	make dev-front

build:
	cd frontend && npm run build
	cd backend && cargo build --release -p abdrust

check:
	cd backend && cargo check -p abdrust
	cd frontend && npm run build

cleanup-commands:
	cd backend && cargo run -p abdrust --bin cleanup_commands
