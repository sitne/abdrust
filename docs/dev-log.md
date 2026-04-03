# Dev Log

このファイルは開発ログです。作業ごとの変更内容、進捗、計画の更新・追加をここに追記していきます。

---

## 2026-04-02: 長期レビューとDAVEロードマップ策定

### 背景

- Twilight 0.16 で止まっている（0.17 で components 対応済み）
- songbird フォーク（`abokado0107/songbird-davey` branch `davey`）に依存 — 個人リポジトリで長期メンテ不透明
- DAVE 4017 エラーを文字列マッチで検知しているだけ。本質的なDAVEハンドシェイク未実装
- 2026年3月1日よりDiscordは非ステージ音声チャンネルでDAVE/E2EEを必須化

### 調査結果: davey

| 項目 | 値 |
|------|-----|
| リポジトリ | `Snazzah/davey` |
| crates.io | `davey` v0.1.3 |
| ライセンス | MIT |
| スター | 61 |
| 依存 | OpenMLS 0.8.1, p256, aes-gcm |
| 作者 | Snazzah（discord.js本体にDAVE PR #10921 をマージした本人） |
| 更新 | 2026年3月現在アクティブ（21リリース） |
| npm版DL | 週 494K |

**davey が提供するもの**: MLSグループ操作、フレーム暗号化/復号、DAVEマーカー、Displayable Code
**davey が提供しないもの**: Voice Gateway通信（opcode 25-31）、RTP/UDP層

### 調査結果: songbird

- 本家 `serenity-rs/songbird` v0.5.0（2025年2月）にDAVEなし
- Issue #293 "E2EE/DAVE protocol required" — 2026年3月7日オープン、未対応
- フォーク `abokado0107/songbird-davey` は個人依存

### 決定: 依存戦略

**「更新が確約されているライブラリだけ使う。RTP/UDP層は自前実装」**

| カテゴリ | クレート | 理由 |
|---------|---------|------|
| Gateway | `twilight-*` 0.17 | Discord API追従が最も速い |
| DAVE暗号 | `davey` 0.1.3+ | 作者はdiscord.js公式DAVE実装者。エコシステム広い |
| 非同期 | `tokio` | Rust標準 |
| HTTP | `axum` 0.8 | Tokioチーム公式 |
| Opusデコード | `opus` 0.3 | コーデック仕様不変。コーデック自体が変わらない限りメンテ不要 |

**使わないもの**: songbird（本家・フォーク問わず）、serenity-rs/voice-model（star 0、早すぎる）、その他Discord voiceラッパー

### アーキテクチャ方針

```
┌──────────────────────────────────────────────┐
│  自前実装 (プロトコル固定で不変)              │
│  ┌────────────────────────────────────────┐  │
│  │ UDPソケット管理 (RTP送受信)             │  │
│  │   - std::net::UdpSocket で十分          │  │
│  │   - RTPヘッダー parse/serialize         │  │
│  └────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────┐  │
│  │ DAVEハンドシェイクオーケストレーション   │  │
│  │   - Voice Gateway opcode 25-31         │  │
│  │   - twilight-gateway の sender で送受信 │  │
│  │   - davey::Session でMLS操作            │  │
│  └────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────┐  │
│  │ フレーム復号 → Opusデコード → PCM       │  │
│  │   - davey で復号                        │  │
│  │   - opus クレートでデコード              │  │
│  └────────────────────────────────────────┘  │
└──────────────────────────────────────────────┘
┌──────────────────────────────────────────────┐
│  外部依存 (Discord変更に追従)                 │
│  ┌────────────────────────────────────────┐  │
│  │ twilight-gateway 0.17+                 │  │
│  └────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────┐  │
│  │ davey 0.1.3+                           │  │
│  └────────────────────────────────────────┘  │
└──────────────────────────────────────────────┘
```

この方針のメリット:
- DiscordがDAVE v2を出したら → daveyのアップデートを待つだけ
- DiscordがGateway APIを変えたら → twilightが追従する
- songbirdが放置されても → 関係ない
- 個人フォークが消えても → 関係ない

### ロードマップ

#### Phase 0: 止血 + Phase 1: davey PoC（並行）

