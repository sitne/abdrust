# ADR-001: Voice Gateway WebSocketライブラリの選定

## 状態
✅ 確定

## 背景
Discord音声ゲートウェイ（`wss://{endpoint}?v=8`）へのWebSocket接続に使用するRustライブラリを選定する必要があった。

## 検討した選択肢

### 1. tokio-tungstenite
- **Pros**: 最も人気のあるtokio用WebSocketライブラリ
- **Cons**: rustlsのJA3 TLSフィンガープリントがCloudflareのボット検知に引っかかり、400 Bad Requestを返される
- **結果**: ❌ 不採用

### 2. native-tls（tokio-tungstenite + native-tls）
- **Pros**: システムのOpenSSLを使用
- **Cons**: 依然として400エラー
- **結果**: ❌ 不採用

### 3. tokio-websockets
- **Pros**: songbirdが `tws` featureで使用している、より標準的なTLSハンドシェイク、Cloudflare 400を回避
- **Cons**: tokio-tungsteniteより新しい、エコシステムが小さい
- **結果**: ✅ 採用

## 決定
`tokio-websockets` 0.11 を使用する。

## 結果
Cloudflare 400エラーを回避し、音声ゲートウェイへの接続に成功。

## 日付
2026-04-03
