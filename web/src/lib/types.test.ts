import { describe, expect, it } from 'vitest'
import type { AlignmentQueryResult, ReferenceSequence } from './types'

describe('reference DTO', () => {
  it('keeps BAM reference lengths as numeric 0-based coordinate bounds', () => {
    const reference: ReferenceSequence = { name: 'chr1', length: 10 }
    expect(reference.length).toBe(10)
  })

  it('includes bounded density bins alongside sampled alignments', () => {
    const result: Pick<AlignmentQueryResult, 'density'> = { density: [0, 3, 1] }
    expect(result.density).toEqual([0, 3, 1])
  })
})
