# AGENTS.md — instructions for coding agents working on bamviz

## Purpose

You are working on **bamviz**, a local-first in-browser BAM alignment viewer.

The product goal is more important than producing code for its own sake.

The primary user workflow is:

```text
drop BAM
→ select reference/contig
→ view mapped reads
→ pan and zoom
→ inspect alignment at base resolution
```

Read [`README.md`](README.md) and [`DESIGN.md`](DESIGN.md) before making architectural changes.

When those documents conflict with assumptions in existing code, treat the documents as the intended direction unless a task or issue explicitly changes that direction.

## Immediate objective

Until the basic viewer exists, prioritise the shortest correct path to this end-to-end result:

> A browser page where a user can drop a real BAM file, select a contig from the BAM header, and see simple rectangles representing reads mapped to the correct genomic coordinates.

Do **not** block this objective on:

- final styling;
- sophisticated read packing;
- complete reference FASTA support;
- pileup views;
- variant calling;
- advanced filters;
- WebGL;
- complex caching;
- broad framework abstractions.

After this vertical slice works, improve the viewer incrementally.

## Relationship to genbank_viewer

bamviz should feel like a sibling of:

<https://github.com/linsalrob/genbank_viewer>

Where useful, inspect `genbank_viewer` for established patterns involving:

- repository layout;
- Rust workspace organisation;
- Rust/WASM boundaries;
- Svelte/TypeScript/Vite configuration;
- Canvas rendering;
- viewport transforms;
- cursor-centred zoom;
- panning;
- nucleotide colours;
- typography;
- layout;
- GitHub Pages deployment;
- tests and CI.

Reuse concepts and proven implementations where sensible.

Do not blindly copy code. Adapt it to BAM alignment semantics.

Do not introduce a shared cross-repository library unless explicitly requested. Duplication is acceptable while the reusable boundary is still uncertain.

## Architectural boundaries

The intended architecture is:

```text
crates/bamviz-core
    pure Rust domain models and algorithms

crates/bamviz-formats
    BAM/BAI/FASTA/FAI parsing and indexed access

crates/bamviz-wasm
    thin serialisable WASM adapters

web/
    Svelte + TypeScript + browser APIs + Canvas rendering
```

Keep these boundaries strong.

### `bamviz-core`

Put genomic semantics and testable algorithms here.

Examples:

- genomic intervals;
- alignment models;
- CIGAR interpretation;
- reference/read coordinate mapping;
- coverage;
- pileups;
- read layout;
- downsampling;
- level-of-detail decisions.

This crate must not depend on browser APIs or Svelte.

### `bamviz-formats`

Put file-format interpretation here.

Examples:

- BAM header parsing;
- BAM record decoding;
- BAI queries;
- FASTA;
- FAI;
- conversion from format-specific records into domain objects.

Prefer pure-Rust libraries that compile cleanly to WebAssembly.

Avoid introducing `htslib` or other native dependencies unless there is a compelling reason and the WASM consequences have been demonstrated.

### `bamviz-wasm`

Keep this thin.

Expose coarse operations and serialisable DTOs.

Do not put substantial biological logic here.

Avoid very chatty Rust↔JavaScript calls, especially one call per read or per base.

### `web`

Use the web application for:

- browser file access;
- drag-and-drop;
- Svelte application state;
- controls;
- viewport interaction;
- Canvas drawing;
- user-facing progress and errors.

Do not reimplement CIGAR or genomic coordinate semantics independently in TypeScript.

## Technical direction

Unless an issue explicitly changes it, use:

- Rust workspace;
- `wasm32-unknown-unknown`;
- `wasm-bindgen` / `wasm-pack`-style integration;
- Svelte;
- TypeScript;
- Vite;
- Canvas 2D;
- Vitest for frontend/unit tests where appropriate;
- Playwright for browser smoke/end-to-end tests.

Match the current `genbank_viewer` toolchain where practical rather than choosing different frameworks without a product reason.

Do not pin arbitrary old versions merely to match historical code. Use versions compatible with the current repository/toolchain and record meaningful compatibility constraints.

