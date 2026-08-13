export interface ReferenceSequence {
  name: string
  length: number
}

export interface AlignmentSummary {
  read_name: string
  start: number
  end: number
  mapping_quality: number
  flags: AlignmentFlags
  cigar: string
  left_clip: number
  right_clip: number
  mate_reference: string | null
  mate_start: number | null
  blocks: AlignedBlock[]
  deletions: ReferenceSpan[]
  insertions: Insertion[]
}

export interface AlignmentFlags {
  raw: number
  is_reverse: boolean
  is_paired: boolean
  is_proper_pair: boolean
  mate_is_reverse: boolean
  is_secondary: boolean
  is_supplementary: boolean
  is_duplicate: boolean
}

export interface AlignmentFilter {
  min_mapping_quality: number
  include_secondary: boolean
  include_supplementary: boolean
  include_duplicates: boolean
}

export interface AlignedBlock { start: number; end: number; bases: string; known_matches: boolean[] }
export interface ReferenceSpan { start: number; end: number }
export interface Insertion { position: number; bases: string }

export interface AlignmentQueryResult {
  total_count: number
  alignments: AlignmentSummary[]
  truncated: boolean
  density: number[]
}

export interface BrowserError {
  message: string
}

export interface BaiReferenceSummary {
  bin_count: number
  chunk_count: number
  linear_interval_count: number
  mapped_count: number | null
  unmapped_count: number | null
}

export interface BaiIndexSummary {
  references: BaiReferenceSummary[]
  unplaced_unmapped_count: number | null
}
