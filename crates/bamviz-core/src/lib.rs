//! Domain types and coordinate semantics for bamviz.
//!
//! All genomic intervals use 0-based, half-open coordinates: `[start, end)`.

use serde::{Deserialize, Serialize};

/// A named reference sequence declared by a BAM header.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReferenceSequence {
    pub name: String,
    pub length: u32,
}

impl ReferenceSequence {
    pub fn new(name: impl Into<String>, length: u32) -> Self {
        Self {
            name: name.into(),
            length,
        }
    }
}

/// The rendering-relevant portion of one mapped BAM record.
///
/// `start` and `end` use 0-based, half-open reference coordinates.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlignmentSummary {
    pub read_name: String,
    pub start: u32,
    pub end: u32,
    pub mapping_quality: u8,
    pub flags: AlignmentFlags,
    pub cigar: String,
    pub left_clip: u32,
    pub right_clip: u32,
    pub mate_reference: Option<String>,
    pub mate_start: Option<u32>,
    pub blocks: Vec<AlignedBlock>,
    pub deletions: Vec<ReferenceSpan>,
    pub insertions: Vec<Insertion>,
}

/// User-facing interpretation of SAM flag bits for a single alignment.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlignmentFlags {
    pub raw: u16,
    pub is_reverse: bool,
    pub is_paired: bool,
    pub is_proper_pair: bool,
    pub mate_is_reverse: bool,
    pub is_secondary: bool,
    pub is_supplementary: bool,
    pub is_duplicate: bool,
}

impl AlignmentFlags {
    pub fn from_sam_flags(raw: u16) -> Self {
        Self {
            raw,
            is_reverse: raw & 0x10 != 0,
            is_paired: raw & 0x1 != 0,
            is_proper_pair: raw & 0x2 != 0,
            mate_is_reverse: raw & 0x20 != 0,
            is_secondary: raw & 0x100 != 0,
            is_supplementary: raw & 0x800 != 0,
            is_duplicate: raw & 0x400 != 0,
        }
    }
}

/// Rust-owned alignment filter semantics applied before browser DTOs are built.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlignmentFilter {
    pub min_mapping_quality: u8,
    pub include_secondary: bool,
    pub include_supplementary: bool,
    pub include_duplicates: bool,
}

impl Default for AlignmentFilter {
    fn default() -> Self {
        Self {
            min_mapping_quality: 0,
            include_secondary: true,
            include_supplementary: true,
            include_duplicates: true,
        }
    }
}

impl AlignmentFilter {
    pub fn matches(self, mapping_quality: u8, flags: &AlignmentFlags) -> bool {
        mapping_quality >= self.min_mapping_quality
            && (self.include_secondary || !flags.is_secondary)
            && (self.include_supplementary || !flags.is_supplementary)
            && (self.include_duplicates || !flags.is_duplicate)
    }
}

/// A contiguous read sequence mapped to reference coordinates.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlignedBlock {
    pub start: u32,
    pub end: u32,
    pub bases: String,
    /// Per-base flags for BAM `=` sequence symbols, which are known to match
    /// the reference even though their resolved nucleotide is unavailable.
    pub known_matches: Vec<bool>,
}

/// A reference-consuming CIGAR gap such as D or N.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReferenceSpan {
    pub start: u32,
    pub end: u32,
}

/// A query-only CIGAR insertion, anchored immediately before `position`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Insertion {
    pub position: u32,
    pub bases: String,
}

/// A bounded set of alignments suitable for a browser UI, plus the exact match count.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlignmentQueryResult {
    pub total_count: u64,
    pub alignments: Vec<AlignmentSummary>,
    pub truncated: bool,
    /// Per-bin counts of alignments overlapping the queried interval.
    pub density: Vec<u32>,
}

/// Produces fixed-width alignment-density bins for a 0-based half-open region.
///
/// Each bin counts alignments that overlap it, rather than bases covered.  This
/// gives a bounded low-zoom summary while retaining the exact query count.
#[derive(Clone, Debug)]
pub struct AlignmentDensity {
    region: GenomicInterval,
    differences: Vec<i64>,
}

