import type { AlignmentFilter, AlignmentQueryResult, ReferenceSequence } from './types'
import init, { FastaFile, parse_fai_references_json, parse_bam_header_json, query_bam_reference_json, query_bam_region_filtered_json, query_bam_region_json } from '@bamviz-wasm'

export type CachedFasta = FastaFile

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
export async function queryBamRegion(bytes: Uint8Array, referenceIndex: number, start: number, end: number): Promise<AlignmentQueryResult> {
  await initializeWasm()
  return query_bam_region_json(bytes, referenceIndex, start, end) as AlignmentQueryResult
}
export async function queryBamRegionFiltered(bytes: Uint8Array, referenceIndex: number, start: number, end: number, filter: AlignmentFilter): Promise<AlignmentQueryResult> {
  await initializeWasm()
  return query_bam_region_filtered_json(bytes, referenceIndex, start, end, filter) as AlignmentQueryResult
}
export async function loadFasta(bytes: Uint8Array): Promise<CachedFasta> { await initializeWasm(); return new FastaFile(bytes) }
export function fastaReferences(fasta: CachedFasta): ReferenceSequence[] { return fasta.references_json() as ReferenceSequence[] }
export function fastaReferenceSlice(fasta: CachedFasta, name: string, start: number, end: number): string { return fasta.reference_slice_json(name, start, end) as string }
export async function parseFaiReferences(bytes: Uint8Array): Promise<ReferenceSequence[]> { await initializeWasm(); return parse_fai_references_json(bytes) as ReferenceSequence[] }
