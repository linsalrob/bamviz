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
}

export interface BrowserError {
  message: string
}
