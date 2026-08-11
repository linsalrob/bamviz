//! Coarse serialisable APIs consumed by the browser application.

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn parse_bam_header_json(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let references = bamviz_formats::parse_bam_header(bytes)
        .map_err(|error| JsValue::from_str(&format!("Could not read BAM: {error}")))?;
    serde_wasm_bindgen::to_value(&references)
        .map_err(|error| JsValue::from_str(&format!("Could not serialise BAM references: {error}")))
}

#[wasm_bindgen]
pub fn query_bam_reference_json(bytes: &[u8], reference_index: usize) -> Result<JsValue, JsValue> {
    let query = bamviz_formats::query_bam_reference(bytes, reference_index)
        .map_err(|error| JsValue::from_str(&format!("Could not read BAM alignments: {error}")))?;
    serde_wasm_bindgen::to_value(&query)
        .map_err(|error| JsValue::from_str(&format!("Could not serialise BAM alignments: {error}")))
}

#[wasm_bindgen]
pub fn query_bam_region_json(
    bytes: &[u8],
    reference_index: usize,
    start: u32,
    end: u32,
) -> Result<JsValue, JsValue> {
    let query = bamviz_formats::query_bam_region(bytes, reference_index, start, end)
        .map_err(|error| JsValue::from_str(&format!("Could not read BAM alignments: {error}")))?;
    serde_wasm_bindgen::to_value(&query)
        .map_err(|error| JsValue::from_str(&format!("Could not serialise BAM alignments: {error}")))
}

/// Parsed local FASTA retained in WASM so viewport updates do not reparse the file.
#[wasm_bindgen]
pub struct FastaFile {
    records: bamviz_formats::FastaRecords,
}

#[wasm_bindgen]
impl FastaFile {
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Result<FastaFile, JsValue> {
        let records = bamviz_formats::FastaRecords::parse(bytes)
            .map_err(|error| JsValue::from_str(&format!("Could not read FASTA: {error}")))?;
        Ok(Self { records })
    }

    pub fn references_json(&self) -> Result<JsValue, JsValue> {
        serde_wasm_bindgen::to_value(&self.records.references())
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }

    pub fn reference_slice_json(
        &self,
        name: String,
        start: u32,
        end: u32,
    ) -> Result<JsValue, JsValue> {
        let sequence = self
            .records
            .reference_slice(&name, start, end)
            .map_err(|error| JsValue::from_str(&format!("Could not read FASTA: {error}")))?;
        serde_wasm_bindgen::to_value(&sequence)
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }
}

#[wasm_bindgen]
pub fn parse_fasta_references_json(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let references = bamviz_formats::FastaRecords::parse(bytes)
        .map_err(|error| JsValue::from_str(&format!("Could not read FASTA: {error}")))?;
    serde_wasm_bindgen::to_value(&references.references())
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn parse_fai_references_json(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let references = bamviz_formats::parse_fai_references(bytes)
        .map_err(|error| JsValue::from_str(&format!("Could not read FAI: {error}")))?;
    serde_wasm_bindgen::to_value(&references).map_err(|error| JsValue::from_str(&error.to_string()))
}
