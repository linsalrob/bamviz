import type { AlignmentQueryResult, ReferenceSequence } from './types'
import init, { fasta_reference_slice_json, parse_fai_references_json, parse_fasta_references_json, parse_bam_header_json, query_bam_reference_json } from '@bamviz-wasm'

let initialization: Promise<void> | undefined

function initializeWasm(): Promise<void> {
  initialization ??= init().then(() => undefined)
  return initialization!
}

export async function parseBamHeader(bytes: Uint8Array): Promise<ReferenceSequence[]> {
  await initializeWasm()
  return parse_bam_header_json(bytes) as ReferenceSequence[]
}

export async function queryBamReference(bytes: Uint8Array, referenceIndex: number): Promise<AlignmentQueryResult> {
  await initializeWasm()
  return query_bam_reference_json(bytes, referenceIndex) as AlignmentQueryResult
}
export async function parseFastaReferences(bytes: Uint8Array): Promise<ReferenceSequence[]> { await initializeWasm(); return parse_fasta_references_json(bytes) as ReferenceSequence[] }
export async function fastaReferenceSlice(bytes: Uint8Array, name: string, start: number, end: number): Promise<string> { await initializeWasm(); return fasta_reference_slice_json(bytes, name, start, end) as string }
export async function parseFaiReferences(bytes: Uint8Array): Promise<ReferenceSequence[]> { await initializeWasm(); return parse_fai_references_json(bytes) as ReferenceSequence[] }
