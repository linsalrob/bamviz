//! File-format adapters. BAM data is converted into `bamviz-core` types here.

use std::io::{Cursor, Read};

use bamviz_core::{
    deterministic_reservoir_slot, AlignedBlock, AlignmentDensity, AlignmentFilter, AlignmentFlags,
    AlignmentQueryResult, AlignmentSummary, GenomicInterval, Insertion, ReferenceSequence,
    ReferenceSpan,
};
use flate2::read::MultiGzDecoder;
use thiserror::Error;

const BAM_MAGIC: &[u8; 4] = b"BAM\x01";
const BAI_MAGIC: &[u8; 4] = b"BAI\x01";
const BAI_METADATA_BIN: u32 = 37_450;
/// Kept in sync with the number of rows rendered by the M1 browser view.
pub const MAX_ALIGNMENT_SUMMARIES: usize = 100;
pub const DENSITY_BIN_COUNT: usize = 256;

#[derive(Debug, Error, PartialEq)]
pub enum FastaError {
    #[error("the FASTA file has no records")]
    NoRecords,
    #[error("the FASTA file has an empty record name")]
    EmptyName,
    #[error("reference {0} was not found in the FASTA")]
    MissingReference(String),
}

/// Parsed FASTA records retained for repeated local reference slices.
pub struct FastaRecords {
    records: Vec<(String, String)>,
}

impl FastaRecords {
    pub fn parse(input: &[u8]) -> Result<Self, FastaError> {
        let text = String::from_utf8_lossy(input);
        let mut records = Vec::new();
        let mut name: Option<String> = None;
        let mut sequence = String::new();
        for line in text.lines() {
            if let Some(header) = line.strip_prefix('>') {
                if let Some(previous) = name.take() {
                    records.push((previous, std::mem::take(&mut sequence)));
                }
                let next = header.split_whitespace().next().unwrap_or_default();
                if next.is_empty() {
                    return Err(FastaError::EmptyName);
                }
                name = Some(next.into());
            } else if name.is_some() {
                sequence.extend(
                    line.bytes()
                        .filter(|byte| !byte.is_ascii_whitespace())
                        .map(|byte| (byte as char).to_ascii_uppercase()),
                );
            }
        }
        if let Some(previous) = name {
            records.push((previous, sequence));
        }
        if records.is_empty() {
            Err(FastaError::NoRecords)
        } else {
            Ok(Self { records })
        }
    }

    pub fn references(&self) -> Vec<ReferenceSequence> {
        self.records
            .iter()
            .map(|(name, sequence)| ReferenceSequence::new(name, sequence.len() as u32))
            .collect()
    }

    pub fn reference_slice(&self, name: &str, start: u32, end: u32) -> Result<String, FastaError> {
        let (_, sequence) = self
            .records
            .iter()
            .find(|(record_name, _)| record_name == name)
            .ok_or_else(|| FastaError::MissingReference(name.into()))?;
        let start = start.min(sequence.len() as u32) as usize;
        let end = end.min(sequence.len() as u32).max(start as u32) as usize;
        Ok(sequence[start..end].to_string())
    }
}

pub fn parse_fasta_references(input: &[u8]) -> Result<Vec<ReferenceSequence>, FastaError> {
    Ok(FastaRecords::parse(input)?.references())
}

pub fn fasta_reference_slice(
    input: &[u8],
    name: &str,
    start: u32,
    end: u32,
) -> Result<String, FastaError> {
    FastaRecords::parse(input)?.reference_slice(name, start, end)
}

