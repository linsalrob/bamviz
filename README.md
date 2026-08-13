# bamviz

**bamviz** is a local-first, in-browser viewer for read alignments stored in BAM files.

The goal is a fast, simple genome alignment viewer that runs entirely in the browser: drop in a BAM file, select a contig, and inspect the reads mapped to that reference. At low zoom, aligned bases are represented primarily by colour; at high zoom, individual bases become visible.

bamviz is intended to share the visual language and interaction model of [`genbank_viewer`](https://github.com/linsalrob/genbank_viewer), including its general layout, colour scheme, cursor-centred zooming, panning behaviour, and local-first architecture.

## Current status

M0 is implemented: the repository contains a Rust workspace, a thin WASM adapter,
a Svelte/Vite application, automated checks, and GitHub Pages deployment workflow.

M1 is implemented. The browser accepts a local BAM file, reads its
BGZF-compressed header through Rust/WASM, lists its references, and reports
compact summaries of mapped alignments for the selected contig. This is a
sequential, unindexed fallback intended for modest files; BAI-backed,
viewport-bounded queries and Canvas read rendering begin in M2.

M2–M4 are implemented as an initial interactive viewer: selected-contig
alignments render on Canvas with whole-contig reset, cursor-centred zoom and
drag panning. Rust projects CIGAR effects into reference-aligned base blocks,
deletions, and insertions for base-scale rendering. Optional FASTA loading
adds visible reference bases; optional FAI loading is recognised as index
metadata only and never treated as sequence data.

M5 alignment inspection is implemented: the viewer exposes read names, SAM
flags, strand, clipping, mate coordinates, and mapping quality. Users can
filter the visible-region query by mapping quality and secondary,
supplementary, or duplicate status, then select a read for its details.

M6 performance and polish is implemented: pan and zoom updates are coalesced
to one viewport query per animation frame, preventing high-frequency pointer
events from issuing redundant local BAM/BAI reads.

The focused alignment viewport also supports keyboard navigation: left/right
arrows pan, `+`/`−` zoom, and Home resets the whole contig.

Deep viewports retain a deterministic bounded sample of alignments instead of
only the first records in coordinate order, while continuing to report the
exact matching alignment count.

At low zoom, bamviz now renders a bounded, Rust-computed alignment-density
histogram for the current region. This represents all matching alignments,
rather than only the sampled detail rows; zooming in restores individual reads.

BAI files can now be dropped onto the loader or selected with **Add BAI**. The
browser parses their local binning and linear-index summaries, checks their
reference count against the BAM, and uses matching BAI chunks for indexed BGZF
region reads. BAM-only sessions retain the safe sequential fallback.

## Performance and browser support

bamviz keeps detailed viewport results bounded to 100 deterministic read
summaries and 256 density bins. The normal large-file workflow is a
coordinate-sorted BAM with its matching BAI: indexed BGZF queries then decode
only the chunks selected for the current viewport. BAM-only viewing remains a
safe sequential fallback, but it must scan the file for each region and is not
the recommended workflow for large files.

The automated validation suite includes a 10,000-alignment synthetic region to
verify that detailed results stay bounded while the total count and density
summary cover every matching alignment. Pan and zoom updates are coalesced to
one request per animation frame, and Canvas backing buffers are resized only
when the viewport dimensions change.

Use a current, evergreen browser with WebAssembly, the File API, Canvas 2D,
and a pointer device. Keyboard navigation works when the alignment viewport is
focused: left/right pan, `+`/`−` zoom, and Home resets the contig. No browser
or device-specific performance guarantee is made; file size, read depth,
indexing, and available memory all affect responsiveness.

Drag the two-line grip on the alignment panel's right edge to make the complete
viewer—including its background, controls, and read details—narrower or wider.
The grip is also keyboard accessible with the left and right arrow keys.

## Project goals

The core user workflow should be:

1. Open bamviz in a modern web browser.
2. Drop or select a `.bam` file.
3. Optionally provide the corresponding BAM index (`.bai`).
4. Select a reference/contig from the BAM header.
5. Pan and zoom through the reference.
6. Inspect mapped reads and their alignment to the reference.
7. Optionally provide a reference FASTA (and `.fai`) to display the true reference sequence and identify matches and mismatches.

No server should be required. Sequence and alignment files should remain on the user's machine and be processed locally in the browser.

## Intended input files

### Required

- coordinate-sorted `.bam`

The BAM header provides the reference/contig names and lengths, so a reference FASTA is not required for the initial viewer.

### Strongly recommended

- `.bam.bai` or corresponding `.bai`

A BAM index enables efficient interval-based access to large coordinate-sorted BAM files. bamviz should remain usable without an index where practical, but indexed access is the normal path for interactive browsing.

### Optional reference context

- `.fasta`, `.fa`, `.fna`, or equivalent reference FASTA
- corresponding `.fai`

The `.fai` is an index into the FASTA; it does not contain the reference sequence itself. If bamviz needs to display the reference bases, the FASTA must also be available.

## Visual model

bamviz should use level-of-detail rendering rather than drawing the same information at every zoom level.

### Whole-contig / very low zoom

Emphasise:

- coverage or alignment density;
- broad mapping structure;
- gaps in coverage;
- major pileups or unusual regions.

Individual bases do not need to be rendered.

### Read-level zoom

Show individual aligned reads as tracks.

Aligned sequence should primarily be represented by nucleotide colour:

- A
- C
- G
- T
- N/other

Individual letters are omitted when there is insufficient horizontal space.

### Base-level zoom

Show:

- reference bases, when a FASTA is available;
- individual read bases;
- mismatches;
- insertions;
- deletions;
- clipping;
- strand and other useful alignment information.

The transition between levels of detail should be smooth and based on viewport scale, not on separate viewer modes that the user must manually select.

When a matching FASTA is loaded, **Highlight differences** is available beside
the alignment filters. It renders reference-matching read bases in grey and
uses the nucleotide palette only for bases that differ from the reference.

## Proposed architecture

bamviz should initially follow the architecture of `genbank_viewer`:

```text
bamviz/
├── .github/
│   ├── workflows/
│   └── ISSUE_TEMPLATE/
├── crates/
│   ├── bamviz-core/
│   ├── bamviz-formats/
│   └── bamviz-wasm/
├── web/
│   ├── src/
│   └── tests/
├── test-data/
├── docs/
├── AGENTS.md
├── DESIGN.md
├── Cargo.toml
├── Cargo.lock
├── Makefile
├── README.md
├── LICENSE
└── .gitignore
```

### `crates/bamviz-core`

Pure Rust domain logic independent of the browser.

Likely responsibilities include:

- genomic coordinates and intervals;
- alignment models;
- CIGAR interpretation;
- aligned-base projection;
- read layout;
- coverage and pileups;
- viewport-aware level-of-detail decisions;
- downsampling;
- reference/read comparison;
- testable algorithms used by the viewer.

This crate should not depend on Svelte, JavaScript, Canvas, browser APIs, or UI state.

### `crates/bamviz-formats`

Rust code concerned with biological file formats and indexed access.

Likely responsibilities include:

- BAM header parsing;
- BAM record decoding;
- BAI handling and interval queries;
- FASTA handling;
- FAI handling;
- conversion from file-format records into bamviz domain models.

Prefer a pure-Rust implementation suitable for WebAssembly rather than native libraries that introduce C/C++ build dependencies into the browser target.

### `crates/bamviz-wasm`

A deliberately thin WebAssembly boundary.

Its job is to expose coarse, serialisable operations from Rust to the browser. It should not contain biological algorithms that belong in `bamviz-core`, and it should not grow into an alternative application layer.

### `web`

Svelte + TypeScript + Vite application.

Responsibilities include:

- file selection and drag-and-drop;
- application state;
- contig selector and controls;
- viewport interaction;
- Canvas rendering;
- loading/error/progress UI;
- keyboard and pointer interactions;
- integration with Rust/WASM.

Canvas 2D should be the default rendering technology unless profiling demonstrates a need for something more complex.

## Initial milestones

GitHub Milestones should be used as the initial roadmap. Issues should represent implementation-sized pieces of work, usually small enough to become one pull request.

A GitHub Project is not required initially.

### M0 — Project skeleton

A compiling, tested Rust/WASM/Svelte application with CI and GitHub Pages deployment.

### M1 — Load a BAM

A user can:

- drop a BAM file;
- parse its header;
- see the reference/contig list;
- select a contig;
- access alignments for that contig;
- receive useful errors for invalid or unsupported input.

### M2 — Alignment viewer

A user can:

- see mapped reads positioned against genomic coordinates;
- pan horizontally;
- zoom smoothly;
- reset to the whole contig;
- navigate without reloading the file.

### M3 — Base-resolution viewer

Add:

- nucleotide colour rendering;
- individual base letters at sufficient zoom;
- CIGAR-aware alignment;
- insertions;
- deletions;
- clipping;
- strand-aware representation.

### M4 — Reference context

Add optional:

- FASTA loading;
- FAI loading/indexing;
- reference sequence display;
- match/mismatch identification.

### M5 — Useful BAM browser

Add high-value alignment metadata and summaries, such as:

- coverage;
- mapping quality;
- flags;
- strand;
- clipping;
- read details;
- mate information where useful;
- optional filtering.

### M6 — Performance and polish

Focus on:

- large BAM files;
- indexed interval access;
- bounded memory use;
- viewport-based data loading;
- downsampling;
- rendering performance;
- browser compatibility;
- usability and documentation.

### v1.0

A documented, tested, deployable BAM viewer suitable for routine use.

## First development target

The first meaningful end-to-end target is intentionally modest:

> A browser page where the user drops a real BAM file, selects a contig from the BAM header, and sees crude rectangles representing reads mapped to the correct genomic coordinates.

Do not block this milestone on final colours, pileups, reference FASTA support, read-detail panels, sophisticated packing, or perfect performance.

Once the end-to-end path works, build the richer viewer iteratively.

## Development principles

See [`DESIGN.md`](DESIGN.md) for architectural and product principles and [`AGENTS.md`](AGENTS.md) for instructions to coding agents.

## Status

bamviz is an early-stage project. File-format support, browser compatibility, performance limits, and rendering semantics should be treated as evolving until validated with representative BAM files.

## Licence

MIT, unless changed before the first public release.
