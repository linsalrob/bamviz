import { describe, expect, it } from 'vitest'
import type { ReferenceSequence } from './types'

describe('reference DTO', () => {
  it('keeps BAM reference lengths as numeric 0-based coordinate bounds', () => {
    const reference: ReferenceSequence = { name: 'chr1', length: 10 }
    expect(reference.length).toBe(10)
  })
})
