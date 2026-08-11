import type { AlignmentSummary, ReferenceSequence } from './types'
import init, { parse_bam_header_json, query_bam_reference_json } from '@bamviz-wasm'

let initialization: Promise<void> | undefined

function initializeWasm(): Promise<void> {
  initialization ??= init().then(() => undefined)
  return initialization!
}

export async function parseBamHeader(bytes: Uint8Array): Promise<ReferenceSequence[]> {
  await initializeWasm()
  return parse_bam_header_json(bytes) as ReferenceSequence[]
}

export async function queryBamReference(bytes: Uint8Array, referenceIndex: number): Promise<AlignmentSummary[]> {
  await initializeWasm()
  return query_bam_reference_json(bytes, referenceIndex) as AlignmentSummary[]
}