pub fn parse_fai_references(input: &[u8]) -> Result<Vec<ReferenceSequence>, FastaError> {
    let references = std::str::from_utf8(input)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let mut columns = line.split('\t');
            let name = columns.next()?.trim();
            let length = columns.next()?.parse::<u32>().ok()?;
            (!name.is_empty()).then(|| ReferenceSequence::new(name, length))
        })
        .collect::<Vec<_>>();
    if references.is_empty() {
        Err(FastaError::NoRecords)
    } else {
        Ok(references)
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum BamHeaderError {
    #[error("the BAM stream could not be decompressed: {0}")]
    Decompression(String),
    #[error("the BAM header is truncated")]
    Truncated,
    #[error("the file is not a BAM stream (expected BAM\\x01 after BGZF decompression)")]
    InvalidMagic,
    #[error("the BAM header has an invalid {field} length")]
    InvalidLength { field: &'static str },
    #[error("the BAM header contains an invalid reference name")]
    InvalidReferenceName,
    #[error("the BAM record stream is truncated")]
    TruncatedRecord,
    #[error("the BAM record has an invalid block size")]
    InvalidRecordSize,
    #[error("the BAM record has an invalid CIGAR operation")]
    InvalidCigar,
    #[error("the BAM record has malformed auxiliary data")]
    InvalidAuxiliaryData,
}

#[derive(Debug, Error, PartialEq)]
pub enum BaiError {
    #[error("the BAI index is truncated")]
    Truncated,
    #[error("the file is not a BAI index (expected BAI\\x01)")]
    InvalidMagic,
    #[error("the BAI index has an invalid {field} count")]
    InvalidCount { field: &'static str },
    #[error("the BAI metadata bin has an invalid chunk count")]
    InvalidMetadata,
}

#[derive(Clone, Copy)]
struct BaiChunk {
    begin: u64,
    end: u64,
}

/// The metadata needed to describe one reference's BAI entries without retaining chunks.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct BaiReferenceSummary {
    pub bin_count: u32,
    pub chunk_count: u64,
    pub linear_interval_count: u32,
    pub mapped_count: Option<u64>,
    pub unmapped_count: Option<u64>,
}

/// A compact, serialisable BAI summary for local browser validation and UI feedback.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct BaiIndexSummary {
    pub references: Vec<BaiReferenceSummary>,
    pub unplaced_unmapped_count: Option<u64>,
}

/// Parses a BAI index without reading the BAM or retaining its virtual-offset chunks.
pub fn parse_bai_index(input: &[u8]) -> Result<BaiIndexSummary, BaiError> {
    let mut cursor = 0;
    if take_bai(input, &mut cursor, 4)? != BAI_MAGIC {
        return Err(BaiError::InvalidMagic);
    }
    let reference_count = read_bai_u32(input, &mut cursor, "reference")? as usize;
    if reference_count > input.len().saturating_sub(cursor) / 8 {
        return Err(BaiError::Truncated);
    }
    let mut references = Vec::with_capacity(reference_count);
    for _ in 0..reference_count {
        let bin_count = read_bai_u32(input, &mut cursor, "bin")?;
        let mut chunk_count = 0_u64;
        let mut mapped_count = None;
        let mut unmapped_count = None;
        for _ in 0..bin_count {
            let bin = read_bai_u32(input, &mut cursor, "bin")?;
            let count = read_bai_u32(input, &mut cursor, "chunk")?;
            if bin == BAI_METADATA_BIN {
                if count != 2 {
                    return Err(BaiError::InvalidMetadata);
                }
                take_bai(input, &mut cursor, 16)?; // ref_beg and ref_end virtual offsets
                mapped_count = Some(read_bai_u64(input, &mut cursor, "metadata")?);
                unmapped_count = Some(read_bai_u64(input, &mut cursor, "metadata")?);
            } else {
                let byte_count = (count as usize)
                    .checked_mul(16)
                    .ok_or(BaiError::InvalidCount { field: "chunk" })?;
                take_bai(input, &mut cursor, byte_count)?;
                chunk_count += u64::from(count);
            }
        }
        let linear_interval_count = read_bai_u32(input, &mut cursor, "linear interval")?;
        let byte_count =
            (linear_interval_count as usize)
                .checked_mul(8)
                .ok_or(BaiError::InvalidCount {
                    field: "linear interval",
                })?;
        take_bai(input, &mut cursor, byte_count)?;
        references.push(BaiReferenceSummary {
            bin_count,
            chunk_count,
            linear_interval_count,
            mapped_count,
            unmapped_count,
        });
    }
    let unplaced_unmapped_count = match input.len().saturating_sub(cursor) {
        0 => None,
        8 => Some(read_bai_u64(input, &mut cursor, "unplaced unmapped")?),
        _ => return Err(BaiError::Truncated),
    };
    Ok(BaiIndexSummary {
        references,
        unplaced_unmapped_count,
    })
}

fn bai_chunks_for_region(
    input: &[u8],
    reference_index: usize,
    start: u32,
    end: u32,
) -> Result<Vec<BaiChunk>, BaiError> {
    let wanted = bai_bins_for_region(start, end);
    let mut cursor = 4;
    if input.get(..4) != Some(BAI_MAGIC) {
        return Err(BaiError::InvalidMagic);
    }
    let reference_count = read_bai_u32(input, &mut cursor, "reference")? as usize;
    if reference_index >= reference_count {
        return Ok(Vec::new());
    }
    let mut chunks = Vec::new();
    for reference in 0..reference_count {
        let bin_count = read_bai_u32(input, &mut cursor, "bin")?;
        for _ in 0..bin_count {
            let bin = read_bai_u32(input, &mut cursor, "bin")?;
            let count = read_bai_u32(input, &mut cursor, "chunk")?;
            for _ in 0..count {
                let begin = read_bai_u64(input, &mut cursor, "chunk")?;
                let finish = read_bai_u64(input, &mut cursor, "chunk")?;
                if reference == reference_index && bin != BAI_METADATA_BIN && wanted.contains(&bin)
                {
                    chunks.push(BaiChunk { begin, end: finish });
                }
            }
        }
        let linear_count = read_bai_u32(input, &mut cursor, "linear interval")? as usize;
        take_bai(
            input,
            &mut cursor,
            linear_count.checked_mul(8).ok_or(BaiError::InvalidCount {
                field: "linear interval",
            })?,
        )?;
    }
    chunks.sort_by_key(|chunk| chunk.begin);
    Ok(coalesce_bai_chunks(chunks))
}

/// Merges overlapping or adjacent virtual-offset ranges so an indexed record is
/// decoded exactly once even when it appears in several BAI bins.
fn coalesce_bai_chunks(chunks: Vec<BaiChunk>) -> Vec<BaiChunk> {
    let mut merged: Vec<BaiChunk> = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        if let Some(previous) = merged
            .last_mut()
            .filter(|previous| chunk.begin <= previous.end)
        {
            previous.end = previous.end.max(chunk.end);
        } else {
            merged.push(chunk);
        }
    }
    merged
}

fn bai_bins_for_region(start: u32, end: u32) -> Vec<u32> {
    if start >= end {
        return Vec::new();
    }
    let end = end - 1;
    let mut bins = vec![0];
    for (offset, shift) in [(1, 26), (9, 23), (73, 20), (585, 17), (4681, 14)] {
        for bin in offset + (start >> shift)..=offset + (end >> shift) {
            bins.push(bin);
        }
    }
    bins
}

/// Uses BAI virtual-offset chunks to decode only BGZF blocks selected for a region.
pub fn query_bam_region_indexed_with_filter(
    input: &[u8],
    bai: &[u8],
    reference_index: usize,
    start: u32,
    end: u32,
    filter: AlignmentFilter,
) -> Result<AlignmentQueryResult, BamHeaderError> {
    let mut header = MultiGzDecoder::new(input);
    let references = read_bam_header(&mut header)?;
    let chunks = bai_chunks_for_region(bai, reference_index, start, end)
        .map_err(|_| BamHeaderError::InvalidRecordSize)?;
    let mut alignments = Vec::new();
    let mut total_count = 0;
    let mut density = AlignmentDensity::new(
        GenomicInterval::new(start, end).unwrap_or(GenomicInterval { start, end: start }),
        DENSITY_BIN_COUNT,
    );
    for chunk in chunks {
        let mut reader =
            BgzfChunkReader::new(input, chunk).map_err(|_| BamHeaderError::InvalidRecordSize)?;
        while let Some(record) = read_bam_record(&mut reader)? {
            let flags = AlignmentFlags::from_sam_flags(record.flags);
            if record.reference_index == reference_index as i32
                && record.start >= 0
                && record.flags & 0x4 == 0
                && (record.start as u32) < end
                && record.end > start
                && filter.matches(record.mapping_quality, &flags)
            {
                total_count += 1;
                density.add(record.start as u32, record.end);
                if let Some(slot) =
                    deterministic_reservoir_slot(total_count - 1, MAX_ALIGNMENT_SUMMARIES)
                {
                    let summary = summary_from_record(record, flags, &references);
                    if slot == alignments.len() {
                        alignments.push(summary);
                    } else {
                        alignments[slot] = summary;
                    }
                }
            }
        }
    }
    alignments
        .sort_by_key(|alignment| (alignment.start, alignment.end, alignment.read_name.clone()));
    Ok(AlignmentQueryResult {
        total_count,
        truncated: total_count > alignments.len() as u64,
        alignments,
        density: density.finish(),
    })
}