## File inputs

Treat these states distinctly.

### BAM only

Valid.

The application should be able to:

- parse the header;
- list references;
- inspect reads;
- render mapping positions and read sequence where available.

### BAM + BAI

Preferred normal interactive state.

Use indexed interval access for efficient navigation.

### BAM + FASTA

Valid reference-enhanced state.

Reference sequence can be displayed if the BAM reference names and FASTA records can be reconciled.

### BAM + FASTA + FAI

Preferred reference-enhanced state for large references.

### FAI without FASTA

The FAI does not contain reference sequence.

Do not fabricate or imply that reference bases are available.

## Coordinate conventions

Use one documented internal convention consistently.

Prefer:

```text
0-based, half-open: [start, end)
```

Convert coordinates for human display at the UI boundary.

When interacting with libraries or formats that use a different convention, make the conversion explicit and test it.

Off-by-one errors are scientific correctness bugs, not cosmetic bugs.

## BAM and CIGAR correctness

Handle BAM/SAM semantics deliberately.

CIGAR operations may include:

- `M`
- `I`
- `D`
- `N`
- `S`
- `H`
- `P`
- `=`
- `X`

Do not assume:

- every BAM has a reference FASTA;
- `M` means a confirmed reference match;
- query length equals reference span;
- every alignment is primary;
- every BAM is indexed;
- every BAM is coordinate-sorted;
- reference names in BAM and FASTA necessarily match.

Add tests whenever coordinate or CIGAR behaviour changes.

## Rendering rules

The viewport must use level-of-detail rendering.

### Very low zoom

Prefer summary information such as coverage/density.

Do not attempt to paint readable individual bases.

### Read-level zoom

Draw reads and nucleotide colour blocks.

Do not draw nucleotide glyphs when they cannot fit.

### Base-level zoom

Draw individual bases and detailed alignment structure.

If reference sequence is available, show it and allow mismatches to become visually obvious.

Thresholds should be configurable constants rather than scattered magic numbers.

## Performance rules

Large BAM files are normal inputs.

Avoid architectures that require:

- decoding the complete BAM up front;
- retaining every alignment as a JavaScript object;
- redrawing the entire contig on every pointer event;
- crossing the WASM boundary per base.

Prefer:

- region-based access;
- bounded data windows;
- viewport-aware querying;
- downsampling at extreme depth;
- measured caching;
- cancellation/ignoring of stale region requests where needed.

Do not prematurely optimise before the first end-to-end viewer works, but preserve the ability to scale.

When addressing performance, measure before and after.

## UI and visual consistency

Use `genbank_viewer` as the visual reference.

Prefer matching its:

- page structure;
- theme;
- spacing;
- controls;
- zoom behaviour;
- coordinate ruler;
- nucleotide colour palette;
- interaction conventions.

Do not redesign the entire visual language unless explicitly requested.

## Privacy and networking

bamviz is local-first.

Do not add:

- upload endpoints;
- server-side BAM processing;
- cloud storage;
- telemetry containing file-derived data;
- third-party sequence APIs in the normal viewing path;

without explicit approval.

GitHub Pages/static hosting should remain possible.

## Testing expectations

Every meaningful implementation should include the smallest useful tests.

### Rust tests

Prioritise:

- interval boundaries;
- CIGAR projection;
- clipping;
- insertions/deletions;
- record-to-domain conversion;
- coverage calculations;
- level-of-detail calculations;
- downsampling invariants.

### Frontend tests

Prioritise:

- app loads;
- file chooser/drop path works;
- contigs populate after BAM load;
- contig selection updates the viewer;
- zoom/pan does not crash;
- useful errors are shown.

### Test fixtures

Use small synthetic, redistributable data.

Never add clinical, confidential, unpublished, or personally identifying sequencing data.

Where a new bug requires a fixture, make the smallest fixture that reproduces it.

## Build and validation

As the repository is established, provide a small number of top-level validation commands, ideally through `Makefile` targets.

The eventual equivalent of:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets
cargo test --workspace