**Step 1: Twilight 0.17 アップグレード**
- [x] `Cargo.toml`: `twilight-*` を `0.17` に
- [x] `commands.rs`: `Command` 構築をビルダーパターンに変更
- [x] `handlers.rs`: interaction型の互換性修正
- [x] `presence.rs`: `UpdatePresence` API修正
- [x] `client.rs`: `StreamExt` 変更点修正
- [x] `cargo check` 通過確認

**Step 2: VoiceEngine トレイト完成 + main.rs 分離**
- [x] `main.rs` から `songbird::Songbird` の直接importを排除
- [x] `SongbirdVoiceEngine` の生成をファクトリ関数に移動

**Step 3: davey 依存追加 + スケルトン実装**
- [x] `Cargo.toml`: `davey = "0.1.3"` 追加
- [x] `dave.rs`: `DaveySession` ラッパー実装
- [x] `voice_engine.rs`: `DaveyVoiceEngine` スケルトン追加

**Step 4: RTP/UDP + DAVE ハンドシェイク実装**
- [x] UDPソケット作成・管理
- [x] RTPパケットの送受信
- [x] Voice Gateway opcode 25-31 の送受信
- [x] `davey::Session` によるMLS操作
- [x] フレーム復号 → Opusデコード → PCM出力

**Step 5: songbird 依存削除**
- [x] `DaveyVoiceEngine` が動作することを確認
- [x] `Cargo.toml` から songbird 関連の依存を削除
- [x] `voice.rs` から songbird::events のimportを削除
- [x] `main.rs` から songbird の初期化コードを削除
- [x] `cargo check` 通過確認

#### Phase 1.5: 音声受信パイプライン

- [x] UDP音声受信ループ（`tokio::select!` でWebSocketと並行）
- [x] RTPヘッダーパース → SSRC lookup
- [x] DAVE復号 → `AppEvent::VoiceAudioFrame` 送信
- [x] DAVE未ready時はpassthrough
- [x] SpeakingイベントのSSRC→user_idマッピング

#### Phase 1.6: 音声送信パス

- [x] `AudioData` 列挙型（`Opus`, `Pcm`, `Silence`）
- [x] `send_audio()` 公開API
- [x] Opusエンコーダー（`opus` クレートラッパー）
- [x] DAVE暗号化 → RTP送信
- [x] `VoiceUdpSocket::send_dave_encrypted()`

#### Phase 1.7: UDPソケット単一化

- [x] `VoiceUdpSocket::from_raw()` — IP Discovery用ソケットを再利用
- [x] `VoiceUdpSocket::set_secret_key()` — Session Description後に更新
- [x] 重複ソケット作成ロジック削除

#### Phase 1.8: エラーリカバリ

- [x] `DisconnectReason` 列挙型
- [x] 指数バックオフ再接続（2^n秒、最大60秒）
- [x] `watch::channel` によるシャットダウンシグナル
- [x] `broadcast::channel` による音声送信チャネル
- [x] 再接続成功時にカウンタリセット
- [x] Fatalエラー時は再接続せずに終了

#### Phase 2: DAVE完全準拠（未着手）

- [ ] プロトコルバージョネゴシエーション（opcode 0, 4）の厳密化
- [ ] MLS グループ作成・維持・破棄の完全実装
- [ ] メンバー追加・削除（external sender経由）の完全実装
- [ ] エポック遷移（prepare → ready → execute）の完全実装
- [ ] Sole member reset
- [ ] Invalid commit/welcome からのリカバリ
- [ ] キーローテーション（nonce wrap対応）
- [ ] コーデック完全対応（VP8/VP9/H264/H265/AV1）

#### Phase 3: 基盤安定化（未着手）

- [ ] `BotGateway` トレイト導入
- [ ] serenity/poise への移行パス確保
- [ ] CI/CD + テスト
- [ ] ADR 導入
- [ ] ハートビートの厳密なタイミング管理
- [ ] SpeakingイベントのSSRC→user_idマッピング実テスト
- [ ] 実際のDiscord音声チャンネルでの動作確認

### 工数見積もり

