# abdrust

Discord Bot と Embedded Activity を、1つのRustプロジェクトとして長期運用するための土台です。Activity の映像UIと、Bot の音声処理やDiscord連携を、同じトークン・同じリポジトリで育てていくことを目的にしています。

## できること

- Bot と Activity を同一プロジェクトで開発できる
- `/voice join` / `/voice leave` / `/voice status` スラッシュコマンド
- Activity 側で bot の状態や音声イベントを表示する
- Discord の Embedded App として起動する

## このテンプレの見どころ

- bot / backend / Activity を1つのRust repoでまとめて扱える
- voice の受信・復号・診断を Activity にそのまま流せる
- `/voice-diag` で DAVE / join / receive の状態を一括確認できる
- 音声層を抽象化しているので、将来の実装差し替えに追従しやすい

## クイックスタート

```bash
git clone <this-repo> abdrust
cd abdrust
cp .env.example .env
```

`.env` に Discord の情報を入れてから起動します。

```bash
make dev
```

個別に起動する場合:

```bash
cd backend && cargo run -p abdrust
cd frontend && npm run dev
```

## 必須設定

| 変数 | 説明 |
|---|---|
| `DISCORD_TOKEN` | Bot トークン |
| `DISCORD_CLIENT_ID` | アプリケーションのクライアントID |
| `DISCORD_CLIENT_SECRET` | OAuth クライアントシークレット |
| `DISCORD_REDIRECT_URI` | OAuth リダイレクトURI |
| `DISCORD_GUILD_ID` | 開発用ギルドID |
| `ACTIVITY_MODE` | `local`（ローカル開発）または `discord`（本番） |

## Activity の起動

Discord の CSP の都合で、ローカルテストには `cloudflared` トンネルが必要です。

```bash
make tunnel
```

表示された `https://*.trycloudflare.com` を Discord Developer Portal の URL Mapping `/` に設定してください。URL Override は使わず、Discord の Activity Shelf から起動します。

## 確認ポイント

- backend: `GET /api/health` が応答する
- bot: Discord Gateway に接続している
- bot: `/abdrust-debug` が応答する
- voice: `/voice join` が成功する
- activity: `initDiscord()` → `POST /api/token` → `ws` → `bot: ready` が通る

## テスト

```bash
# バックエンド
cargo test

# フロントエンド
npm run build

# 両方
make check
```

## ブラウザツール

詳細は `docs/browser-tooling-playbook.md` を参照。

```bash
cd frontend && npm run test:e2e     # Playwright
cd frontend && npm run test:a11y    # axe アクセシビリティ
cd frontend && npm run lighthouse   # Lighthouse CI
```

これらは `?tooling=1` のローカル確認モードで動作します。Discord 認証・WS・プライベートAPIは無効化され、localhost でのみ有効です。

## `.env` の扱い

- ルート直下の `.env` だけを使います
- backend は `../.env` を読む
- frontend は Vite 起動時にルート `.env` を読む

## その他のコマンド

```bash
make check            # cargo check + npm run build
make cleanup-commands # 登録済みスラッシュコマンドの整理
```

## プロジェクト構成

```
abdrust/
├── backend/app/src/    # Rust バックエンド（bot、音声エンジン、HTTPサーバー）
├── frontend/src/       # React + TypeScript Activity UI
├── docs/               # アーキテクチャ、ADR、開発ログ
├── scripts/            # ユーティリティスクリプト
├── AGENTS.md           # AI エージェント用指示書
└── .env                # 設定の唯一の情報源
```

## 設計方針

- クローン直後でも理解しやすい変更を優先する
- ボットとActivityコードは境界を越える場合も整合性を保つ
- 将来の保守性を最適化する。ビルドを通すだけの最短経路は取らない
- Discord APIは変わると想定し、柔軟なアーキテクチャを保つ

## ライセンス

- リポジトリのコードとオリジナルのドキュメント: MIT, `LICENSE` を参照
- `protocol.md`: 別個の CC BY-NC-SA 4.0 ライセンス, `THIRD_PARTY_NOTICES.md` を参照

## 言語

- [English](README.md)
- [日本語](README.ja.md)
- [中文](README.zh.md)
- [Español](README.es.md)
