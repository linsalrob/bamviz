# bamviz design principles

This document records the architectural and product constraints for **bamviz**.

These principles are intended to prevent the project from drifting into an over-engineered genome browser or a generic bioinformatics framework. bamviz should remain a focused, local-first BAM viewer with excellent interaction and rendering.

## 1. Product definition

bamviz is an **in-browser alignment viewer for BAM files**.

The primary question it should answer is:

> What reads are mapped here, and what do those alignments look like?

The application should make it extremely easy to move from a BAM file to a visual representation of its alignments.

bamviz is not initially:

- an alignment editor;
- an assembler;
- a variant caller;
- a full IGV replacement;
- a genome annotation platform;
- a workflow manager;
- a server-backed genomics application;
- a general-purpose plotting library.

Features that support alignment inspection are welcome. Features that substantially expand the product category require an explicit design decision.

## 2. Local-first and privacy-preserving

User data should remain local.

The normal application architecture is:

```text
local BAM/BAI/FASTA/FAI
        ↓
Browser File API
        ↓
Rust/WASM + TypeScript
        ↓
Canvas viewer
```

There should be no application backend required for normal viewing.

bamviz must not upload BAM, FASTA, read names, sequences, reference names, coordinates, or other user data to a remote service as part of normal operation.

Avoid analytics or telemetry that could expose file-derived information.

A static deployment, including GitHub Pages, should be sufficient to run the application.

## 3. BAM is the primary input

The BAM is the primary object.

The BAM header provides:

- reference names;
- reference lengths;
- alignment metadata.

Therefore a reference FASTA must not be required merely to open and navigate a BAM file.

A BAI should be strongly encouraged because interactive region queries against large BAM files should use indexed access.

Reference FASTA support is an enhancement that supplies actual reference bases and enables richer match/mismatch rendering.

## 4. FAI is not a reference sequence

Do not treat a `.fai` as if it contains reference bases.

An FAI provides indexing metadata for a FASTA. To retrieve actual bases, bamviz requires access to the FASTA itself.

The UI and internal APIs should preserve this distinction.

## 5. Rust owns genomic semantics

Biological and coordinate-sensitive logic belongs in Rust wherever practical.

Examples include:

- genomic interval arithmetic;
- BAM record interpretation;
- CIGAR semantics;
- reference/read coordinate projection;
- aligned-base representation;
- coverage;
- pileups;
- filtering predicates where they involve SAM/BAM semantics;
- read packing algorithms;
- viewport-dependent summarisation/downsampling.

TypeScript should not independently reimplement BAM, CIGAR, SAM flag, or genomic-coordinate semantics.

This prevents two subtly different definitions of the same biological operation from emerging on opposite sides of the WASM boundary.

## 6. File-format code and domain logic are separate

`bamviz-formats` should understand BAM/BAI/FASTA/FAI.

`bamviz-core` should understand bamviz domain objects.

For example:

```text
BAM Record
   ↓
bamviz-formats
   ↓
Alignment
   ↓
bamviz-core
   ↓
WASM DTO
   ↓
browser renderer
```

Do not let low-level BAM parser types leak throughout the application.

This separation makes core algorithms testable using small synthetic domain objects without constructing binary BAM records.

## 7. Keep the WASM boundary coarse and thin

Avoid crossing the Rust/JavaScript boundary once per base or once per read during rendering.

Prefer coarse operations such as:

```text
open_alignment_file(...)
list_references(...)
query_region(reference, start, end, options)
summarise_region(reference, start, end, resolution)
get_reference_sequence(reference, start, end)
```

The exact API may differ, but the principle is important: move useful chunks of data, not chatty streams of tiny calls.

Data transferred across the boundary should be deliberately shaped for rendering.

Do not place core algorithms in the WASM adapter merely because they are called by the browser.

## 8. TypeScript owns the application interface

Svelte/TypeScript should own:

- application state;
- browser file handles;
- drag-and-drop;
- menus and controls;
- selected reference;
- viewport state;
- pointer and keyboard interaction;
- user-visible errors and progress;
- Canvas orchestration.

The web layer may make rendering decisions, but biological interpretation must remain consistent with Rust domain logic.

## 9. Canvas is the default renderer

Use Canvas 2D for the alignment viewport unless profiling demonstrates a real need to change.

Do not begin with:

- a DOM element per base;
- a DOM element per read;
- SVG for thousands of alignment primitives;
- WebGL purely because the application is graphical.

The renderer must be able to redraw efficiently during pan and zoom.

Separate:

1. data preparation;
2. layout;
3. drawing.

This makes performance work measurable and keeps Canvas code understandable.

## 10. Reuse the interaction language of genbank_viewer

bamviz should feel like a sibling of `genbank_viewer`.

Where appropriate, reuse or closely reproduce:

- visual theme;
- typography;
- control placement;
- nucleotide colours;
- cursor-centred zoom;
- click-and-drag panning;
- keyboard navigation;
- coordinate ruler style;
- whole-reference reset behaviour;
- loading and error conventions.

Prefer extracting or adapting proven concepts from `genbank_viewer` over inventing subtly different behaviour.

Do not create a shared package between the repositories prematurely. First establish what code is genuinely reusable.

## 11. Rendering is resolution-dependent

The viewer must not attempt to render every biological detail at every scale.

Define explicit level-of-detail behaviour based on bases per pixel or pixels per base.

A conceptual model is:

### Density level

When many bases map to one horizontal pixel:

- display coverage/density or other summaries;
- avoid drawing bases;
- avoid drawing every read if doing so produces meaningless overplotting.

### Read level

When reads can be visually distinguished:

- draw packed read rectangles/tracks;
- colour aligned bases where useful;
- omit nucleotide glyphs if they do not fit.

### Base level

When individual bases have sufficient screen width:

- draw nucleotide letters;
- display CIGAR effects;
- show the reference if available;
- emphasise mismatches and indels.

Thresholds should be empirical and easy to tune.

## 12. Coordinates must be explicit

Genomics has persistent off-by-one hazards.

Internally, define and document one canonical interval convention, preferably 0-based half-open intervals:

```text
[start, end)
```

Convert to human-friendly coordinates only at presentation boundaries.

Any API using a different convention must be explicit about it.

Tests should cover:

- first base;
- last base;
- one-base intervals;
- zero-length edge cases where valid;
- reads at contig boundaries;
- CIGAR operations around queried interval boundaries.

## 13. CIGAR correctness precedes visual sophistication

The viewer must correctly distinguish at least:

- `M`
- `=`
- `X`
- `I`
- `D`
- `N`
- `S`
- `H`
- `P`

before claiming complete alignment rendering.

Not all operations need elaborate visual treatment in the first version, but their coordinate effects must be correct.

Do not infer a mismatch solely from `M`; whether a base matches the reference requires reference sequence or appropriate alignment metadata.

## 14. Missing reference sequence is a normal state

A BAM-only session is not an error.

Without FASTA, bamviz can still display:

- mapping coordinates;
- read sequence;
- coverage;
- CIGAR structure;
- mapping quality;
- strand;
- clipping;
- BAM metadata.

Features requiring actual reference bases should be disabled or clearly labelled as unavailable.

The application must not fabricate reference bases.

## 15. Large files are a first-class design constraint

Do not design around loading an entire multi-gigabyte BAM into JavaScript objects.

Prefer:

- indexed interval queries;
- bounded working sets;
- lazy decoding;
- viewport-aware retrieval;
- reusable buffers where practical;
- downsampling at high depth;
- caches with explicit limits.

Performance decisions should be based on profiling with representative files.

Avoid premature micro-optimisation, but do not choose an architecture that fundamentally requires the entire BAM to be expanded in memory.

## 16. The viewer must remain responsive

Expensive parsing, summarisation, or layout must not make pointer interaction feel broken.

As the implementation matures, consider:

- Web Workers;
- WASM work off the main UI path;
- cancellation of obsolete region requests;
- debounced loading during rapid interaction;
- caching adjacent regions.

These are architectural options, not requirements for the first end-to-end milestone.

Implement them when measurements or observed UX justify them.

## 17. Progressive functionality beats speculative architecture

Build vertical slices.

The preferred sequence is:

```text
load BAM
→ list contigs
→ select contig
→ obtain alignments
→ draw crude reads
→ pan/zoom
→ colour bases
→ render CIGAR correctly
→ add reference
→ optimise
```

Do not spend substantial time designing abstractions for features that do not yet exist.

A crude but correct visible alignment is more valuable early in the project than a comprehensive framework with no viewer.

## 18. Test biological semantics independently of rendering

Rust unit tests should cover:

- interval arithmetic;
- BAM-to-domain conversion;
- CIGAR projection;
- insertions/deletions/clipping;
- coverage calculations;
- downsampling invariants;
- packing/layout invariants where possible.

Frontend tests should cover:

- file-selection workflow;
- contig selection;
- zoom/pan interactions;
- rendering smoke tests;
- user-visible error handling.

Use small synthetic BAM fixtures with known expected alignments.

Add representative real-world fixtures only when licensing/privacy and repository size permit.

## 19. Test data must be safe to publish

Never commit clinical, sensitive, unpublished, or personally identifying sequencing data.

Prefer synthetic fixtures specifically constructed to exercise edge cases.

Test fixtures should be small enough to keep repository clones and CI fast.

## 20. Error messages are part of the product

Errors should tell the user:

1. what failed;
2. which file was involved;
3. whether the file appears invalid, unsupported, mismatched, or missing;
4. what they can do next.

Examples of conditions to handle gracefully include:

- invalid BAM;
- truncated BAM;
- missing or incompatible BAI;
- unsorted BAM where indexed behaviour requires sorting;
- FASTA/FAI mismatch;
- reference names that differ between BAM and FASTA;
- unsupported browser APIs;
- memory pressure.

Do not expose raw Rust panics as the normal error interface.

## 21. Avoid unnecessary dependencies

Prefer small, well-maintained dependencies that work cleanly under `wasm32-unknown-unknown`.

Before adding a dependency, ask:

- does the standard library already solve this?
- is this needed in production or only development?
- does it build cleanly for WASM?
- does it substantially simplify correctness?
- what transitive cost does it introduce?

Native C/C++ dependencies should be avoided unless there is a compelling, documented reason.

## 22. Accessibility and browser ergonomics matter

The Canvas view may be graphical, but controls should use semantic HTML.

Support, where practical:

- keyboard navigation;
- visible focus states;
- sufficient contrast;
- readable labels;
- high-DPI displays;
- browser resizing;
- trackpads and mouse wheels.

Do not make essential actions available only through an undiscoverable gesture.

## 23. Scientific correctness over decorative complexity

When faced with a choice between a beautiful but ambiguous rendering and a simpler scientifically interpretable rendering, prefer the latter.

Visual encodings should have stable meanings.

Colour should not be the sole carrier of critical information where a practical alternative exists.

Document any downsampling or aggregation that changes what the user sees.

## 24. Initial success criterion

The first important product milestone is:

> A user can drop a real BAM file into the browser, choose a contig from the BAM header, and see reads drawn at the correct genomic positions.

The initial rectangles may be visually crude.

They must be positioned correctly.

Everything else is incremental.