struct BgzfChunkReader<'a> {
    data: &'a [u8],
    next_block: usize,
    chunk_end: u64,
    block: Vec<u8>,
    cursor: usize,
    limit: usize,
}
impl<'a> BgzfChunkReader<'a> {
    fn new(data: &'a [u8], chunk: BaiChunk) -> Result<Self, BaiError> {
        let mut reader = Self {
            data,
            next_block: (chunk.begin >> 16) as usize,
            chunk_end: chunk.end,
            block: Vec::new(),
            cursor: (chunk.begin & 0xffff) as usize,
            limit: 0,
        };
        reader.load_block()?;
        Ok(reader)
    }
    fn load_block(&mut self) -> Result<(), BaiError> {
        let address = self.next_block as u64;
        if address > self.chunk_end >> 16 {
            self.block.clear();
            return Ok(());
        }
        let header = self
            .data
            .get(self.next_block..self.next_block + 18)
            .ok_or(BaiError::Truncated)?;
        if header[..3] != [31, 139, 8] || header[3] & 4 == 0 || header[12..14] != *b"BC" {
            return Err(BaiError::InvalidMagic);
        }
        let size = u16::from_le_bytes(header[16..18].try_into().expect("BGZF size")) as usize + 1;
        let encoded = self
            .data
            .get(self.next_block..self.next_block + size)
            .ok_or(BaiError::Truncated)?;
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(Cursor::new(encoded))
            .read_to_end(&mut decoded)
            .map_err(|_| BaiError::Truncated)?;
        self.next_block += size;
        self.block = decoded;
        self.cursor = 0;
        self.limit = if address == self.chunk_end >> 16 {
            (self.chunk_end & 0xffff) as usize
        } else {
            self.block.len()
        };
        self.limit = self.limit.min(self.block.len());
        Ok(())
    }
}
impl Read for BgzfChunkReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.cursor >= self.limit {
            self.load_block()
                .map_err(|_| std::io::Error::from(std::io::ErrorKind::UnexpectedEof))?;
            if self.block.is_empty() {
                return Ok(0);
            }
        }
        let count = out.len().min(self.limit - self.cursor);
        out[..count].copy_from_slice(&self.block[self.cursor..self.cursor + count]);
        self.cursor += count;
        Ok(count)
    }
}

fn take_bai<'a>(input: &'a [u8], cursor: &mut usize, length: usize) -> Result<&'a [u8], BaiError> {
    let end = cursor.checked_add(length).ok_or(BaiError::Truncated)?;
    let bytes = input.get(*cursor..end).ok_or(BaiError::Truncated)?;
    *cursor = end;
    Ok(bytes)
}

fn read_bai_u32(input: &[u8], cursor: &mut usize, field: &'static str) -> Result<u32, BaiError> {
    let bytes: [u8; 4] = take_bai(input, cursor, 4)?
        .try_into()
        .expect("fixed BAI u32");
    let value = u32::from_le_bytes(bytes);
    if field == "reference" && value >= (1 << 31) {
        return Err(BaiError::InvalidCount { field });
    }
    Ok(value)
}

fn read_bai_u64(input: &[u8], cursor: &mut usize, field: &'static str) -> Result<u64, BaiError> {
    let bytes: [u8; 8] = take_bai(input, cursor, 8)?
        .try_into()
        .expect("fixed BAI u64");
    let _ = field;
    Ok(u64::from_le_bytes(bytes))
}

/// Reads reference names and lengths from a BGZF-compressed BAM header.
///
/// This deliberately stops after the header; indexed, viewport-bounded record access
/// will be added separately rather than expanding all records into browser memory.
pub fn parse_bam_header(input: &[u8]) -> Result<Vec<ReferenceSequence>, BamHeaderError> {
    let mut decoded = MultiGzDecoder::new(input);
    read_bam_header(&mut decoded)
}

/// Sequentially scans a BAM for mapped alignments on one reference.
///
/// This is the unindexed M1 path. It deliberately returns compact rendering DTOs
/// rather than raw BAM records. BAI-backed viewport queries can replace this scan
/// without changing its domain result in a later milestone.
pub fn query_bam_reference(
    input: &[u8],
    reference_index: usize,
) -> Result<AlignmentQueryResult, BamHeaderError> {
    query_bam_region_with_filter(
        input,
        reference_index,
        0,
        u32::MAX,
        AlignmentFilter::default(),
    )
}

/// Sequentially scans mapped alignments overlapping a 0-based half-open region.
///
/// This preserves bounded browser DTOs while ensuring the unindexed fallback renders
/// the current viewport rather than an unrelated prefix of the contig.
pub fn query_bam_region(
    input: &[u8],
    reference_index: usize,
    start: u32,
    end: u32,
) -> Result<AlignmentQueryResult, BamHeaderError> {
    query_bam_region_with_filter(
        input,
        reference_index,
        start,
        end,
        AlignmentFilter::default(),
    )
}

