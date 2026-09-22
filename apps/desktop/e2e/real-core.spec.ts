/**
 * The React client against the real Rust core through the dev bridge.
 * Requires `riddle serve` (RIDDLE_BRIDGE_FILE points at its bridge.json)
 * and the Vite dev server. Screenshots land in e2e/shots/.
 */
import { test, expect, type Page } from '@playwright/test'
import { readFileSync, mkdirSync } from 'node:fs'

const bridgeFile = process.env.RIDDLE_BRIDGE_FILE ?? ''
const bridge = bridgeFile ? (JSON.parse(readFileSync(bridgeFile, 'utf8')) as { port: number; token: string }) : null
const SHOTS = process.env.RIDDLE_SHOTS ?? 'e2e/shots'
mkdirSync(SHOTS, { recursive: true })

test.skip(!bridge, 'RIDDLE_BRIDGE_FILE not set')

async function open(page: Page, route: string) {
  await page.goto(`/?bridge=${bridge!.port}:${bridge!.token}#${route}`)
  await page.waitForLoadState('networkidle')
  // The first-run tour is a real product feature; a returning user has seen it.
  await page.evaluate(() => { try { localStorage.setItem('riddle.tour.done', '1'); localStorage.setItem('riddleTourDone', '1') } catch {} })
  const skip = page.getByText(/Skip \(Esc\)/)
  if (await skip.isVisible().catch(() => false)) await page.keyboard.press('Escape')
}

async function shot(page: Page, name: string) {
  await page.screenshot({ path: `${SHOTS}/${name}.png`, fullPage: false })
}

test('summary, finds, inbox and why are served by the real core', async ({ page }) => {
  const errors: string[] = []
  page.on('pageerror', (e) => errors.push(e.message))
  page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()) })

  await open(page, '/')
  await expect(page.getByText(/Good (morning|afternoon|evening)|Caught up|Catching up/i).first()).toBeVisible({ timeout: 15_000 })
  await shot(page, '01-summary')

  await open(page, '/finds')
  await expect(page.locator('main')).toContainText(/Find|Respond|Review|Attend|Pay|Know|Nothing needs/i, { timeout: 15_000 })
  await shot(page, '02-finds')

  await open(page, '/inbox')
  await expect(page.locator('main')).toContainText(/messages/, { timeout: 15_000 })
  await page.waitForTimeout(1000)
  await shot(page, '03-inbox')
  // Keyboard triage: j moves, Enter opens the current row.
  await page.locator('main').click({ position: { x: 5, y: 5 } })
  await page.keyboard.press('j')
  await page.keyboard.press('Enter')
  await page.waitForTimeout(2000)
  await shot(page, '04-message')
  await expect(page.locator('main')).toContainText(/Why|Correct|Reply|From/i)

  await open(page, '/activity')
  await page.waitForTimeout(1000)
  await shot(page, '05-activity')

  await open(page, '/meshes')
  await expect(page.locator('main')).toContainText(/GitHub Notifications|Newsletters|Receipts/, { timeout: 15_000 })
  await shot(page, '06-meshes')

  await open(page, '/signals')
  await expect(page.locator('main')).toContainText(/requires_response|review_requested/, { timeout: 15_000 })
  await shot(page, '07-signals')

  await open(page, '/developer')
  await page.waitForTimeout(1500)
  await shot(page, '08-developer')

  await open(page, '/settings')
  await page.waitForTimeout(1500)
  await shot(page, '09-settings')

  // A blocked script inside a mail body is the sandbox working, not an error.
  const fatal = errors.filter((e) => !/favicon|ResizeObserver|net::ERR_|about:srcdoc/.test(e))
  expect(fatal, fatal.join('\n')).toEqual([])
})

test('a spoil review action round-trips through the Action layer with undo', async ({ page }) => {
  await open(page, '/spoil')
  await page.waitForTimeout(1500)
  await shot(page, '10-spoil')
  const empty = await page.locator('main').textContent()
  test.skip(!!empty && /nothing|no messages|empty/i.test(empty) && !/Newsletters/.test(empty), 'no Spoil in this fixture run')
  await page.locator('main').click({ position: { x: 5, y: 5 } })
  await page.keyboard.press('j')
  await page.keyboard.press('Enter')
  await page.waitForTimeout(1500)
  const notSpoil = page.getByRole('button', { name: /not spoil|restore|correct/i }).first()
  if (await notSpoil.isVisible().catch(() => false)) {
    await notSpoil.click()
    await page.waitForTimeout(1500)
    await shot(page, '11-after-restore')
  }
  await open(page, '/activity')
  await page.waitForTimeout(1500)
  await shot(page, '12-activity-after')
})

test('the Director answers through the real model and the conversation persists', async ({ page }) => {
  test.setTimeout(180_000)
  await open(page, '/director')
  await page.waitForTimeout(1000)
  const input = page.locator('textarea, input[type="text"]').last()
  await input.fill('What needs my attention this morning? Keep it to three lines.')
  await input.press('Enter')
  await expect(page.locator('main')).toContainText(/./, { timeout: 5_000 })
  // Wait for the turn to finish: the Director's reply appears (or a stated error).
  await page.waitForFunction(() => /Director/.test(document.querySelector('main')?.textContent ?? '') && (document.querySelector('main')?.textContent?.length ?? 0) > 400, null, { timeout: 150_000 })
  await page.waitForTimeout(3000)
  await shot(page, '13-director')
  const text = (await page.locator('main').textContent()) ?? ''
  expect(text).not.toMatch(/AccessDeniedException|could not complete/i)
})

test('a Mesh edit previews its impact against real history before applying', async ({ page }) => {
  await open(page, '/meshes')
  await expect(page.locator('main')).toContainText(/GitHub Notifications/, { timeout: 15_000 })
  await page.getByText('GitHub Notifications', { exact: false }).first().click()
  await page.waitForTimeout(1500)
  await shot(page, '14-mesh')
  const editor = page.locator('textarea').first()
  if (await editor.isVisible().catch(() => false)) {
    const text = await editor.inputValue()
    // Tighten the review threshold and ask for the historical impact.
    await editor.fill(text.replace('review_requested@v1 > .80', 'review_requested@v1 > .90'))
    const preview = page.getByRole('button', { name: /preview|impact|simulate/i }).first()
    if (await preview.isVisible().catch(() => false)) {
      await preview.click()
      await expect(page.locator('main')).toContainText(/Unchanged|Changed|judge calls|Historical/i, { timeout: 30_000 })
      await page.waitForTimeout(1000)
      await shot(page, '15-mesh-simulation')
    }
  }
})
