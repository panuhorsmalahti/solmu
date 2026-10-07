import { test, expect } from '@playwright/test'

const products = [
  { slug: 'muxer', title: 'Muxer · Solmu', feature: 'Real terminals, side by side', image: 'Muxer GUI showing a Solmu workspace and Spaces sidebar', docs: '../docs/muxer/' },
  { slug: 'boxer', title: 'Boxer · Solmu', feature: 'Choose what files are available', image: 'Boxer GUI showing a live Boxer process list', docs: '../docs/boxer/' },
  { slug: 'congregator', title: 'Congregator · Solmu', feature: 'Expose application ports', image: 'Congregator dashboard showing an agent and exposed sandbox ports', docs: '../docs/congregator/' },
]

for (const product of products) {
  test(`${product.slug} product page describes current features and links to its guide`, async ({ page }) => {
    await page.goto(`http://127.0.0.1:4174/solmu/${product.slug}/`)
    await expect(page).toHaveTitle(product.title)
    await expect(page.getByRole('heading', { level: 1 })).toContainText(product.slug === 'muxer' ? 'Muxer' : product.slug[0].toUpperCase() + product.slug.slice(1))
    await expect(page.getByRole('heading', { name: product.feature })).toBeVisible()
    await expect(page.getByRole('link', { name: 'Read the guide' })).toHaveAttribute('href', product.docs)
    await expect(page.getByRole('link', { name: 'Back to Solmu home' })).toHaveAttribute('href', '../')
    if (product.image) {
      const image = page.getByRole('img', { name: product.image })
      await expect(image).toBeVisible()
      await expect.poll(() => image.evaluate((element: HTMLImageElement) => element.complete && element.naturalWidth > 0)).toBe(true)
    } else {
      await expect(page.getByLabel('Boxer sandbox command example')).toBeVisible()
    }
  })

  test(`${product.slug} product page fits a phone screen`, async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 })
    await page.goto(`http://127.0.0.1:4174/solmu/${product.slug}/`)
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true)
    await expect(page.getByRole('link', { name: 'Read the guide' })).toBeVisible()
  })
}
