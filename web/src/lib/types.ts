export interface ReferenceSequence {
  name: string
  length: number
}

export interface AlignmentSummary {
  start: number
  end: number
  mapping_quality: number
  is_reverse: boolean
  cigar: string
  blocks: AlignedBlock[]
  deletions: ReferenceSpan[]
  insertions: Insertion[]
}

export interface AlignedBlock { start: number; end: number; bases: string }
export interface ReferenceSpan { start: number; end: number }
export interface Insertion { position: number; bases: string }

export interface AlignmentQueryResult {
  total_count: number
  alignments: AlignmentSummary[]
  truncated: boolean
}

export interface BrowserError {
  message: string
}