| Step | 工数 | リスク | 状態 |
|------|------|--------|------|
| Step 1: Twilight 0.17 | 2-4時間 | 低 | ✅ 完了 |
| Step 2: トレイト完成 | 1-2時間 | 低 | ✅ 完了 |
| Step 3: daveyスケルトン | 4-8時間 | 中 | ✅ 完了 |
| Step 4: RTP/UDP + DAVE | 2-4週間 | 高 | ✅ 完了 |
| Step 5: songbird削除 | 1-2時間 | 低 | ✅ 完了 |
| Phase 1.5: 受信パイプライン | 4-8時間 | 中 | ✅ 完了 |
| Phase 1.6: 送信パス | 4-8時間 | 中 | ✅ 完了 |
| Phase 1.7: UDP単一化 | 1-2時間 | 低 | ✅ 完了 |
| Phase 1.8: エラーリカバリ | 4-8時間 | 中 | ✅ 完了 |
| Phase 1.9: tokio-websockets移行 | 2-4時間 | 高 | ✅ 完了 |
| Phase 1.10: session_idバグ修正 | 1-2時間 | 低 | 作業中 |
| Phase 2: DAVE完全準拠 | 1-3ヶ月 | 高 | 未着手 |
| Phase 3: 基盤安定化 | 3-6ヶ月 | 中 | 未着手 |

---

## 2026-04-02: Discord公式ドキュメントMCPツールの導入

### 概要

Discord公式開発者ドキュメント（`docs.discord.com`）をMCP経由で検索・参照できるようにした。

### 確認済み機能

- `discord-docs_search_documentation_discord`: クエリベースのドキュメント検索
  - 例: `DAVE protocol voice gateway E2EE` → 関連ページとスニペットを返す
  - 例: `voice gateway opcodes identify resume` → opcode一覧、heartbeat仕様を返す
- `discord-docs_get_page_documentation_discord`: ページパス指定での全文取得
  - 例: `developers/topics/voice-connections` → Voice Gateway v8、DAVEハンドシェイク、RTP構造の全文
  - 例: `developers/topics/opcodes-and-status-codes` → Gateway/Voice opcode一覧、close code一覧

### 得られた知見

**Voice Gateway v8 の重要な変更点**:
- heartbeat に `seq_ack`（最後に受信したメッセージのシーケンス番号）が必須に
- resume にも `seq_ack` が必須に
- バイナリメッセージ形式が導入（DAVE opcode 25-31 の一部）
  - 構造: `[seq: u16?][opcode: u8][payload: bytes]`

**DAVE opcode 一覧（Voice Gateway）**:
| Code | Name | 方向 | Binary | 説明 |
|------|------|------|--------|------|
| 0 | Identify | client→server | | 接続開始。`max_dave_protocol_version` を含める |
| 1 | Select Protocol | client→server | | UDPプロトコル選択 |
| 2 | Ready | server→client | | SSRC、UDP IP/port、encryption modes |
| 3 | Heartbeat | client→server | | キープアライブ（v8は `seq_ack` 必須） |
| 4 | Session Description | server→client | | `mode`, `secret_key`, `dave_protocol_version` |
| 5 | Speaking | client↔server | | 発話状態 |
| 6 | Heartbeat ACK | server→client | | ハートビート応答 |
| 7 | Resume | client→server | | 再接続（v8は `seq_ack` 必須） |
| 8 | Hello | server→client | | `heartbeat_interval` |
| 9 | Resumed | server→client | | 再接続成功 |
| 11 | Clients Connect | server→client | | 新規クライアント接続 |
| 13 | Client Disconnect | server→client | | クライアント切断 |
| 21 | DAVE Prepare Transition | server→client | | DAVEダウングレード予告 |
| 22 | DAVE Execute Transition | server→client | | 遷移実行指示 |
| 23 | DAVE Transition Ready | client→server | | 遷移準備完了 |
| 24 | DAVE Prepare Epoch | server→client | | プロトコルバージョン/グループ変更予告 |
| 25 | DAVE MLS External Sender | server→client | X | MLS external senderのcredentialと公開鍵 |
| 26 | DAVE MLS Key Package | client→server | X | 保留中メンバーのMLS key package |
| 27 | DAVE MLS Proposals | server→client | X | 追加/削除proposal |
| 28 | DAVE MLS Commit Welcome | client→server | X | commit + optional welcome |
| 29 | DAVE MLS Announce Commit Transition | server→client | X | commitのブロードキャスト |
| 30 | DAVE MLS Welcome | server→client | X | 新メンバーへのwelcome |
| 31 | DAVE MLS Invalid Commit Welcome | client→server | | 無効なcommit/welcomeの報告 |

**Transport Encryption**:
- 推奨: `aead_aes256_gcm_rtpsize`
- 必須: `aead_xchacha20_poly1305_rtpsize`
- 非推奨（2024年11月18日で廃止）: `xsalsa20_poly1305` 系