/// Sequentially scans mapped alignments overlapping a region with Rust-owned filters.
pub fn query_bam_region_with_filter(
    input: &[u8],
    reference_index: usize,
    start: u32,
    end: u32,
    filter: AlignmentFilter,
) -> Result<AlignmentQueryResult, BamHeaderError> {
    let mut decoded = MultiGzDecoder::new(input);
    let references = read_bam_header(&mut decoded)?;
    if reference_index >= references.len() {
        return Ok(AlignmentQueryResult {
            total_count: 0,
            alignments: Vec::new(),
            truncated: false,
            density: vec![0; DENSITY_BIN_COUNT],
        });
    }

    let mut alignments = Vec::new();
    let mut total_count = 0_u64;
    let mut density = AlignmentDensity::new(
        GenomicInterval::new(start, end).unwrap_or(GenomicInterval { start, end: start }),
        DENSITY_BIN_COUNT,
    );
    while let Some(record) = read_bam_record(&mut decoded)? {
        let flags = AlignmentFlags::from_sam_flags(record.flags);
        if record.reference_index == reference_index as i32
            && record.start >= 0
            && record.flags & 0x4 == 0
            && (record.start as u32) < end
            && record.end > start
            && filter.matches(record.mapping_quality, &flags)
        {
            total_count += 1;
            density.add(record.start as u32, record.end);
            if let Some(slot) =
                deterministic_reservoir_slot(total_count - 1, MAX_ALIGNMENT_SUMMARIES)
            {
                let summary = summary_from_record(record, flags, &references);
                if slot == alignments.len() {
                    alignments.push(summary);
                } else {
                    alignments[slot] = summary;
                }
            }
        }
    }
    alignments
        .sort_by_key(|alignment| (alignment.start, alignment.end, alignment.read_name.clone()));
    Ok(AlignmentQueryResult {
        total_count,
        truncated: total_count > alignments.len() as u64,
        alignments,
        density: density.finish(),
    })
}

fn summary_from_record(
    record: DecodedRecord,
    flags: AlignmentFlags,
    references: &[ReferenceSequence],
) -> AlignmentSummary {
    AlignmentSummary {
        read_name: record.read_name,
        start: record.start as u32,
        end: record.end,
        mapping_quality: record.mapping_quality,
        flags,
        cigar: record.cigar,
        left_clip: record.left_clip,
        right_clip: record.right_clip,
        mate_reference: record
            .mate_reference_index
            .try_into()
            .ok()
            .and_then(|index: usize| references.get(index))
            .map(|reference| reference.name.clone()),
        mate_start: u32::try_from(record.mate_start).ok(),
        blocks: record.blocks,
        deletions: record.deletions,
        insertions: record.insertions,
    }
}

fn read_bam_header(reader: &mut impl Read) -> Result<Vec<ReferenceSequence>, BamHeaderError> {
    let mut magic = [0; 4];
    read_exact(reader, &mut magic)?;
    if magic != *BAM_MAGIC {
        return Err(BamHeaderError::InvalidMagic);
    }
    let text_length = read_i32(reader, "header text")?;
    let text_length = usize::try_from(text_length).map_err(|_| BamHeaderError::InvalidLength {
        field: "header text",
    })?;
    let mut text = vec![0; text_length];
    read_exact(reader, &mut text)?;
    let reference_count = read_i32(reader, "reference count")?;
    let reference_count =
        usize::try_from(reference_count).map_err(|_| BamHeaderError::InvalidLength {
            field: "reference count",
        })?;

    let mut references = Vec::with_capacity(reference_count);
    for _ in 0..reference_count {
        let name_length = read_i32(reader, "reference name")?;
        let name_length =
            usize::try_from(name_length).map_err(|_| BamHeaderError::InvalidLength {
                field: "reference name",
            })?;
        let mut encoded_name = vec![0; name_length];
        read_exact(reader, &mut encoded_name)?;
        let name = encoded_name
            .strip_suffix(&[0])
            .ok_or(BamHeaderError::InvalidReferenceName)?;
        let name = std::str::from_utf8(name).map_err(|_| BamHeaderError::InvalidReferenceName)?;
        if name.is_empty() {
            return Err(BamHeaderError::InvalidReferenceName);
        }
        let length = read_i32(reader, "reference length")?;
        let length = u32::try_from(length).map_err(|_| BamHeaderError::InvalidLength {
            field: "reference length",
        })?;
        references.push(ReferenceSequence::new(name, length));
    }
    Ok(references)
}

struct DecodedRecord {
    read_name: String,
    reference_index: i32,
    start: i32,
    end: u32,
    mapping_quality: u8,
    flags: u16,
    mate_reference_index: i32,
    mate_start: i32,
    cigar: String,
    blocks: Vec<AlignedBlock>,
    deletions: Vec<ReferenceSpan>,
    insertions: Vec<Insertion>,
    left_clip: u32,
    right_clip: u32,
}

type CigarProjection = (Vec<AlignedBlock>, Vec<ReferenceSpan>, Vec<Insertion>);

