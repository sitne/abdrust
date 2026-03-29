# Local Template Flow

1. `git clone <repo> abdrust`
2. `cp .env.example .env`
3. Discord Developer Portal で Bot / OAuth2 / Embedded App を設定
4. `.env` に以下を記入
    - `DISCORD_TOKEN`
    - `DISCORD_CLIENT_ID`
    - `DISCORD_CLIENT_SECRET`
    - `DISCORD_GUILD_ID`
    - `ACTIVITY_MODE=local`
5. `make dev` で backend と frontend を起動
6. `make tunnel` を別ターミナルで起動（未導入なら `./scripts/install-cloudflared.sh`）
7. 表示された `https://*.trycloudflare.com` を Developer Portal の URL Mapping `/` に設定
8. Discord の Activity から起動し、`/api/token`・`ws`・`/voice-diag` を確認
9. `voice join` → `voice status` → `voice-diag` の順で、voice 接続と診断を確認
10. 古いコマンドが残っている場合は `make cleanup-commands` を実行（global/guild 両方、entry point は残す）

## このテンプレのポイント

- bot / backend / Activity を1リポジトリで運ぶ
- voice の状態と診断を Activity UI に流す
- DAVE 対応や voice 実装差し替えを後から入れやすくする
