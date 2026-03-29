# Browser Tooling Skill

このリポジトリでは、実際の platform skill はまだ使えないため、
このドキュメントを repo-local の skill 代わりにします。

## 使う場面

- 画面を実際に開いて確認したい
- クリックや入力を自動化したい
- a11y や perf をローカルで見たい
- Discord 認証を使わずに Activity を確認したい

## 安全ルール

- `?tooling=1` は `localhost` / `127.0.0.1` / `::1` だけで有効
- Discord auth / WebSocket / private API は tooling mode で無効
- 公開 URL には原則使わない
- 秘密情報はテストや Lighthouse に渡さない

## コマンド

```bash
cd frontend
npm run test:e2e
npm run test:a11y
npm run lighthouse
npx playwright show-report
npx playwright test --headed
```

## 補足

- Playwright は `frontend/playwright.config.ts` を見る
- Lighthouse CI は `frontend/lighthouserc.cjs` を見る
- a11y は `@axe-core/playwright` で `main` を検査する

## 公式リファレンス

- Playwright: `https://playwright.dev/docs/intro`
- Lighthouse CI: `https://github.com/GoogleChrome/lighthouse-ci`
- axe-core: `https://github.com/dequelabs/axe-core-npm`