fn read_bam_record(reader: &mut impl Read) -> Result<Option<DecodedRecord>, BamHeaderError> {
    let Some(block_size) = read_optional_i32(reader)? else {
        return Ok(None);
    };
    let block_size = usize::try_from(block_size).map_err(|_| BamHeaderError::InvalidRecordSize)?;
    if block_size < 32 {
        return Err(BamHeaderError::InvalidRecordSize);
    }
    let mut block = vec![0; block_size];
    read_exact_record(reader, &mut block)?;

    let reference_index = i32::from_le_bytes(block[0..4].try_into().expect("fixed BAM record"));
    let start = i32::from_le_bytes(block[4..8].try_into().expect("fixed BAM record"));
    let read_name_length = block[8] as usize;
    let mapping_quality = block[9];
    let cigar_count =
        u16::from_le_bytes(block[12..14].try_into().expect("fixed BAM record")) as usize;
    let flags = u16::from_le_bytes(block[14..16].try_into().expect("fixed BAM record"));
    let sequence_length = i32::from_le_bytes(block[16..20].try_into().expect("fixed BAM record"));
    let mate_reference_index =
        i32::from_le_bytes(block[20..24].try_into().expect("fixed BAM record"));
    let mate_start = i32::from_le_bytes(block[24..28].try_into().expect("fixed BAM record"));
    let sequence_length =
        usize::try_from(sequence_length).map_err(|_| BamHeaderError::InvalidRecordSize)?;
    let cigar_start = 32usize
        .checked_add(read_name_length)
        .ok_or(BamHeaderError::InvalidRecordSize)?;
    let cigar_bytes = cigar_count
        .checked_mul(4)
        .ok_or(BamHeaderError::InvalidRecordSize)?;
    let cigar_end = cigar_start
        .checked_add(cigar_bytes)
        .ok_or(BamHeaderError::InvalidRecordSize)?;
    let sequence_bytes = sequence_length.div_ceil(2);
    let minimum_size = cigar_end
        .checked_add(sequence_bytes)
        .and_then(|end| end.checked_add(sequence_length))
        .ok_or(BamHeaderError::InvalidRecordSize)?;
    if minimum_size > block.len() {
        return Err(BamHeaderError::InvalidRecordSize);
    }
    let read_name = block[32..cigar_start]
        .strip_suffix(&[0])
        .and_then(|name| std::str::from_utf8(name).ok())
        .ok_or(BamHeaderError::InvalidRecordSize)?
        .to_owned();

    let core_cigar = block[cigar_start..cigar_end]
        .chunks_exact(4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("fixed CIGAR operation")))
        .collect::<Vec<_>>();
    let auxiliary_start = minimum_size;
    let cigar_operations = long_cigar_operations(&block[auxiliary_start..])?.unwrap_or(core_cigar);
    let (reference_span, cigar) = decode_cigar_operations(&cigar_operations)?;
    let (left_clip, right_clip) = clip_lengths(&cigar_operations);
    let sequence = decode_sequence(
        &block[cigar_end..cigar_end + sequence_bytes],
        sequence_length,
    );
    let end = if start < 0 {
        0
    } else {
        u32::try_from(start)
            .ok()
            .and_then(|start| start.checked_add(reference_span))
            .ok_or(BamHeaderError::InvalidRecordSize)?
    };
    let (blocks, deletions, insertions) = project_cigar(start, &cigar_operations, &sequence)?;
    Ok(Some(DecodedRecord {
        read_name,
        reference_index,
        start,
        end,
        mapping_quality,
        flags,
        mate_reference_index,
        mate_start,
        cigar,
        blocks,
        deletions,
        insertions,
        left_clip,
        right_clip,
    }))
}

fn clip_lengths(operations: &[u32]) -> (u32, u32) {
    let is_clipping = |operation: &&u32| matches!(**operation & 0x0f, 4 | 5);
    let left = operations
        .iter()
        .take_while(is_clipping)
        .map(|operation| operation >> 4)
        .sum();
    let right = operations
        .iter()
        .rev()
        .take_while(is_clipping)
        .map(|operation| operation >> 4)
        .sum();
    (left, right)
}

fn decode_sequence(encoded: &[u8], length: usize) -> String {
    let decode = |code: u8| match code {
        1 => 'A',
        2 => 'C',
        4 => 'G',
        8 => 'T',
        15 => 'N',
        _ => 'N',
    };
    encoded
        .iter()
        .flat_map(|byte| [decode(byte >> 4), decode(byte & 0x0f)])
        .take(length)
        .collect()
}

fn project_cigar(
    start: i32,
    operations: &[u32],
    sequence: &str,
) -> Result<CigarProjection, BamHeaderError> {
    if start < 0 {
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    }
    let mut reference = start as u32;
    let mut query = 0_usize;
    let bases = sequence.as_bytes();
    let mut blocks = Vec::new();
    let mut deletions = Vec::new();
    let mut insertions = Vec::new();
    for encoded in operations {
        let length = (encoded >> 4) as usize;
        match encoded & 0x0f {
            0 | 7 | 8 => {
                let end_query = query
                    .checked_add(length)
                    .ok_or(BamHeaderError::InvalidRecordSize)?;
                let text = std::str::from_utf8(
                    bases
                        .get(query..end_query)
                        .ok_or(BamHeaderError::InvalidRecordSize)?,
                )
                .map_err(|_| BamHeaderError::InvalidRecordSize)?;
                let end = reference
                    .checked_add(length as u32)
                    .ok_or(BamHeaderError::InvalidRecordSize)?;
                blocks.push(AlignedBlock {
                    start: reference,
                    end,
                    bases: text.into(),
                });
                reference = end;
                query = end_query;
            }
            1 => {
                let end_query = query
                    .checked_add(length)
                    .ok_or(BamHeaderError::InvalidRecordSize)?;
                let text = std::str::from_utf8(
                    bases
                        .get(query..end_query)
                        .ok_or(BamHeaderError::InvalidRecordSize)?,
                )
                .map_err(|_| BamHeaderError::InvalidRecordSize)?;
                insertions.push(Insertion {
                    position: reference,
                    bases: text.into(),
                });
                query = end_query;
            }
            2 | 3 => {
                let end = reference
                    .checked_add(length as u32)
                    .ok_or(BamHeaderError::InvalidRecordSize)?;
                deletions.push(ReferenceSpan {
                    start: reference,
                    end,
                });
                reference = end;
            }
            4 => {
                query = query
                    .checked_add(length)
                    .ok_or(BamHeaderError::InvalidRecordSize)?;
                if query > bases.len() {
                    return Err(BamHeaderError::InvalidRecordSize);
                }
            }
            5 | 6 => {}
            _ => return Err(BamHeaderError::InvalidCigar),
        }
    }
    Ok((blocks, deletions, insertions))
}

fn decode_cigar_operations(operations: &[u32]) -> Result<(u32, String), BamHeaderError> {
    let mut reference_span = 0_u32;
    let mut cigar = String::new();
    for encoded in operations {
        let encoded = *encoded;
        let length = encoded >> 4;
        let operation = match encoded & 0x0f {
            0 => 'M',
            1 => 'I',
            2 => 'D',
            3 => 'N',
            4 => 'S',
            5 => 'H',
            6 => 'P',
            7 => '=',
            8 => 'X',
            _ => return Err(BamHeaderError::InvalidCigar),
        };
        if matches!(operation, 'M' | 'D' | 'N' | '=' | 'X') {
            reference_span = reference_span
                .checked_add(length)
                .ok_or(BamHeaderError::InvalidRecordSize)?;
        }
        use std::fmt::Write;
        write!(cigar, "{length}{operation}").expect("writing to string cannot fail");
    }
    Ok((reference_span, cigar))
}

