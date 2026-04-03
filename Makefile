# Project name (override via environment variable or .env file)
PROJECT_NAME ?= abdrust

.PHONY: dev dev-front dev-back tunnel build check cleanup-commands

dev-back:
	cd backend && cargo run -p $(PROJECT_NAME) --bin $(PROJECT_NAME)

dev-front:
	cd frontend && npm run dev

tunnel:
	./scripts/run-tunnel.sh

dev:
	make dev-back &
	make dev-front

build:
	cd frontend && npm run build
	cd backend && cargo build --release -p $(PROJECT_NAME)

check:
	cd backend && cargo check -p $(PROJECT_NAME)
	cd frontend && npm run build

cleanup-commands:
	cd backend && cargo run -p $(PROJECT_NAME) --bin cleanup_commands
