# abdrust

Discord Bot と Embedded Activity を、1つのRustプロジェクトとして長期運用するための土台です。
Activity の映像UIと、Bot の音声処理やDiscord連携を、同じトークン・同じリポジトリで育てていくことを目的にしています。

## できること

- Bot と Activity を同一プロジェクトで開発できる
- `/voice join` / `/voice leave` / `/voice status` を扱う
- Activity 側で bot の状態や音声イベントを表示する
- Discord の Embedded App として起動する

## このテンプレの見どころ

- bot / backend / Activity を1つのRust repoでまとめて扱える
- voice の受信・復号・診断を Activity にそのまま流せる
- `/voice-diag` で DAVE / join / receive の状態を一括確認できる
- 音声層を抽象化しているので、将来の実装差し替えに追従しやすい

## セットアップ

```bash
git clone <this-repo> abdrust
cd abdrust
cp .env.example .env
```

`.env` に Discord の情報を入れてから起動します。

## 必須設定

- `DISCORD_TOKEN` - Bot token
- `DISCORD_CLIENT_ID` - Application client id
- `DISCORD_CLIENT_SECRET` - OAuth client secret
- `DISCORD_REDIRECT_URI` - OAuth redirect URI
- `DISCORD_GUILD_ID` - 開発用 guild id
- `ACTIVITY_MODE=local` - ローカル起動モード

## 起動

```bash
make dev
```

個別に起動する場合:

```bash
cd backend && cargo run -p abdrust
cd frontend && npm run dev
```

## Activity 起動

Activity の表示は Discord の CSP の都合で `cloudflared` 前提です。

```bash
make tunnel
```

表示された `https://*.trycloudflare.com` を Discord Developer Portal の URL Mapping `/` に設定してください。
URL Override は使わず、Discord の Activity Shelf から起動します。

## 確認ポイント

- backend: `GET /api/health`
- bot: Discord Gateway に接続していること
- bot: `/abdrust-debug` が応答すること
- voice: `/voice join` が成功すること
- activity: `initDiscord()` / `POST /api/token` / `ws` / `bot: ready` が通ること

## 検証

- backend: `cargo test`
- frontend: `npm run build`
- 両方触った変更は、両方の検証を通す

## ブラウザ確認

- プロジェクト用の skill-style 手順は `docs/browser-tooling-playbook.md`
- Playwright: `cd frontend && npm run test:e2e`
- axe: `cd frontend && npm run test:a11y`
- Lighthouse CI: `cd frontend && npm run lighthouse`
- これらは `?tooling=1` のローカル確認モードで動き、Discord 認証・WS・private API を止めたまま確認します
- `?tooling=1` は `localhost` / `127.0.0.1` / `::1` などのローカルホストだけで有効です

## `.env` の扱い

- ルート直下の `.env` だけを使います
- backend は `../.env` を読む
- frontend は Vite 起動時にルート `.env` を読む

## 補足

- `make check` で backend / frontend の確認ができます
- `make cleanup-commands` で既存コマンドを整理できます
- 変更は、小さく読みやすく、将来の Discord 仕様変更に追従しやすい形を優先します

## 参考

- `docs/embedded-activity-landscape.md`
- `docs/security-and-multiguild.md`
- `docs/performance-notes.md`
- `docs/review-checklist.md`