fn long_cigar_operations(auxiliary: &[u8]) -> Result<Option<Vec<u32>>, BamHeaderError> {
    let mut cursor = 0;
    while cursor < auxiliary.len() {
        let tag = take_auxiliary(auxiliary, &mut cursor, 2)?;
        let kind = *take_auxiliary(auxiliary, &mut cursor, 1)?
            .first()
            .expect("one-byte slice");
        match kind {
            b'A' | b'c' | b'C' => skip_auxiliary(auxiliary, &mut cursor, 1)?,
            b's' | b'S' => skip_auxiliary(auxiliary, &mut cursor, 2)?,
            b'i' | b'I' | b'f' => skip_auxiliary(auxiliary, &mut cursor, 4)?,
            b'Z' | b'H' => {
                let remaining = &auxiliary[cursor..];
                let Some(terminator) = remaining.iter().position(|byte| *byte == 0) else {
                    return Err(BamHeaderError::InvalidAuxiliaryData);
                };
                cursor += terminator + 1;
            }
            b'B' => {
                let subtype = *take_auxiliary(auxiliary, &mut cursor, 1)?
                    .first()
                    .expect("one-byte slice");
                let count = read_auxiliary_i32(auxiliary, &mut cursor)?;
                let count =
                    usize::try_from(count).map_err(|_| BamHeaderError::InvalidAuxiliaryData)?;
                let element_size = match subtype {
                    b'c' | b'C' => 1,
                    b's' | b'S' => 2,
                    b'i' | b'I' | b'f' => 4,
                    _ => return Err(BamHeaderError::InvalidAuxiliaryData),
                };
                let byte_count = count
                    .checked_mul(element_size)
                    .ok_or(BamHeaderError::InvalidAuxiliaryData)?;
                let values = take_auxiliary(auxiliary, &mut cursor, byte_count)?;
                if tag == b"CG" && subtype == b'I' {
                    return Ok(Some(
                        values
                            .chunks_exact(4)
                            .map(|bytes| {
                                u32::from_le_bytes(bytes.try_into().expect("fixed CG operation"))
                            })
                            .collect(),
                    ));
                }
            }
            _ => return Err(BamHeaderError::InvalidAuxiliaryData),
        }
    }
    Ok(None)
}

fn take_auxiliary<'a>(
    input: &'a [u8],
    cursor: &mut usize,
    length: usize,
) -> Result<&'a [u8], BamHeaderError> {
    let end = cursor
        .checked_add(length)
        .ok_or(BamHeaderError::InvalidAuxiliaryData)?;
    let value = input
        .get(*cursor..end)
        .ok_or(BamHeaderError::InvalidAuxiliaryData)?;
    *cursor = end;
    Ok(value)
}

fn skip_auxiliary(input: &[u8], cursor: &mut usize, length: usize) -> Result<(), BamHeaderError> {
    take_auxiliary(input, cursor, length).map(|_| ())
}

fn read_auxiliary_i32(input: &[u8], cursor: &mut usize) -> Result<i32, BamHeaderError> {
    let bytes: [u8; 4] = take_auxiliary(input, cursor, 4)?
        .try_into()
        .expect("fixed auxiliary integer");
    Ok(i32::from_le_bytes(bytes))
}

fn read_exact(reader: &mut impl Read, buffer: &mut [u8]) -> Result<(), BamHeaderError> {
    reader
        .read_exact(buffer)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::UnexpectedEof => BamHeaderError::Truncated,
            _ => BamHeaderError::Decompression(error.to_string()),
        })
}

fn read_i32(reader: &mut impl Read, field: &'static str) -> Result<i32, BamHeaderError> {
    let mut bytes = [0; 4];
    read_exact(reader, &mut bytes)?;
    let value = i32::from_le_bytes(bytes);
    if value < 0 {
        return Err(BamHeaderError::InvalidLength { field });
    }
    Ok(value)
}

fn read_optional_i32(reader: &mut impl Read) -> Result<Option<i32>, BamHeaderError> {
    let mut first = [0; 1];
    match reader.read(&mut first) {
        Ok(0) => return Ok(None),
        Ok(_) => {}
        Err(error) => return Err(BamHeaderError::Decompression(error.to_string())),
    }
    let mut remaining = [0; 3];
    read_exact_record(reader, &mut remaining)?;
    Ok(Some(i32::from_le_bytes([
        first[0],
        remaining[0],
        remaining[1],
        remaining[2],
    ])))
}