cd web
npm ci
npm run check
npm run test
npm run build
npm run test:e2e
```

should be straightforward.

Do not claim success unless the relevant checks were actually run.

If an environment prevents a check, state exactly which check was not run and why.

## Dependency policy

Before adding a production dependency, check:

1. Is it necessary?
2. Does it support the WASM target?
3. Does it improve correctness or materially reduce complexity?
4. Is it maintained?
5. What transitive dependencies does it introduce?

Prefer pure-Rust BAM/BAI handling suitable for WASM.

Do not add large UI frameworks, state-management frameworks, rendering frameworks, or generic bioinformatics frameworks without evidence that the existing stack is insufficient.

## Scope control

Do not opportunistically refactor unrelated code while implementing an issue.

Small adjacent cleanup is acceptable when it directly simplifies or makes the requested change safer.

Large refactors should be separate work.

Do not implement speculative features because they seem useful.

If you notice a worthwhile future feature, document it as a suggestion or issue rather than silently expanding scope.

## Git and issue discipline

GitHub Milestones define broad development stages.

Issues should be implementation-sized work units.

Prefer:

```text
one focused issue
→ one focused branch
→ one focused pull request
```

Commit messages should describe the change, not the coding process.

Keep generated build output out of git unless deployment explicitly requires it.

Do not commit secrets, local paths, large BAM files, or private data.

## Documentation discipline

Update documentation when behaviour, architecture, supported inputs, or important limitations change.

Do not write extensive speculative documentation for features that do not exist.

README statements should describe implemented behaviour clearly and distinguish planned features.

`DESIGN.md` describes intended architectural constraints and can be forward-looking.

## Error handling

User-facing errors should be actionable.

Prefer errors such as:

```text
Could not read sample.bam: the BAM header is truncated.
```

or:

```text
The selected BAI does not appear to match this BAM. You can continue without
the index, but region navigation may be slower.
```

over:

```text
parse error
```

Do not expose Rust panics as expected control flow.

Use structured errors across internal boundaries where practical.

## Decision hierarchy

When choosing between implementation approaches, use this priority:

1. scientific/coordinate correctness;
2. user-visible product behaviour;
3. privacy/local-first operation;
4. maintainability;
5. performance supported by measurement;
6. elegance/abstraction.

A simpler implementation that correctly advances the viewer is usually preferable to a sophisticated abstraction built for hypothetical future requirements.

## Before starting a task

1. Read the relevant issue/task.
2. Read `README.md` and `DESIGN.md` if architectural context matters.
3. Inspect nearby code before proposing a new abstraction.
4. Inspect `genbank_viewer` when matching interaction, styling, or repository patterns.
5. Identify the smallest end-to-end change that satisfies the task.
6. Preserve existing behaviour unless the task intentionally changes it.

## Before finishing a task

1. Run relevant formatting checks.
2. Run relevant tests.
3. Run type/build checks for changed components.
4. Add or update tests for corrected semantics.
5. Remove debug logging and temporary files.
6. Check that no sensitive test data has been introduced.
7. Summarise what changed and any genuine limitations.
8. Do not claim tests passed if they were not executed.

## Avoid these failure modes

Do not:

- build a backend for a browser-local problem;
- require FASTA merely to list BAM contigs;
- interpret `.fai` as sequence data;
- reimplement CIGAR independently in TypeScript;
- put all logic into one WASM crate;
- make a DOM node for every base/read;
- load a huge BAM fully into JS memory by default;
- optimise everything before a crude viewer exists;
- redesign the UI away from `genbank_viewer` without instruction;
- turn bamviz into a generic genome browser prematurely;
- prioritise code volume over a working scientific viewer.

## Definition of early success

Early development is successful when:

- the app builds reproducibly;
- a real BAM can be selected locally;
- references are populated from its header;
- one reference can be selected;
- mapped reads appear at correct coordinates;
- pan and zoom work;
- tests protect the core coordinate semantics.

Once that works, proceed toward base-resolution and reference-aware rendering.
