# abdrust

一个用于在单个 Rust 项目中同时运行 Discord Bot 和嵌入式 Activity 的基础框架。面向长期维护——Activity UI 和 Bot 语音层在同一代码库中共同演进，使用同一个 Token、同一个仓库。

## 功能

- 在同一项目中开发 Bot 和 Activity
- `/voice join` / `/voice leave` / `/voice status` 斜杠命令
- Activity 端显示 Bot 状态和实时语音事件
- 作为 Discord 嵌入式应用运行

## 核心特性

- Bot、后端和 Activity 集成在一个 Rust 仓库中
- 语音接收、解密和诊断直接传输到 Activity UI
- `/voice-diag` 一站式查看 DAVE / 加入 / 接收状态
- 抽象化的语音层——无需改动其余部分即可替换实现

## 快速开始

```bash
git clone <this-repo> abdrust
cd abdrust
cp .env.example .env
```

编辑 `.env` 填入 Discord 凭据，然后启动：

```bash
make dev
```

或分别启动：

```bash
cd backend && cargo run -p abdrust
cd frontend && npm run dev
```

## 必需配置

| 变量 | 说明 |
|---|---|
| `DISCORD_TOKEN` | Bot Token |
| `DISCORD_CLIENT_ID` | 应用客户端 ID |
| `DISCORD_CLIENT_SECRET` | OAuth 客户端密钥 |
| `DISCORD_REDIRECT_URI` | OAuth 重定向 URI |
| `DISCORD_GUILD_ID` | 开发服务器 ID |
| `ACTIVITY_MODE` | `local`（本地开发）或 `discord`（生产环境） |

## 运行 Activity

由于 Discord 的 CSP 限制，本地测试需要 `cloudflared` 隧道：

```bash
make tunnel
```

将显示的 `https://*.trycloudflare.com` URL 设置到 Discord Developer Portal 的 URL Mapping `/` 中。请从 Discord Activity Shelf 启动，不要使用 URL Override。

## 验证清单

- 后端: `GET /api/health` 正常响应
- Bot: 已连接到 Discord Gateway
- Bot: `/abdrust-debug` 正常响应
- 语音: `/voice join` 成功
- Activity: `initDiscord()` → `POST /api/token` → `ws` → `bot: ready` 全部通过

## 运行测试

```bash
# 后端
cargo test

# 前端
npm run build

# 两者
make check
```

## 浏览器工具

完整工作流参见 `docs/browser-tooling-playbook.md`。

```bash
cd frontend && npm run test:e2e     # Playwright
cd frontend && npm run test:a11y    # axe 无障碍测试
cd frontend && npm run lighthouse   # Lighthouse CI
```

这些在 `?tooling=1` 本地模式下运行——Discord 认证、WS 和私有 API 已禁用，仅 localhost 可用。

## `.env` 处理

- 仅使用根目录的 `.env`
- 后端读取 `../.env`
- 前端通过 Vite 在构建时读取根目录 `.env`

## 其他命令

```bash
make check            # cargo check + npm run build
make cleanup-commands # 清理已注册的斜杠命令
```

## 项目结构

```
abdrust/
├── backend/app/src/    # Rust 后端（Bot、语音引擎、HTTP 服务器）
├── frontend/src/       # React + TypeScript Activity UI
├── docs/               # 架构、ADR、开发日志
├── scripts/            # 工具脚本
├── AGENTS.md           # AI 代理指令
└── .env                # 配置的唯一来源
```

## 设计原则

- 优先选择克隆后易于理解的变更
- Bot 和 Activity 代码在跨越边界时保持一致性
- 优化长期可维护性，而非仅仅通过构建
- 假设 Discord API 会变化——保持架构灵活

## 许可证

- 仓库代码和原创文档: MIT，见 `LICENSE`
- `protocol.md`: 独立的 CC BY-NC-SA 4.0 许可证，见 `THIRD_PARTY_NOTICES.md`

## 语言

- [English](README.md)
- [日本語](README.ja.md)
- [中文](README.zh.md)
- [Español](README.es.md)
