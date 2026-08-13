import { expect, test } from '@playwright/test'
import { gzipSync } from 'node:zlib'

function syntheticBam(): Buffer {
  const bytes: number[] = [...Buffer.from('BAM\x01')]
  const i32 = (value: number) => bytes.push(...Buffer.from(Uint8Array.of(value & 255, (value >>> 8) & 255, (value >>> 16) & 255, (value >>> 24) & 255)))
  const u16 = (value: number) => bytes.push(value & 255, (value >>> 8) & 255)
  i32(0)
  i32(2)
  for (const [name, length] of [['chr1', 100], ['chr2', 50]] as const) {
    i32(name.length + 1)
    bytes.push(...Buffer.from(name), 0)
    i32(length)
  }
  const record = (reference: number, start: number, mapq: number) => {
    const body: number[] = []
    const bodyI32 = (value: number) => body.push(...Buffer.from(Uint8Array.of(value & 255, (value >>> 8) & 255, (value >>> 16) & 255, (value >>> 24) & 255)))
    bodyI32(reference); bodyI32(start); body.push(2, mapq); body.push(0, 0, 1, 0, 0, 0); bodyI32(5); bodyI32(-1); bodyI32(-1); bodyI32(0)
    body.push(...Buffer.from('r\0')); bodyI32(5 << 4); body.push(0x12, 0x48, 0x10, 255, 255, 255, 255, 255)
    i32(body.length); bytes.push(...body)
  }
  record(0, 10, 60)
  record(1, 4, 30)
  return gzipSync(Buffer.from(bytes))
}

function syntheticBai(): Buffer {
  const bytes: number[] = [...Buffer.from('BAI\x01')]
  const u32 = (value: number) => bytes.push(...Buffer.from(Uint8Array.of(value & 255, (value >>> 8) & 255, (value >>> 16) & 255, (value >>> 24) & 255)))
  u32(2)
  u32(0); u32(0)
  u32(0); u32(0)
  return Buffer.from(bytes)
}

test('opens the local BAM loader', async ({ page }) => {
  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'bamviz' })).toBeVisible()
  await expect(page.getByRole('button', { name: 'Choose BAM' })).toBeVisible()
})

test('explains required browser APIs when WebAssembly is unavailable', async ({ browser }) => {
  const context = await browser.newContext()
  await context.addInitScript(() => Object.defineProperty(window, 'WebAssembly', { value: undefined, configurable: true }))
  const page = await context.newPage()
  await page.goto('/')
  await expect(page.getByRole('alert')).toContainText('requires WebAssembly, the File API, and Canvas 2D')
  await expect(page.getByRole('button', { name: 'Choose BAM' })).toBeDisabled()
  await context.close()
})

test('loads a BAM and changes the selected contig', async ({ page }) => {
  await page.goto('/')
  await page
    .getByRole('region', { name: 'BAM file loader' })
    .locator('input[accept^=".bam"]')
    .setInputFiles({ name: 'synthetic.bam', mimeType: 'application/octet-stream', buffer: syntheticBam() })
  await expect(page.getByText('synthetic.bam', { exact: true })).toBeVisible()
  await expect(page.getByText('2 references')).toBeVisible()
  await page.getByRole('region', { name: 'BAM file loader' }).locator('input[accept^=".bai"]').setInputFiles({ name: 'synthetic.bam.bai', mimeType: 'application/octet-stream', buffer: syntheticBai() })
  await expect(page.getByText('synthetic.bam.bai', { exact: true })).toBeVisible()
  await expect(page.getByText('BAI loaded: 2 reference indexes available for this BAM')).toBeVisible()
  await expect(page.getByText('1 mapped alignment found.')).toBeVisible()
  await expect(page.getByText('Drag the two-line grip on the panel’s right edge to resize the complete viewer. Arrow keys resize it when the grip is focused.')).toBeVisible()
  const resizeGrip = page.getByRole('button', { name: 'Resize alignment panel' })
  await expect(resizeGrip).toHaveCSS('cursor', 'ew-resize')
  const panelWidth = await page.getByRole('region', { name: 'BAM references' }).evaluate((panel) => panel.getBoundingClientRect().width)
  await resizeGrip.press('ArrowLeft')
  await expect.poll(() => page.getByRole('region', { name: 'BAM references' }).evaluate((panel) => panel.getBoundingClientRect().width)).toBeLessThan(panelWidth)
  await page.setViewportSize({ width: 320, height: 720 })
  expect(await page.getByRole('region', { name: 'BAM references' }).evaluate((panel) => panel.getBoundingClientRect().right <= panel.parentElement!.getBoundingClientRect().right)).toBe(true)
  await page.getByLabel('Alignment viewport').focus()
  await page.keyboard.press('+')
  await expect(page.getByLabel('Viewport coordinates')).toContainText('21–80 (1-based)')
  await expect(page.getByLabel('Alignment viewport')).toBeFocused()
  await page.keyboard.press('ArrowRight')
  await expect(page.getByLabel('Viewport coordinates')).toContainText('33–92 (1-based)')
  await page.keyboard.press('Home')
  await expect(page.getByLabel('Viewport coordinates')).toContainText('1–100 (1-based)')
  await page.getByRole('button', { name: /r 11–15/ }).click()
  await expect(page.getByRole('region', { name: 'Selected read details' })).toContainText('Mapping quality')
  await expect(page.getByRole('region', { name: 'Selected read details' })).toContainText('60')
  await page.getByLabel('Reference / contig').selectOption('chr2')
  await expect(page.getByText('5–9')).toBeVisible()
  await expect(page.getByText('30')).toBeVisible()
  const referenceFiles = page.getByRole('region', { name: 'Optional reference files' })
  await referenceFiles.locator('input[accept=".fa,.fasta,.fna,text/plain"]').setInputFiles({ name: 'reference.fasta', mimeType: 'text/plain', buffer: Buffer.from('>chr2\nACGTA\n') })
  await expect(page.getByText('reference.fasta', { exact: true })).toBeVisible()
  await referenceFiles.locator('input[accept^=".fai"]').setInputFiles({ name: 'reference.fasta.fai', mimeType: 'text/plain', buffer: Buffer.from('chr2\t5\t6\t5\t6\n') })
  await expect(page.getByText('reference.fasta.fai', { exact: true })).toBeVisible()
})
