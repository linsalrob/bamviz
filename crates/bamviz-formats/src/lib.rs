//! File-format adapters. BAM data is converted into `bamviz-core` types here.

use std::io::Read;

use bamviz_core::{AlignmentQueryResult, AlignmentSummary, ReferenceSequence};
use flate2::read::MultiGzDecoder;
use thiserror::Error;

const BAM_MAGIC: &[u8; 4] = b"BAM\x01";
/// Kept in sync with the number of rows rendered by the M1 browser view.
pub const MAX_ALIGNMENT_SUMMARIES: usize = 100;

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
    let mut decoded = MultiGzDecoder::new(input);
    let references = read_bam_header(&mut decoded)?;
    if reference_index >= references.len() {
        return Ok(AlignmentQueryResult {
            total_count: 0,
            alignments: Vec::new(),
            truncated: false,
        });
    }

    let mut alignments = Vec::new();
    let mut total_count = 0_u64;
    while let Some(record) = read_bam_record(&mut decoded)? {
        if record.reference_index == reference_index as i32
            && record.start >= 0
            && record.flags & 0x4 == 0
        {
            total_count += 1;
            if alignments.len() < MAX_ALIGNMENT_SUMMARIES {
                alignments.push(AlignmentSummary {
                    start: record.start as u32,
                    end: record.end,
                    mapping_quality: record.mapping_quality,
                    is_reverse: record.flags & 0x10 != 0,
                    cigar: record.cigar,
                });
            }
        }
    }
    Ok(AlignmentQueryResult {
        total_count,
        truncated: total_count > alignments.len() as u64,
        alignments,
    })
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
    reference_index: i32,
    start: i32,
    end: u32,
    mapping_quality: u8,
    flags: u16,
    cigar: String,
}

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

    let core_cigar = block[cigar_start..cigar_end]
        .chunks_exact(4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("fixed CIGAR operation")))
        .collect::<Vec<_>>();
    let auxiliary_start = minimum_size;
    let cigar_operations = long_cigar_operations(&block[auxiliary_start..])?.unwrap_or(core_cigar);
    let (reference_span, cigar) = decode_cigar_operations(&cigar_operations)?;
    let end = if start < 0 {
        0
    } else {
        u32::try_from(start)
            .ok()
            .and_then(|start| start.checked_add(reference_span))
            .ok_or(BamHeaderError::InvalidRecordSize)?
    };
    Ok(Some(DecodedRecord {
        reference_index,
        start,
        end,
        mapping_quality,
        flags,
        cigar,
    }))
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
        long_cigar_operations, parse_bam_header, query_bam_reference, BamHeaderError,
        MAX_ALIGNMENT_SUMMARIES,
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
                    start: 10,
                    end: 18,
                    mapping_quality: 60,
                    is_reverse: false,
                    cigar: "5M2I3D".into(),
                }],
                truncated: false,
            }
        );
        assert_eq!(
            query_bam_reference(&bam, 1).expect("valid BAM"),
            bamviz_core::AlignmentQueryResult {
                total_count: 1,
                alignments: vec![bamviz_core::AlignmentSummary {
                    start: 4,
                    end: 12,
                    mapping_quality: 12,
                    is_reverse: true,
                    cigar: "8=".into(),
                }],
                truncated: false,
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
}