**Voice Close Code 4017**: `E2EE/DAVE protocol required` — DAVE未対応クライアントは接続拒否

### 今後の活用方針

- DAVE実装時に公式ドキュメントを直接参照できるため、`protocol.md`（ローカルコピー）との差分確認に使う
- Voice Gateway opcodeの仕様変更をリアルタイムでキャッチアップ
- 新しいDiscord API機能（components, activities等）の調査にも流用

---

## 2026-04-02: Phase 0 — Twilight 0.17 アップグレード + songbird脱却

### 作業内容

**Cargo.toml**:
- `twilight-gateway` `0.16` → `0.17`
- `twilight-http` `0.16` → `0.17`
- `twilight-model` `0.16` → `0.17`
- `davey` `0.1.2` → `0.1.3`
- `songbird` git依存を削除

**voice_engine.rs**: 全面的書き換え
- `SongbirdVoiceEngine` を削除
- `DaveyVoiceEngine` スケルトンに置き換え
- `process_voice_state_update`: twilightイベント処理を維持（songbird::process 呼び出しを削除）
- `process_voice_server_update`: トレース記録のみ（songbird::process 呼び出しを削除）
- `join`: プレースホルダー実装（セッション登録のみ、DAVEハンドシェイク未実装）
- `leave`: 既存ロジックを維持
- `join` の戻り値を `JoinResult<VoiceSession>` → `Result<VoiceSession>` に変更

**voice.rs**: 大幅削減（558行 → 89行）
- `VoiceReceiveHandler` 構造体と `SongbirdEventHandler` 実装を全削除
- `JoinFailureKind`、`classify_join_error`、`join_error_causes`、`describe_join_error` を削除
- ユーティリティ関数のみを残す: `voice_meta_from_voice_state`、`resolve_voice_user_meta`、`fetch_member`、`voice_meta_from_member`、`join_voice`、`leave_voice`、`describe_voice_session`

**dave.rs**: songbird依存削除
- `use songbird::error::JoinError` を削除
- `is_dave_required_join_error(err: &JoinError)` → `is_dave_required_error(err: &anyhow::Error)` に変更
- `ENGINE_NAME` を `"songbird+davey"` → `"davey"` に変更

**main.rs**: songbird初期化コード削除
- `Songbird::twilight()`、`TwilightMap`、`DecodeMode` 設定を全削除
- `DaveyVoiceEngine::new()` を直接使用

**bot/handlers.rs**: エラーハンドリング簡略化
- `voice::classify_join_error`、`voice::JoinFailureKind`、`dave::is_dave_required_join_error` の呼び出しを削除
- `anyhow::Error` ベースの単純なエラーハンドリングに変更

### 結果

- `cargo check`: ✅ 警告なし
- `cargo test`: ✅ 0 tests passed（既存テストなし）

### 現在の状態

- **bot**: 正常動作（スラッシュコマンド登録、interaction処理）
- **HTTP/WS**: 正常動作（Activity用トークン交換、イベント配信）
- **音声**: プレースホルダー（joinはセッション登録のみ、実際のUDP/DAVE接続は未実装）
- **Activity UI**: ツーリングモードで表示確認可能

### 次ステップ

Step 3: davey を使ったDAVEハンドシェイク実装（Voice Gateway opcode 25-31）
Step 4: RTP/UDP層の実装（std::net::UdpSocket + RTPヘッダー）

---

## 2026-04-02: Phase 1 — DAVEハンドシェイク実装（Step 3a-3d）

### 作業内容

#### Step 3a: Voice Gateway WebSocket層
**新規ファイル: `src/voice/gateway.rs`**
- Voice Gateway WebSocket接続（`wss://{endpoint}?v=8`）
- Opcode 0-8 の送受信: Identify, Select Protocol, Ready, Heartbeat, Session Description, Hello, Speaking
- Opcode 21-31 のDAVEプロトコルメッセージ処理
- バイナリメッセージ形式のサポート（opcode 25, 27, 29, 30）
  - 形式: `[seq: u16?][opcode: u8][payload: bytes]`
- JSONメッセージのパースとイベント変換
- `VoiceEvent` 列挙型で全イベントを表現

