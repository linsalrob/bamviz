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
    pub start: u32,
    pub end: u32,
    pub mapping_quality: u8,
    pub is_reverse: bool,
    pub cigar: String,
}

/// A bounded set of alignments suitable for a browser UI, plus the exact match count.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AlignmentQueryResult {
    pub total_count: u64,
    pub alignments: Vec<AlignmentSummary>,
    pub truncated: bool,
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
    use super::GenomicInterval;

    #[test]
    fn interval_uses_half_open_coordinates() {
        let interval = GenomicInterval::new(0, 1).expect("valid interval");
        assert_eq!(interval.len(), 1);
        assert!(!interval.is_empty());
        assert_eq!(GenomicInterval::new(5, 4), None);
    }
}
