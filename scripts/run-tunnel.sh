#!/usr/bin/env bash
set -euo pipefail

if ! command -v cloudflared >/dev/null 2>&1; then
  echo "cloudflared is not installed. Run scripts/install-cloudflared.sh first."
  exit 1
fi

cloudflared tunnel --url http://localhost:5173
