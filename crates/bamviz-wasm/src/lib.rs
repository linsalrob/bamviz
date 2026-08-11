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
pub fn parse_fasta_references_json(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let references = bamviz_formats::parse_fasta_references(bytes)
        .map_err(|error| JsValue::from_str(&format!("Could not read FASTA: {error}")))?;
    serde_wasm_bindgen::to_value(&references).map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn fasta_reference_slice_json(
    bytes: &[u8],
    name: String,
    start: u32,
    end: u32,
) -> Result<JsValue, JsValue> {
    let sequence = bamviz_formats::fasta_reference_slice(bytes, &name, start, end)
        .map_err(|error| JsValue::from_str(&format!("Could not read FASTA: {error}")))?;
    serde_wasm_bindgen::to_value(&sequence).map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn parse_fai_references_json(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let references = bamviz_formats::parse_fai_references(bytes)
        .map_err(|error| JsValue::from_str(&format!("Could not read FAI: {error}")))?;
    serde_wasm_bindgen::to_value(&references).map_err(|error| JsValue::from_str(&error.to_string()))
}
