<script lang="ts">
  import type { AlignmentSummary, BrowserError, ReferenceSequence } from './lib/types'
  import { parseBamHeader, queryBamReference } from './lib/wasm'

  let references: ReferenceSequence[] = []
  let selectedReference = ''
  let filename = ''
  let fileSize = 0
  let state: 'idle' | 'parsing' | 'ready' | 'error' = 'idle'
  let error: BrowserError | null = null
  let fileInput: HTMLInputElement
  let bamBytes: Uint8Array | null = null
  let alignments: AlignmentSummary[] = []
  let alignmentCount = 0
  let alignmentsTruncated = false
  let alignmentState: 'idle' | 'loading' | 'ready' = 'idle'
  let loadGeneration = 0
  let queryGeneration = 0

  async function loadBam(file: File) {
    const generation = ++loadGeneration
    references = []
    alignments = []
    alignmentCount = 0
    alignmentsTruncated = false
    selectedReference = ''
    filename = file.name
    fileSize = file.size
    error = null
    state = 'parsing'
    try {
      const bytes = new Uint8Array(await file.arrayBuffer())
      if (generation !== loadGeneration) return
      const parsedReferences = await parseBamHeader(bytes)
      if (generation !== loadGeneration) return
      bamBytes = bytes
      references = parsedReferences
      selectedReference = references[0]?.name ?? ''
      state = 'ready'
      await loadSelectedReference(generation)
    } catch (caught) {
      if (generation !== loadGeneration) return
      error = { message: caught instanceof Error ? caught.message : String(caught) }
      state = 'error'
    }
  }

  async function loadSelectedReference(expectedLoadGeneration = loadGeneration) {
    const referenceIndex = references.findIndex((reference) => reference.name === selectedReference)
    if (!bamBytes || referenceIndex < 0) return
    const generation = ++queryGeneration
    alignmentState = 'loading'
    error = null
    try {
      const result = await queryBamReference(bamBytes, referenceIndex)
      if (expectedLoadGeneration !== loadGeneration || generation !== queryGeneration) return
      alignments = result.alignments
      alignmentCount = result.total_count
      alignmentsTruncated = result.truncated
      alignmentState = 'ready'
    } catch (caught) {
      if (expectedLoadGeneration !== loadGeneration || generation !== queryGeneration) return
      alignments = []
      error = { message: caught instanceof Error ? caught.message : String(caught) }
      state = 'error'
    }
  }

  function handleFiles(files: FileList | null) {
    const file = files?.[0]
    if (file) void loadBam(file)
  }

  function drop(event: DragEvent) {
    event.preventDefault()
    handleFiles(event.dataTransfer?.files ?? null)
  }
</script>

<svelte:head><title>bamviz — local-first BAM viewer</title></svelte:head>

<header>
  <div><h1>bamviz</h1><p>Local-first BAM alignment viewer</p></div>
  <span>Alignment data stays in your browser</span>
</header>

<main>
  <section class="file-loader" aria-label="BAM file loader" ondragover={(event) => event.preventDefault()} ondrop={drop}>
    <h2>Open a BAM file</h2>
    <p>Drop a coordinate-sorted <code>.bam</code> file here, or choose one from your computer.</p>
    <input bind:this={fileInput} type="file" accept=".bam,application/octet-stream" onchange={(event) => handleFiles(event.currentTarget.files)} />
    <button onclick={() => fileInput.click()}>Choose BAM</button>
    <small>BAM headers and selected-contig alignments are decoded locally. BAI acceleration follows in M2.</small>
  </section>

  {#if state === 'parsing'}<p role="status">Reading the header from {filename}…</p>{/if}
  {#if error}
    <section class="error" role="alert"><h2>Could not load {filename || 'BAM file'}</h2><p>{error.message}</p></section>
  {/if}
  {#if state === 'ready'}
    <section class="file-facts" aria-label="Loaded BAM details">
      <strong>{filename}</strong><span>{fileSize.toLocaleString()} bytes</span><span>{references.length} reference{references.length === 1 ? '' : 's'}</span>
    </section>
    {#if references.length}
      <section class="contigs" aria-label="BAM references">
        <label for="contig">Reference / contig</label>
        <select id="contig" bind:value={selectedReference} onchange={() => void loadSelectedReference()}>
          {#each references as reference}<option value={reference.name}>{reference.name} — {reference.length.toLocaleString()} bp</option>{/each}
        </select>
        {#if selectedReference}
          <p>Selected <strong>{selectedReference}</strong>.</p>
          {#if alignmentState === 'loading'}<p role="status">Scanning alignments…</p>{/if}
          {#if alignmentState === 'ready'}
            <p><strong>{alignmentCount.toLocaleString()}</strong> mapped alignment{alignmentCount === 1 ? '' : 's'} found.</p>
            {#if alignments.length}
              <div class="alignment-list" aria-label="Mapped alignments">
                <div class="alignment-heading"><span>Position (1-based)</span><span>CIGAR</span><span>MAPQ</span><span>Strand</span></div>
                {#each alignments.slice(0, 100) as alignment}
                  <div class="alignment"><span>{(alignment.start + 1).toLocaleString()}–{alignment.end.toLocaleString()}</span><code>{alignment.cigar || '*'}</code><span>{alignment.mapping_quality}</span><span>{alignment.is_reverse ? '−' : '+'}</span></div>
                {/each}
              </div>
              {#if alignmentsTruncated}<small>Showing the first 100 alignments. M2 will provide viewport-based rendering.</small>{/if}
            {/if}
          {/if}
        {/if}
      </section>
    {:else}
      <section class="error" role="status"><h2>No references in this BAM</h2><p>The header is valid but contains no reference sequences.</p></section>
    {/if}
  {/if}
</main>

<style>
  :global(*) { box-sizing: border-box } :global(body) { margin: 0; color: #172433; background: #edf2f6; font-family: system-ui, sans-serif } :global(button), :global(input), :global(select) { font: inherit }
  header { display: flex; justify-content: space-between; align-items: center; gap: 1rem; padding: .8rem max(1rem, calc((100% - 1100px) / 2)); color: #fff; background: #173e51 } h1 { margin: 0 } header p { margin: .1rem 0 } header span { padding: .4rem .7rem; border: 1px solid #8ed7c4; border-radius: 2rem }
  main { display: grid; gap: 1rem; max-width: 1100px; margin: auto; padding: 1rem } section { background: #fff; border: 1px solid #c6d3dc; border-radius: .5rem; padding: 1rem } h2 { margin-top: 0 }
  .file-loader { display: grid; gap: .8rem; border: 2px dashed #53788b; text-align: center } .file-loader input { display: none } button { justify-self: center; padding: .55rem 1rem; color: #fff; background: #176d7d; border: 0; border-radius: .3rem; cursor: pointer } small { color: #536473 }
  .file-facts { display: flex; flex-wrap: wrap; gap: 1rem }.contigs { display: grid; gap: .7rem; max-width: 48rem }.contigs select { padding: .45rem }.error { border-color: #bc4545; color: #702222 }
  .alignment-list { overflow-x: auto; border: 1px solid #d2dde4; border-radius: .25rem; font-variant-numeric: tabular-nums }.alignment, .alignment-heading { display: grid; grid-template-columns: 1.4fr 1fr .7fr .5fr; gap: .7rem; padding: .45rem .6rem; min-width: 29rem }.alignment:nth-child(odd) { background: #f4f8fa }.alignment-heading { color: #fff; background: #315b6d; font-weight: 700 }
  @media (max-width: 700px) { header { align-items: flex-start; flex-direction: column } }
</style>
