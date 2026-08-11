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

test('opens the local BAM loader', async ({ page }) => {
  await page.goto('/')
  await expect(page.getByRole('heading', { name: 'bamviz' })).toBeVisible()
  await expect(page.getByRole('button', { name: 'Choose BAM' })).toBeVisible()
})

test('loads a BAM and changes the selected contig', async ({ page }) => {
  await page.goto('/')
  await page
    .getByRole('region', { name: 'BAM file loader' })
    .locator('input[type=file]')
    .setInputFiles({ name: 'synthetic.bam', mimeType: 'application/octet-stream', buffer: syntheticBam() })
  await expect(page.getByText('2 references')).toBeVisible()
  await expect(page.getByText('1 mapped alignment found.')).toBeVisible()
  await page.getByRole('button', { name: /r 11–15/ }).click()
  await expect(page.getByRole('region', { name: 'Selected read details' })).toContainText('Mapping quality')
  await expect(page.getByRole('region', { name: 'Selected read details' })).toContainText('60')
  await page.getByLabel('Reference / contig').selectOption('chr2')
  await expect(page.getByText('5–9')).toBeVisible()
  await expect(page.getByText('30')).toBeVisible()
})
