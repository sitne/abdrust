import { expect, test } from '@playwright/test'
import AxeBuilder from '@axe-core/playwright'

test('passes basic accessibility checks', async ({ page }) => {
  await page.goto('/?tooling=1')
  await expect(page.locator('main')).toBeVisible()

  const results = await new AxeBuilder({ page }).include('main').analyze()
  expect(results.violations).toEqual([])
})