fn read_exact_record(reader: &mut impl Read, buffer: &mut [u8]) -> Result<(), BamHeaderError> {
    reader
        .read_exact(buffer)
        .map_err(|_| BamHeaderError::TruncatedRecord)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::{write::GzEncoder, Compression};

    use super::{
        clip_lengths, coalesce_bai_chunks, fasta_reference_slice, long_cigar_operations,
        parse_bai_index, parse_bam_header, parse_fai_references, parse_fasta_references,
        query_bam_reference, query_bam_region, BaiChunk, BaiError, BamHeaderError,
        DENSITY_BIN_COUNT, MAX_ALIGNMENT_SUMMARIES,
    };

    fn compressed_header(references: &[(&str, i32)]) -> Vec<u8> {
        let mut raw = b"BAM\x01".to_vec();
        raw.extend_from_slice(&0_i32.to_le_bytes());
        raw.extend_from_slice(&(references.len() as i32).to_le_bytes());
        for (name, length) in references {
            raw.extend_from_slice(&((name.len() + 1) as i32).to_le_bytes());
            raw.extend_from_slice(name.as_bytes());
            raw.push(0);
            raw.extend_from_slice(&length.to_le_bytes());
        }
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&raw).expect("write test data");
        encoder.finish().expect("compress test data")
    }

    fn compressed_bam(references: &[(&str, i32)], records: &[Vec<u8>]) -> Vec<u8> {
        let mut raw = b"BAM\x01".to_vec();
        raw.extend_from_slice(&0_i32.to_le_bytes());
        raw.extend_from_slice(&(references.len() as i32).to_le_bytes());
        for (name, length) in references {
            raw.extend_from_slice(&((name.len() + 1) as i32).to_le_bytes());
            raw.extend_from_slice(name.as_bytes());
            raw.push(0);
            raw.extend_from_slice(&length.to_le_bytes());
        }
        for record in records {
            raw.extend_from_slice(&(record.len() as i32).to_le_bytes());
            raw.extend_from_slice(record);
        }
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&raw).expect("write test data");
        encoder.finish().expect("compress test data")
    }

    fn record(
        reference_index: i32,
        start: i32,
        flags: u16,
        mapping_quality: u8,
        cigar: &[(u32, u32)],
    ) -> Vec<u8> {
        let sequence_length = cigar
            .iter()
            .filter(|(_, operation)| matches!(operation, 0 | 1 | 4 | 7 | 8))
            .map(|(length, _)| length)
            .sum::<u32>();
        let mut record = Vec::new();
        record.extend_from_slice(&reference_index.to_le_bytes());
        record.extend_from_slice(&start.to_le_bytes());
        record.push(2); // l_read_name: "r\\0"
        record.push(mapping_quality);
        record.extend_from_slice(&0_u16.to_le_bytes());
        record.extend_from_slice(&(cigar.len() as u16).to_le_bytes());
        record.extend_from_slice(&flags.to_le_bytes());
        record.extend_from_slice(&(sequence_length as i32).to_le_bytes());
        record.extend_from_slice(&(-1_i32).to_le_bytes());
        record.extend_from_slice(&(-1_i32).to_le_bytes());
        record.extend_from_slice(&0_i32.to_le_bytes());
        record.extend_from_slice(b"r\0");
        for (length, operation) in cigar {
            record.extend_from_slice(&((length << 4) | operation).to_le_bytes());
        }
        record.extend(std::iter::repeat_n(
            0_u8,
            sequence_length.div_ceil(2) as usize,
        ));
        record.extend(std::iter::repeat_n(255_u8, sequence_length as usize));
        record
    }

    #[test]
    fn reads_references_from_a_compressed_bam_header() {
        let header = compressed_header(&[("chr1", 248_956_422), ("plasmid", 9)]);
        assert_eq!(
            parse_bam_header(&header).expect("valid header"),
            vec![
                bamviz_core::ReferenceSequence::new("chr1", 248_956_422),
                bamviz_core::ReferenceSequence::new("plasmid", 9),
            ]
        );
    }

    #[test]
    fn coalesces_overlapping_bai_chunks_before_decoding_records() {
        assert_eq!(
            coalesce_bai_chunks(vec![
                BaiChunk { begin: 10, end: 30 },
                BaiChunk { begin: 20, end: 40 },
                BaiChunk { begin: 40, end: 50 },
                BaiChunk { begin: 60, end: 70 },
            ])
            .iter()
            .map(|chunk| (chunk.begin, chunk.end))
            .collect::<Vec<_>>(),
            vec![(10, 50), (60, 70)]
        );
    }

    #[test]
    fn rejects_non_bam_gzip_data() {
        let mut data = GzEncoder::new(Vec::new(), Compression::default());
        data.write_all(b"not BAM").expect("write test data");
        assert_eq!(
            parse_bam_header(&data.finish().expect("compress test data")),
            Err(BamHeaderError::InvalidMagic)
        );
    }

    #[test]
    fn returns_compact_summaries_for_the_selected_reference() {
        let bam = compressed_bam(
            &[("chr1", 100), ("chr2", 100)],
            &[
                record(0, 10, 0, 60, &[(5, 0), (2, 1), (3, 2)]),
                record(1, 4, 0x10, 12, &[(8, 7)]),
                record(0, 20, 0x4, 0, &[]),
            ],
        );
        assert_eq!(
            query_bam_reference(&bam, 0).expect("valid BAM"),
            bamviz_core::AlignmentQueryResult {
                total_count: 1,
                alignments: vec![bamviz_core::AlignmentSummary {
                    read_name: "r".into(),
                    start: 10,
                    end: 18,
                    mapping_quality: 60,
                    flags: bamviz_core::AlignmentFlags::from_sam_flags(0),
                    cigar: "5M2I3D".into(),
                    left_clip: 0,
                    right_clip: 0,
                    mate_reference: None,
                    mate_start: None,
                    blocks: vec![bamviz_core::AlignedBlock {
                        start: 10,
                        end: 15,
                        bases: "NNNNN".into()
                    }],
                    deletions: vec![bamviz_core::ReferenceSpan { start: 15, end: 18 }],
                    insertions: vec![bamviz_core::Insertion {
                        position: 15,
                        bases: "NN".into()
                    }],
                }],
                truncated: false,
                density: std::iter::once(1)
                    .chain(std::iter::repeat_n(0, DENSITY_BIN_COUNT - 1))
                    .collect(),
            }
        );
        assert_eq!(
            query_bam_reference(&bam, 1).expect("valid BAM"),
            bamviz_core::AlignmentQueryResult {
                total_count: 1,
                alignments: vec![bamviz_core::AlignmentSummary {
                    read_name: "r".into(),
                    start: 4,
                    end: 12,
                    mapping_quality: 12,
                    flags: bamviz_core::AlignmentFlags::from_sam_flags(0x10),
                    cigar: "8=".into(),
                    left_clip: 0,
                    right_clip: 0,
                    mate_reference: None,
                    mate_start: None,
                    blocks: vec![bamviz_core::AlignedBlock {
                        start: 4,
                        end: 12,
                        bases: "NNNNNNNN".into()
                    }],
                    deletions: vec![],
                    insertions: vec![],
                }],
                truncated: false,
                density: std::iter::once(1)
                    .chain(std::iter::repeat_n(0, DENSITY_BIN_COUNT - 1))
                    .collect(),
            }
        );
    }

    #[test]
    fn bounds_summaries_but_retains_the_exact_alignment_count() {
        let records = (0..=MAX_ALIGNMENT_SUMMARIES)
            .map(|start| record(0, start as i32, 0, 60, &[(1, 0)]))
            .collect::<Vec<_>>();
        let bam = compressed_bam(&[("chr1", 1_000)], &records);
        let result = query_bam_reference(&bam, 0).expect("valid BAM");
        assert_eq!(result.total_count, (MAX_ALIGNMENT_SUMMARIES + 1) as u64);
        assert_eq!(result.alignments.len(), MAX_ALIGNMENT_SUMMARIES);
        assert!(result.truncated);
        assert!(result
            .alignments
            .windows(2)
            .all(|pair| pair[0].start <= pair[1].start));
    }

    #[test]
    fn high_depth_region_retains_bounded_details_and_complete_density() {
        let records = (0..10_000)
            .map(|start| record(0, start, 0, 60, &[(100, 0)]))
            .collect::<Vec<_>>();
        let bam = compressed_bam(&[("chr1", 10_100)], &records);
        let result = query_bam_region(&bam, 0, 0, 10_100).expect("valid BAM");
        assert_eq!(result.total_count, 10_000);
        assert_eq!(result.alignments.len(), MAX_ALIGNMENT_SUMMARIES);
        assert!(result.truncated);
        assert_eq!(result.density.len(), DENSITY_BIN_COUNT);
        assert!(result.density.iter().any(|count| *count > 100));
    }

    #[test]
    fn returns_only_alignments_overlapping_a_half_open_region() {
        let bam = compressed_bam(
            &[("chr1", 1_000)],
            &[
                record(0, 10, 0, 60, &[(5, 0)]),
                record(0, 100, 0, 60, &[(5, 0)]),
            ],
        );
        let result = query_bam_region(&bam, 0, 15, 101).expect("valid BAM");
        assert_eq!(result.total_count, 1);
        assert_eq!(result.alignments[0].start, 100);
        assert!(query_bam_region(&bam, 0, 15, 100)
            .expect("valid BAM")
            .alignments
            .is_empty());
    }

    #[test]
    fn uses_the_cg_auxiliary_tag_for_a_long_cigar() {
        let mut auxiliary = b"CGBI".to_vec();
        auxiliary.extend_from_slice(&2_i32.to_le_bytes());
        auxiliary.extend_from_slice(&(5_u32 << 4).to_le_bytes());
        auxiliary.extend_from_slice(&((2_u32 << 4) | 1).to_le_bytes());
        assert_eq!(
            long_cigar_operations(&auxiliary).expect("valid CG tag"),
            Some(vec![80, 33])
        );
    }

    #[test]
    fn reads_fasta_records_and_a_half_open_slice() {
        let fasta = b">chr1 description\nACGT\nNN\n>plasmid\nTA\n";
        assert_eq!(
            parse_fasta_references(fasta).expect("valid FASTA"),
            vec![
                bamviz_core::ReferenceSequence::new("chr1", 6),
                bamviz_core::ReferenceSequence::new("plasmid", 2),
            ]
        );
        assert_eq!(
            fasta_reference_slice(fasta, "chr1", 1, 5).expect("slice"),
            "CGTN"
        );
    }

    #[test]
    fn reads_fai_metadata_without_claiming_sequence() {
        assert_eq!(
            parse_fai_references(b"chr1\t6\t6\t4\t5\n").expect("valid FAI"),
            vec![bamviz_core::ReferenceSequence::new("chr1", 6)]
        );
    }

    #[test]
    fn reads_bai_reference_and_metadata_summaries() {
        let mut bai = b"BAI\x01".to_vec();
        bai.extend_from_slice(&1_u32.to_le_bytes()); // n_ref
        bai.extend_from_slice(&2_u32.to_le_bytes()); // n_bin
        bai.extend_from_slice(&4681_u32.to_le_bytes());
        bai.extend_from_slice(&1_u32.to_le_bytes());
        bai.extend_from_slice(&10_u64.to_le_bytes());
        bai.extend_from_slice(&20_u64.to_le_bytes());
        bai.extend_from_slice(&37_450_u32.to_le_bytes());
        bai.extend_from_slice(&2_u32.to_le_bytes());
        bai.extend_from_slice(&10_u64.to_le_bytes());
        bai.extend_from_slice(&20_u64.to_le_bytes());
        bai.extend_from_slice(&7_u64.to_le_bytes());
        bai.extend_from_slice(&3_u64.to_le_bytes());
        bai.extend_from_slice(&2_u32.to_le_bytes()); // n_intv
        bai.extend_from_slice(&[0_u8; 16]);
        bai.extend_from_slice(&5_u64.to_le_bytes()); // n_no_coor

        let index = parse_bai_index(&bai).expect("valid BAI");
        assert_eq!(index.references.len(), 1);
        assert_eq!(index.references[0].bin_count, 2);
        assert_eq!(index.references[0].chunk_count, 1);
        assert_eq!(index.references[0].linear_interval_count, 2);
        assert_eq!(index.references[0].mapped_count, Some(7));
        assert_eq!(index.references[0].unmapped_count, Some(3));
        assert_eq!(index.unplaced_unmapped_count, Some(5));
    }

    #[test]
    fn rejects_invalid_bai_magic_and_truncation() {
        assert_eq!(parse_bai_index(b"nope"), Err(BaiError::InvalidMagic));
        assert_eq!(parse_bai_index(b"BAI\x01\x01"), Err(BaiError::Truncated));
        assert_eq!(
            parse_bai_index(b"BAI\x01\xff\xff\xff\x7f"),
            Err(BaiError::Truncated)
        );
    }

    #[test]
    fn sums_mixed_terminal_hard_and_soft_clipping() {
        assert_eq!(
            clip_lengths(&[
                (5 << 4) | 5,
                (10 << 4) | 4,
                80 << 4,
                (5 << 4) | 4,
                (5 << 4) | 5
            ]),
            (15, 10)
        );
    }
}
