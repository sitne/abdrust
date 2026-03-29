const { chromium } = require('@playwright/test')

module.exports = {
  ci: {
    collect: {
      numberOfRuns: 1,
      url: ['http://127.0.0.1:4173/?tooling=1'],
      startServerCommand: 'npm run preview:ci',
      startServerReadyPattern: 'Local:',
      chromePath: process.env.CHROME_PATH || chromium.executablePath(),
      settings: {
        chromeFlags: '--no-sandbox --disable-setuid-sandbox',
      },
    },
    assert: {
      assertions: {
        'categories:performance': ['warn', { minScore: 0.55 }],
        'categories:accessibility': ['warn', { minScore: 0.9 }],
        'categories:best-practices': ['warn', { minScore: 0.8 }],
      },
    },
    upload: {
      target: 'filesystem',
      outputDir: './.lighthouseci',
    },
  },
}