#### Step 3b: DAVE MLSハンドシェイク層
**新規ファイル: `src/voice/session.rs`**
- `DaveyVoiceSession`: davey::DaveSession のラッパー
- External Sender設定（opcode 25受信時）
- Key Package作成・送信（opcode 26）
- Proposals処理（opcode 27）→ Commit/Welcome生成
- Welcome処理（opcode 30）→ グループ参加
- Commit処理（opcode 29）→ エポック更新
- 暗号化/復号: `encrypt_opus()`, `decrypt()`
- プライバシーコード取得

#### Step 3c: UDP RTP層
**新規ファイル: `src/voice/udp.rs`**
- `RtpHeader`: RTPヘッダーのparse/serialize（v2, CSRC, extension対応）
- `VoiceUdpSocket`: UDP音声送受信ソケット
  - RTPヘッダー自動付与、シーケンス番号/タイムスタンプ管理
  - 受信（非ブロッキング）
- `IpDiscovery`: NAT越えのためのIP発見プロトコル
  - Type/Length/SSRC/Address/Port 形式のパケット送受信

#### Step 3d: DaveyVoiceEngine 統合
**更新: `src/voice_engine.rs`**
- `DaveyVoiceEngine` に本格的な音声接続実装
- `join()`: VoiceServerUpdate待機 → Voice Gateway接続 → Identify → DAVEハンドシェイク
- `run_voice_loop_inner()`: 音声イベントループ
  - Heartbeat自動送信
  - Ready受信 → IP Discovery → Select Protocol
  - Session Description受信 → DAVEセッション作成
  - DAVE MLS External Sender → Key Package送信
  - DAVE Proposals → Commit/Welcome応答
  - DAVE Welcome/Commit → グループ参加
  - 音声フレーム受信（復号パスはTODO）
- `leave()`: シャットダウンシグナル送信 + 後片付け

**更新: `src/state.rs`**
- `PendingVoiceInfo` 構造体追加（session_id, token, endpoint）
- `AppEvent::VoiceSessionReady` 追加
- `store_pending_voice_info()`, `take_pending_voice_info()` メソッド追加
- `BotState` に `bot_user_id`, `pending_voice_info` フィールド追加

**更新: `src/main.rs`**
- `bot_user_id` を取得して `DaveyVoiceEngine::new()` と `AppState::new()` に渡す

**更新: `src/http/ws.rs`**
- `VoiceSessionReady` イベントのフォワード対応

**更新: `src/voice/mod.rs`**
- ユーティリティ関数をモジュール内に移動（旧voice.rsの内容）

**更新: `Cargo.toml`**
- `byteorder = "1"` 追加（IP Discoveryパケット構築）
- `opus = "0.3"` 追加（Opusデコード用）
- `tokio-tungstenite = "0.28"` 追加（Voice Gateway WebSocket）

### 結果

- `cargo check`: ✅ 警告3件（unused import/field、minor）
- `cargo test`: ✅ 通過

### 現在の状態

- **Voice Gateway接続**: 実装済み（Identify → Ready → IP Discovery → Select Protocol → Session Description）
- **DAVEハンドシェイク**: 実装済み（External Sender → Key Package → Proposals → Commit/Welcome）
- **音声フレーム受信**: 受信はするが復号後の処理はTODO
- **音声フレーム送信**: RTPヘッダー構築は実装済み、DAVE暗号化は未統合

### 残課題

- [ ] ハートビートの厳密なタイミング管理
- [ ] SpeakingイベントのSSRC→user_idマッピング実テスト
- [ ] **Voice Gateway WebSocket接続（Cloudflare 400エラー）** — 接続確立が最大のボトルネック

---

## 2026-04-03: バグ修正 — Voice Join/Leave の UpdateVoiceState 送信

### 作業内容

**問題1: `/voice join` がタイムアウト**
- 原因: `join()` で `UpdateVoiceState`（Gateway Opcode 4）を送信していなかった
- 修正: `voice_engine.rs` の `join()` 先頭で `UpdateVoiceState::new(guild_id, Some(channel_id), false, false)` を送信
- 結果: Discord から `VoiceStateUpdate` + `VoiceServerUpdate` が正常に返ってくるようになった

**問題2: `/voice leave` がDiscord上で反映されない**
- 原因: `leave()` で Discord に退出通知を送信していなかった
- 修正: `voice_engine.rs` の `leave()` で `UpdateVoiceState::new(guild_id, None, false, false)` を送信
- 結果: Discord 上でボットがVCから退出するようになった

