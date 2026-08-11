<script lang="ts">
  import { afterUpdate, onMount } from 'svelte'
  import type { AlignmentFilter, AlignmentSummary, BaiIndexSummary, BrowserError, ReferenceSequence } from './lib/types'
  import type { CachedFasta } from './lib/wasm'
  import { fastaReferenceSlice, fastaReferences, loadFasta as loadCachedFasta, parseBaiIndex, parseBamHeader, parseFaiReferences, queryBamRegionFiltered, queryBamRegionIndexedFiltered } from './lib/wasm'

  let references: ReferenceSequence[] = []
  let selectedReference = ''
  let filename = ''
  let fileSize = 0
  let state: 'idle' | 'parsing' | 'ready' | 'error' = 'idle'
  let error: BrowserError | null = null
  let fileInput: HTMLInputElement
  let baiInput: HTMLInputElement
  let bamBytes: Uint8Array | null = null
  let baiIndex: BaiIndexSummary | null = null
  let baiBytes: Uint8Array | null = null
  let baiStatus = ''
  let alignments: AlignmentSummary[] = []
  let alignmentCount = 0
  let alignmentsTruncated = false
  let selectedAlignment: AlignmentSummary | null = null
  let alignmentFilter: AlignmentFilter = { min_mapping_quality: 0, include_secondary: true, include_supplementary: true, include_duplicates: true }
  let alignmentState: 'idle' | 'loading' | 'ready' = 'idle'
  let loadGeneration = 0
  let queryGeneration = 0
  let fastaInput: HTMLInputElement
  let faiInput: HTMLInputElement
  let fasta: CachedFasta | null = null
  let fastaStatus = ''
  let referenceBases = ''
  let referenceBasesStart = 0
  let referenceGeneration = 0
  let canvas: HTMLCanvasElement
  let viewStart = 0
  let viewEnd = 1
  let drag: { x: number; start: number; end: number } | null = null

  $: selectedReferenceData = references.find((reference) => reference.name === selectedReference)

  async function loadBam(file: File) {
    const generation = ++loadGeneration
    references = []
    alignments = []
    selectedAlignment = null
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
      const initialReference = references[0]
      selectedReference = initialReference?.name ?? ''
      resetView(initialReference)
      state = 'ready'
      updateBaiStatus()
      await loadSelectedReference(generation)
    } catch (caught) {
      if (generation !== loadGeneration) return
      error = { message: caught instanceof Error ? caught.message : String(caught) }
      state = 'error'
    }
  }

  function updateBaiStatus() {
    if (!baiIndex) return
    if (!references.length) { baiStatus = `BAI loaded for ${baiIndex.references.length} reference${baiIndex.references.length === 1 ? '' : 's'}; load its BAM to validate it`; return }
    baiStatus = baiIndex.references.length === references.length
      ? `BAI loaded: ${baiIndex.references.length} reference index${baiIndex.references.length === 1 ? '' : 'es'} available for this BAM`
      : `The selected BAI has ${baiIndex.references.length} references but this BAM has ${references.length}; it may not match this BAM`
  }

  async function loadBai(file: File) {
    baiStatus = `Reading ${file.name}…`
    try {
      const bytes = new Uint8Array(await file.arrayBuffer())
      baiIndex = await parseBaiIndex(bytes)
      baiBytes = bytes
      updateBaiStatus()
    } catch (caught) { baiStatus = caught instanceof Error ? caught.message : String(caught) }
  }

  function resetView(reference = selectedReferenceData) {
    if (!reference) return
    viewStart = 0
    viewEnd = Math.max(1, reference.length)
    void refreshReferenceContext(reference)
  }

  function selectReference() {
    resetView(references.find((reference) => reference.name === selectedReference))
    void loadSelectedReference()
  }

  async function loadFasta(file: File) {
    fastaStatus = `Reading ${file.name}…`
    try {
      const bytes = new Uint8Array(await file.arrayBuffer())
      const parsedFasta = await loadCachedFasta(bytes)
      fasta?.free()
      fasta = parsedFasta
      const fastaReferenceList = fastaReferences(fasta)
      fastaStatus = `${file.name}: ${fastaReferenceList.length} reference${fastaReferenceList.length === 1 ? '' : 's'} available`
      await refreshReferenceContext()
    } catch (caught) { fastaStatus = caught instanceof Error ? caught.message : String(caught) }
  }

  async function loadFai(file: File) {
    try { const references = await parseFaiReferences(new Uint8Array(await file.arrayBuffer())); fastaStatus = `${file.name}: index for ${references.length} reference${references.length === 1 ? '' : 's'} loaded${fasta ? '' : ' (FASTA sequence is still required)'}` }
    catch (caught) { fastaStatus = caught instanceof Error ? caught.message : String(caught) }
  }

  async function refreshReferenceContext(reference = selectedReferenceData) {
    if (!fasta || !reference) { referenceBases = ''; return }
    const generation = ++referenceGeneration
    const name = reference.name
    const start = Math.floor(viewStart); const end = Math.ceil(viewEnd)
    try {
      const bases = fastaReferenceSlice(fasta, name, start, end)
      if (generation !== referenceGeneration || name !== selectedReference) return
      referenceBases = bases
      referenceBasesStart = start
    } catch {
      if (generation !== referenceGeneration || name !== selectedReference) return
      referenceBases = ''
      fastaStatus = `No FASTA record matched ${name}`
    }
  }

  function zoomBy(scale: number) {
    if (!selectedReferenceData) return
    const middle = (viewStart + viewEnd) / 2
    const width = Math.min(selectedReferenceData.length, Math.max(1, (viewEnd - viewStart) * scale))
    viewStart = Math.max(0, Math.min(selectedReferenceData.length - width, middle - width / 2))
    viewEnd = viewStart + width
    void refreshViewport()
  }

  function refreshViewport() {
    void refreshReferenceContext()
    void loadSelectedReference()
  }

  function applyFilter() {
    const mapq = Number(alignmentFilter.min_mapping_quality)
    alignmentFilter = {
      ...alignmentFilter,
      min_mapping_quality: Number.isFinite(mapq) ? Math.min(255, Math.max(0, Math.trunc(mapq))) : 0,
    }
    selectedAlignment = null
    void loadSelectedReference()
  }

  function zoomCanvas(event: WheelEvent) {
    event.preventDefault()
    if (!canvas || !selectedReferenceData) return
    const bounds = canvas.getBoundingClientRect()
    const fraction = Math.min(1, Math.max(0, (event.clientX - bounds.left) / bounds.width))
    const focus = viewStart + (viewEnd - viewStart) * fraction
    const scale = event.deltaY < 0 ? 0.7 : 1 / 0.7
    const width = Math.min(selectedReferenceData.length, Math.max(1, (viewEnd - viewStart) * scale))
    viewStart = Math.max(0, Math.min(selectedReferenceData.length - width, focus - width * fraction))
    viewEnd = viewStart + width
    refreshViewport()
  }

  function pointerDown(event: PointerEvent) {
    canvas.setPointerCapture(event.pointerId)
    drag = { x: event.clientX, start: viewStart, end: viewEnd }
  }

  function pointerMove(event: PointerEvent) {
    if (!drag || !canvas || !selectedReferenceData) return
    const delta = (event.clientX - drag.x) / canvas.getBoundingClientRect().width * (drag.end - drag.start)
    const width = drag.end - drag.start
    viewStart = Math.max(0, Math.min(selectedReferenceData.length - width, drag.start - delta))
    viewEnd = viewStart + width
    refreshViewport()
  }

  function pointerUp() { drag = null }

  function drawCanvas() {
    if (!canvas || !selectedReferenceData) return
    const cssWidth = canvas.clientWidth
    const cssHeight = 250
    if (!cssWidth) return
    const ratio = window.devicePixelRatio || 1
    canvas.width = Math.floor(cssWidth * ratio); canvas.height = cssHeight * ratio
    const context = canvas.getContext('2d')!
    context.setTransform(ratio, 0, 0, ratio, 0, 0)
    context.clearRect(0, 0, cssWidth, cssHeight)
    context.fillStyle = '#f7fafc'; context.fillRect(0, 0, cssWidth, cssHeight)
    const basesPerPixel = (viewEnd - viewStart) / cssWidth
    const toX = (position: number) => (position - viewStart) / basesPerPixel
    context.fillStyle = '#315b6d'; context.font = '12px system-ui'
    context.fillText(`${Math.floor(viewStart + 1).toLocaleString()}–${Math.ceil(viewEnd).toLocaleString()} (1-based)`, 8, 16)
    if (referenceBases && basesPerPixel <= 0.15) {
      context.fillStyle = '#172433'
      for (let index = 0; index < referenceBases.length; index++) {
        const position = referenceBasesStart + index
        if (position >= viewStart && position < viewEnd) context.fillText(referenceBases[index], toX(position) + 1, 30)
      }
    }
    const lanes: number[] = []
    for (const alignment of alignments) {
      if (alignment.end <= viewStart || alignment.start >= viewEnd) continue
      let lane = lanes.findIndex((end) => end <= alignment.start)
      if (lane === -1) { lane = lanes.length; lanes.push(alignment.end) } else lanes[lane] = alignment.end
      const y = (referenceBases ? 45 : 30) + lane * 19
      if (y > cssHeight - 10) continue
      const left = toX(Math.max(alignment.start, viewStart)); const right = toX(Math.min(alignment.end, viewEnd))
      context.fillStyle = alignment.flags.is_reverse ? '#6f58a7' : '#176d7d'
      context.fillRect(left, y, Math.max(1, right - left), 13)
      if (basesPerPixel <= 1.5) {
        for (const block of alignment.blocks) {
          for (let index = 0; index < block.bases.length; index++) {
            const position = block.start + index
            if (position < viewStart || position >= viewEnd) continue
            const base = block.bases[index].toUpperCase()
            const colour = ({ A: '#4daf4a', C: '#377eb8', G: '#ffb000', T: '#e34a33' } as Record<string, string>)[base] ?? '#7f8c8d'
            const x = toX(position); const width = Math.max(1, 1 / basesPerPixel)
            context.fillStyle = colour; context.fillRect(x, y, width, 13)
            if (basesPerPixel <= 0.09) { context.fillStyle = '#172433'; context.fillText(base, x + 1, y + 11) }
          }
        }
        context.fillStyle = '#f7fafc'
        for (const deletion of alignment.deletions) {
          const left = toX(Math.max(deletion.start, viewStart)); const right = toX(Math.min(deletion.end, viewEnd))
          context.fillRect(left, y, Math.max(1, right - left), 13)
        }
        context.strokeStyle = '#c33'; context.lineWidth = 1
        for (const deletion of alignment.deletions) {
          const left = toX(Math.max(deletion.start, viewStart)); const right = toX(Math.min(deletion.end, viewEnd))
          context.beginPath(); context.moveTo(left, y + 6.5); context.lineTo(right, y + 6.5); context.stroke()
        }
        context.fillStyle = '#8e44ad'
        for (const insertion of alignment.insertions) { const x = toX(insertion.position); context.fillRect(x - 1, y - 3, 3, 19) }
      }
    }
  }

  onMount(() => { window.addEventListener('resize', drawCanvas); return () => window.removeEventListener('resize', drawCanvas) })
  afterUpdate(drawCanvas)

  async function loadSelectedReference(expectedLoadGeneration = loadGeneration) {
    const referenceIndex = references.findIndex((reference) => reference.name === selectedReference)
    if (!bamBytes || referenceIndex < 0) return
    const generation = ++queryGeneration
    alignmentState = 'loading'
    error = null
    try {
      const result = baiBytes && baiIndex?.references.length === references.length && (baiIndex.references[referenceIndex]?.chunk_count ?? 0) > 0
        ? await queryBamRegionIndexedFiltered(bamBytes, baiBytes, referenceIndex, Math.floor(viewStart), Math.ceil(viewEnd), alignmentFilter)
        : await queryBamRegionFiltered(bamBytes, referenceIndex, Math.floor(viewStart), Math.ceil(viewEnd), alignmentFilter)
      if (expectedLoadGeneration !== loadGeneration || generation !== queryGeneration) return
      const previousSelection = selectedAlignment
      alignments = result.alignments
      selectedAlignment = previousSelection
        ? alignments.find((alignment) => alignment.read_name === previousSelection.read_name && alignment.start === previousSelection.start && alignment.end === previousSelection.end) ?? null
        : null
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

  function loadDroppedFiles(files: FileList | null) {
    for (const file of Array.from(files ?? [])) {
      const name = file.name.toLowerCase()
      if (name.endsWith('.bam')) void loadBam(file)
      else if (name.endsWith('.bai')) void loadBai(file)
      else if (name.endsWith('.fai')) void loadFai(file)
      else if (name.endsWith('.fa') || name.endsWith('.fasta') || name.endsWith('.fna')) void loadFasta(file)
    }
  }

  function drop(event: DragEvent) {
    event.preventDefault()
    loadDroppedFiles(event.dataTransfer?.files ?? null)
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
    <p>Drop local <code>.bam</code>, <code>.bai</code>, <code>.fasta</code>, or <code>.fai</code> files here, or choose them individually.</p>
    <input bind:this={fileInput} type="file" accept=".bam,application/octet-stream" onchange={(event) => handleFiles(event.currentTarget.files)} />
    <button onclick={() => fileInput.click()}>Choose BAM</button>
    <input bind:this={baiInput} type="file" accept=".bai,application/octet-stream" onchange={(event) => event.currentTarget.files?.[0] && void loadBai(event.currentTarget.files[0])} />
    <button onclick={() => baiInput.click()}>Add BAI</button>
    <small>BAM headers and selected-contig alignments are decoded locally. A matching BAI enables indexed BGZF region reads; BAM-only sessions use the sequential fallback.</small>{#if baiStatus}<small role="status">{baiStatus}</small>{/if}
  </section>
  <section class="reference-loader" aria-label="Optional reference files"><h2>Optional reference context</h2><p>Load a FASTA to display reference bases. An FAI is an index only and does not provide sequence.</p><input bind:this={fastaInput} type="file" accept=".fa,.fasta,.fna,text/plain" onchange={(event) => event.currentTarget.files?.[0] && void loadFasta(event.currentTarget.files[0])} /><button onclick={() => fastaInput.click()}>Choose FASTA</button><input bind:this={faiInput} type="file" accept=".fai,text/plain" onchange={(event) => event.currentTarget.files?.[0] && void loadFai(event.currentTarget.files[0])} /><button onclick={() => faiInput.click()}>Choose FAI</button>{#if fastaStatus}<small role="status">{fastaStatus}</small>{/if}</section>

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
        <select id="contig" bind:value={selectedReference} onchange={selectReference}>
          {#each references as reference}<option value={reference.name}>{reference.name} — {reference.length.toLocaleString()} bp</option>{/each}
        </select>
        {#if selectedReference}
          <p>Selected <strong>{selectedReference}</strong>.</p>
          {#if alignmentState === 'loading'}<p role="status">Scanning alignments…</p>{/if}
          {#if alignmentState === 'ready'}
            <p><strong>{alignmentCount.toLocaleString()}</strong> mapped alignment{alignmentCount === 1 ? '' : 's'} found.</p>
            <fieldset class="alignment-filters"><legend>Alignment filters</legend><label>Minimum MAPQ <input type="number" min="0" max="255" bind:value={alignmentFilter.min_mapping_quality} onchange={applyFilter} /></label><label><input type="checkbox" bind:checked={alignmentFilter.include_secondary} onchange={applyFilter} /> Include secondary</label><label><input type="checkbox" bind:checked={alignmentFilter.include_supplementary} onchange={applyFilter} /> Include supplementary</label><label><input type="checkbox" bind:checked={alignmentFilter.include_duplicates} onchange={applyFilter} /> Include duplicates</label></fieldset>
            <nav class="viewer-controls" aria-label="Alignment viewer controls"><button onclick={() => resetView()}>Whole contig</button><button onclick={() => zoomBy(.6)}>Zoom in</button><button onclick={() => zoomBy(1 / .6)}>Zoom out</button><output>{Math.ceil((viewEnd - viewStart) / 700).toLocaleString()} bp/px</output></nav>
            <canvas bind:this={canvas} class="alignment-canvas" aria-label="Alignment viewport" onwheel={zoomCanvas} onpointerdown={pointerDown} onpointermove={pointerMove} onpointerup={pointerUp} onpointercancel={pointerUp}></canvas>
            {#if alignments.length}
              <div class="alignment-list" aria-label="Mapped alignments">
                <div class="alignment-heading"><span>Read / position (1-based)</span><span>CIGAR / clipping</span><span>MAPQ</span><span>Flags</span></div>
                {#each alignments.slice(0, 100) as alignment}
                  <button class:selected={selectedAlignment === alignment} class="alignment" onclick={() => selectedAlignment = alignment}><span><strong>{alignment.read_name}</strong><br />{(alignment.start + 1).toLocaleString()}–{alignment.end.toLocaleString()}</span><span><code>{alignment.cigar || '*'}</code>{#if alignment.left_clip || alignment.right_clip}<br /><small>{alignment.left_clip}′ / {alignment.right_clip}′ clipped</small>{/if}</span><span>{alignment.mapping_quality}</span><span>{alignment.flags.is_reverse ? '−' : '+'}{alignment.flags.is_secondary ? ' secondary' : ''}{alignment.flags.is_supplementary ? ' supplementary' : ''}{alignment.flags.is_duplicate ? ' duplicate' : ''}</span></button>
                {/each}
              </div>
              {#if selectedAlignment}<section class="read-details" aria-label="Selected read details"><h3>{selectedAlignment.read_name}</h3><dl><dt>Position</dt><dd>{(selectedAlignment.start + 1).toLocaleString()}–{selectedAlignment.end.toLocaleString()} (1-based)</dd><dt>CIGAR</dt><dd><code>{selectedAlignment.cigar}</code></dd><dt>Mapping quality</dt><dd>{selectedAlignment.mapping_quality}</dd><dt>Flags</dt><dd>{selectedAlignment.flags.raw} ({selectedAlignment.flags.is_reverse ? 'reverse' : 'forward'} strand{selectedAlignment.flags.is_paired ? ', paired' : ''}{selectedAlignment.flags.is_proper_pair ? ', proper pair' : ''})</dd><dt>Clipping</dt><dd>{selectedAlignment.left_clip}′ left; {selectedAlignment.right_clip}′ right</dd><dt>Mate</dt><dd>{#if selectedAlignment.mate_reference && selectedAlignment.mate_start !== null}{selectedAlignment.mate_reference}:{(selectedAlignment.mate_start + 1).toLocaleString()}{selectedAlignment.flags.mate_is_reverse ? ' (reverse)' : ''}{:else}not available{/if}</dd></dl></section>{/if}
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
  .file-loader, .reference-loader { display: grid; gap: .8rem; border: 2px dashed #53788b; text-align: center } .file-loader input, .reference-loader input { display: none } button { justify-self: center; padding: .55rem 1rem; color: #fff; background: #176d7d; border: 0; border-radius: .3rem; cursor: pointer } small { color: #536473 }
  .file-facts { display: flex; flex-wrap: wrap; gap: 1rem }.contigs { display: grid; gap: .7rem; max-width: 48rem }.contigs select { padding: .45rem }.error { border-color: #bc4545; color: #702222 }
  .alignment-filters { display:flex; flex-wrap:wrap; gap:.7rem; border:1px solid #d2dde4; border-radius:.25rem }.alignment-filters label { display:flex; align-items:center; gap:.25rem }.alignment-filters input[type=number] { width:5rem }.alignment-list { overflow-x: auto; border: 1px solid #d2dde4; border-radius: .25rem; font-variant-numeric: tabular-nums }.alignment, .alignment-heading { display: grid; grid-template-columns: 1.4fr 1fr .7fr .9fr; gap: .7rem; padding: .45rem .6rem; min-width: 29rem }.alignment { width:100%; color:inherit; text-align:left; background:#fff; border:0; border-radius:0 }.alignment:nth-child(odd) { background: #f4f8fa }.alignment.selected { outline:2px solid #176d7d; outline-offset:-2px }.alignment-heading { color: #fff; background: #315b6d; font-weight: 700 }.read-details { margin-top:.7rem; background:#f4f8fa }.read-details h3 { margin-top:0 }.read-details dl { display:grid; grid-template-columns:max-content 1fr; gap:.35rem .8rem; margin:0 }.read-details dt { font-weight:700 }.read-details dd { margin:0 }
  .viewer-controls { display:flex; flex-wrap:wrap; align-items:center; gap:.5rem }.viewer-controls button { justify-self:auto }.viewer-controls output { margin-left:auto; color:#536473 }.alignment-canvas { width:100%; height:250px; touch-action:none; border:1px solid #b8c8d1; border-radius:.3rem; cursor:grab }
  @media (max-width: 700px) { header { align-items: flex-start; flex-direction: column } }
</style>