impl AlignmentDensity {
    pub fn new(region: GenomicInterval, bin_count: usize) -> Self {
        Self {
            region,
            differences: vec![0; bin_count.saturating_add(1)],
        }
    }

    pub fn add(&mut self, start: u32, end: u32) {
        if self.differences.len() <= 1 || end <= self.region.start || start >= self.region.end {
            return;
        }
        let width = u64::from(self.region.len());
        if width == 0 {
            return;
        }
        let bins = self.differences.len() - 1;
        let clipped_start = start.max(self.region.start) - self.region.start;
        let clipped_end = end.min(self.region.end) - self.region.start;
        let first = ((u64::from(clipped_start) * bins as u64) / width) as usize;
        let last = (((u64::from(clipped_end - 1) * bins as u64) / width) as usize).min(bins - 1);
        self.differences[first] += 1;
        self.differences[last + 1] -= 1;
    }

    pub fn finish(self) -> Vec<u32> {
        let mut count = 0_i64;
        self.differences[..self.differences.len().saturating_sub(1)]
            .iter()
            .map(|difference| {
                count += difference;
                count.max(0) as u32
            })
            .collect()
    }
}

/// Selects a deterministic reservoir slot for the `seen`th item in a stream.
///
/// This keeps a bounded, reproducible sample across a deep viewport rather than
/// privileging the first records encountered in coordinate order.
pub fn deterministic_reservoir_slot(seen: u64, capacity: usize) -> Option<usize> {
    if capacity == 0 {
        return None;
    }
    if seen < capacity as u64 {
        return Some(seen as usize);
    }
    let mut value = seen.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    let candidate = (value ^ (value >> 31)) % (seen + 1);
    (candidate < capacity as u64).then_some(candidate as usize)
}

/// A 0-based, half-open genomic interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenomicInterval {
    pub start: u32,
    pub end: u32,
}

impl GenomicInterval {
    pub fn new(start: u32, end: u32) -> Option<Self> {
        (start <= end).then_some(Self { start, end })
    }

    pub fn len(self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

#[cfg(test)]
mod tests {
    use super::{
        deterministic_reservoir_slot, AlignmentDensity, AlignmentFilter, AlignmentFlags,
        GenomicInterval,
    };

    #[test]
    fn interval_uses_half_open_coordinates() {
        let interval = GenomicInterval::new(0, 1).expect("valid interval");
        assert_eq!(interval.len(), 1);
        assert!(!interval.is_empty());
        assert_eq!(GenomicInterval::new(5, 4), None);
    }

    #[test]
    fn filter_interprets_sam_flags_in_rust() {
        let flags = AlignmentFlags::from_sam_flags(0x10 | 0x100 | 0x400);
        assert!(flags.is_reverse);
        assert!(flags.is_secondary);
        assert!(flags.is_duplicate);
        assert!(!AlignmentFilter {
            min_mapping_quality: 20,
            include_secondary: false,
            include_supplementary: true,
            include_duplicates: false,
        }
        .matches(60, &flags));
    }

    #[test]
    fn reservoir_is_bounded_and_reproducible() {
        let slots = (0..1_000)
            .filter_map(|seen| deterministic_reservoir_slot(seen, 10))
            .collect::<Vec<_>>();
        assert!(slots.iter().all(|slot| *slot < 10));
        assert_eq!(
            slots,
            (0..1_000)
                .filter_map(|seen| deterministic_reservoir_slot(seen, 10))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn density_counts_overlapping_alignments_in_half_open_bins() {
        let mut density = AlignmentDensity::new(GenomicInterval::new(0, 100).unwrap(), 4);
        density.add(0, 25);
        density.add(24, 76);
        density.add(100, 101);
        assert_eq!(density.finish(), vec![2, 1, 1, 1]);
    }
}
