#!/usr/bin/env bash
set -euo pipefail

if command -v cloudflared >/dev/null 2>&1; then
  echo "cloudflared is already installed"
  exit 0
fi

if command -v apt-get >/dev/null 2>&1; then
  apt-get update
  apt-get install -y curl gnupg
  curl -fsSL https://pkg.cloudflare.com/cloudflare-main.gpg | gpg --dearmor -o /usr/share/keyrings/cloudflare-main.gpg
  echo "deb [signed-by=/usr/share/keyrings/cloudflare-main.gpg] https://pkg.cloudflare.com/cloudflared jammy main" >/etc/apt/sources.list.d/cloudflared.list
  apt-get update
  apt-get install -y cloudflared
  exit 0
fi

echo "Install cloudflared manually from https://github.com/cloudflare/cloudflared/releases"
exit 1
