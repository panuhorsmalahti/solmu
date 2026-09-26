import { test, expect } from '@playwright/test'

test('website describes Solmu features and links to every client and installation', async ({ page }) => {
  await page.goto('http://127.0.0.1:4174')
  await expect(page).toHaveTitle('Solmu — your ideas, connected')
  await expect(page.getByRole('heading', { level: 1 })).toContainText('Good ideas')
  for (const feature of ['See it take shape', 'Follow your threads', 'Bring your model', 'Make it yours']) await expect(page.getByRole('heading', { name: feature })).toBeVisible()
  for (const client of ['Terminal', 'Desktop', 'Web']) await expect(page.getByRole('link').filter({ has: page.getByRole('heading', { name: client }) })).toHaveAttribute('href', /github\.com\/panuhorsmalahti\/solmu\/tree\/main\/clients\//)
  await expect(page.getByRole('link', { name: 'Meet your Solmu' })).toHaveAttribute('href', 'https://github.com/panuhorsmalahti/solmu#install')
  await page.getByRole('link', { name: 'Find your space' }).click()
  await expect(page).toHaveURL(/#your-space$/)
  await expect(page.getByRole('heading', { name: 'Think where you feel at home.' })).toBeInViewport()
})

test('website is usable on a narrow screen and by keyboard', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 })
  await page.goto('http://127.0.0.1:4174')
  await page.keyboard.press('Tab')
  await expect(page.getByRole('link', { name: 'Skip to content' })).toBeFocused()
  await page.keyboard.press('Enter')
  await expect(page).toHaveURL(/#main$/)
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
  await expect(page.getByRole('link', { name: 'GitHub' })).toBeVisible()
})
