import { expect, test } from '@playwright/test'

test('renders the tooling shell', async ({ page }) => {
  await page.goto('/?tooling=1')

  await expect(page.getByRole('heading', { name: 'Voice diagnostics' })).toBeVisible()
  await expect(page.getByText('browser tooling mode')).toBeVisible()
  await expect(page.locator('main')).toBeVisible()
})