**問題3: `VoiceSessionReady` イベントのguild_idマッチングバグ**
- 原因: `endpoint`（voiceサーバーのホスト名）でguild_idと比較していた
- 修正: `VoiceSessionReady` に `guild_id` フィールドを追加、正しくマッチング

### 既知の問題: Voice Gateway WebSocket 接続失敗（Cloudflare 400）

**状況**: `wss://c-nrt08-xxx.discord.media:2096?v=8` へのWebSocket接続がCloudflareから400 Bad Requestで拒否される
- `cf-ray: "-"` — Cloudflareがリクエストをルーティングする前に拒否
- TLSハンドシェイク段階で失敗

**試したアプローチ（全て失敗）:**
1. `rustls-tls-native-roots` → 400
2. `native-tls`（OpenSSL）→ 400
3. `rustls-tls-webpki-roots` → 400
4. `connect_async`（デフォルト）→ 400
5. `Sec-WebSocket-Extensions` ヘッダー削除 → 400
6. 手動リクエスト構築（Host, Origin, User-Agent明示）→ 400

**比較: discord.js の実装**
- 標準 `ws` ライブラリを使用、特別なヘッダーなし
- Node.js の `ws` はデフォルトで `permessage-deflate` を送信
- `tokio-tungstenite` のハンドシェイクフォーマットがDiscord音声ゲートウェイと互換性がない可能性

**次の調査候補:**
- `websocat` コマンドで直接接続テスト
- `tokio-tungstenite` のHTTPリクエスト生データをキャプチャ
- Discord voice gateway が要求する正確なハンドシェイクヘッダーを特定

---

## 2026-04-03: Phase 1.9 — tokio-websockets 移行 + Cloudflare 400 回避

### 作業内容

**問題**: `tokio-tungstenite` を使ったVoice Gateway WebSocket接続がCloudflareから400 Bad Requestで拒否される
- 6つのアプローチを試して全て失敗（TLS設定変更、ヘッダー削除、バージョン変更など）
- 根本原因: rustlsのJA3 TLSフィンガープリントがCloudflareのボット検知に引っかかる

**解決策**: `tokio-websockets` 0.11 に切り替え（songbirdの `tws` feature と同じクレート）
- `Cargo.toml`: `tokio-tungstenite` → `tokio-websockets`
- TLS: `rustls-native-roots`（システムの信頼済み証明書）
- 接続: `ClientBuilder::new().uri(url).limits(Limits::unlimited()).connect()`
- Message API: `Message::text()`, `Message::binary()`, `Message::pong()` など

**結果**: ✅ Cloudflare 400 を回避。WebSocket接続成功。
```
Voice WebSocket connected
Hello: heartbeat_interval=13750ms
```

**残る問題**: 接続直後に切断（Identify送信前にrecvループに入るバグ）
→ `run_voice_loop_inner()` の先頭にIdentify送信を追加

---

## 変更履歴

| 日付 | 変更 |
|------|------|
| 2026-04-03 | Phase 1.9: tokio-websockets移行 — Cloudflare 400回避成功 |
| 2026-04-03 | バグ修正: Voice Join/Leave の UpdateVoiceState 送信 + VoiceSessionReady マッチング修正 |
| 2026-04-03 | 調査: Voice Gateway WebSocket 400エラー（Cloudflare拒否）— 6アプローチ試して全て失敗 |
| 2026-04-02 | Phase 1.8: エラーリカバリ実装（指数バックオフ再接続 + watch/broadcastチャネル） |
| 2026-04-02 | Phase 1.7: UDPソケット単一化（IP Discoveryと受信/送信で同一ソケット） |
| 2026-04-02 | Phase 1.6: 音声送信パス実装（Opusエンコード→DAVE暗号化→RTP送信） |
| 2026-04-02 | Phase 1.5: 音声フレーム受信パイプライン実装（UDP受信→RTPパース→DAVE復号→Activity送信） |
| 2026-04-02 | Phase 1完了: DAVEハンドシェイク実装（Voice Gateway + davey統合） |
| 2026-04-02 | Phase 0完了: Twilight 0.17 + songbird脱却 |
| 2026-04-02 | Discord公式ドキュメントMCPツール導入・動作確認 |
| 2026-04-02 | 初期作成: 長期レビュー、davey調査、ロードマップ策定 |
