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
  let unsupportedBrowser = false
  let fileInput: HTMLInputElement
  let baiInput: HTMLInputElement
  let bamBytes: Uint8Array | null = null
  let baiIndex: BaiIndexSummary | null = null
  let baiBytes: Uint8Array | null = null
  let baiFilename = ''
  let baiStatus = ''
  let alignments: AlignmentSummary[] = []
  let alignmentDensity: number[] = []
  let alignmentCount = 0
  let alignmentsTruncated = false
  let selectedAlignment: AlignmentSummary | null = null
  let alignmentFilter: AlignmentFilter = { min_mapping_quality: 0, include_secondary: true, include_supplementary: true, include_duplicates: true }
  let highlightDifferences = false
  let alignmentState: 'idle' | 'loading' | 'ready' = 'idle'
  let alignmentReady = false
  let loadGeneration = 0
  let queryGeneration = 0
  let fastaInput: HTMLInputElement
  let faiInput: HTMLInputElement
  let fasta: CachedFasta | null = null
  let fastaFilename = ''
  let faiFilename = ''
  let fastaStatus = ''
  let referenceBases = ''
  let referenceBasesStart = 0
  let referenceGeneration = 0
  let canvas: HTMLCanvasElement
  let viewport: HTMLDivElement
  let contigsPanel: HTMLElement
  let canvasWidth = 0
  let viewStart = 0
  let viewEnd = 1
  let drag: { x: number; start: number; end: number } | null = null
  let panelResize: { x: number; width: number } | null = null
  let viewportRefreshFrame: number | null = null
  const DENSITY_BASES_PER_PIXEL = 5

  $: selectedReferenceData = references.find((reference) => reference.name === selectedReference)
  $: densityVisible = canvasWidth > 0 && (viewEnd - viewStart) / canvasWidth > DENSITY_BASES_PER_PIXEL
  $: viewportBasesPerPixel = Math.ceil((viewEnd - viewStart) / (canvasWidth || 700))
  $: renderCurrentResults = alignmentReady && alignmentState === 'ready'

  async function loadBam(file: File) {
    const generation = ++loadGeneration
    references = []
    alignments = []
    alignmentDensity = []
    selectedAlignment = null
    alignmentCount = 0
    alignmentsTruncated = false
    alignmentReady = false
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
    baiFilename = file.name
    baiStatus = `Reading ${file.name}…`
    try {
      const bytes = new Uint8Array(await file.arrayBuffer())
      baiIndex = await parseBaiIndex(bytes)
      baiBytes = bytes
      updateBaiStatus()
    } catch (caught) { baiStatus = caught instanceof Error ? caught.message : String(caught) }
  }

  function resetView(reference = selectedReferenceData, refreshAlignments = false) {
    if (!reference) return
    viewStart = 0
    viewEnd = Math.max(1, reference.length)
    if (refreshAlignments) refreshViewport()
    else void refreshReferenceContext(reference)
  }

  function selectReference() {
    resetView(references.find((reference) => reference.name === selectedReference))
    void loadSelectedReference()
  }

  async function loadFasta(file: File) {
    fastaFilename = file.name
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
    faiFilename = file.name
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

  function panBy(fraction: number) {
    if (!selectedReferenceData) return
    const width = viewEnd - viewStart
    viewStart = Math.max(0, Math.min(selectedReferenceData.length - width, viewStart + width * fraction))
    viewEnd = viewStart + width
    refreshViewport()
  }

  function refreshViewport() {
    if (viewportRefreshFrame !== null) return
    if (alignmentReady) alignmentState = 'loading'
    viewportRefreshFrame = requestAnimationFrame(() => {
      viewportRefreshFrame = null
      void refreshReferenceContext()
      void loadSelectedReference()
    })
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

  function referenceBaseAt(position: number): string | undefined {
    const index = position - referenceBasesStart
    return index >= 0 && index < referenceBases.length ? referenceBases[index]?.toUpperCase() : undefined
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

  function resizePanelBy(delta: number) {
    if (!contigsPanel) return
    const parentWidth = contigsPanel.parentElement?.clientWidth ?? contigsPanel.clientWidth
    const minimum = Math.min(320, parentWidth)
    const width = Math.max(minimum, Math.min(parentWidth, contigsPanel.getBoundingClientRect().width + delta))
    contigsPanel.style.width = `${width}px`
  }

  function panelResizeDown(event: PointerEvent) {
    panelResize = { x: event.clientX, width: contigsPanel.getBoundingClientRect().width }
    ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  }

  function panelResizeMove(event: PointerEvent) {
    if (!panelResize) return
    const parentWidth = contigsPanel.parentElement?.clientWidth ?? panelResize.width
    const minimum = Math.min(320, parentWidth)
    const width = Math.max(minimum, Math.min(parentWidth, panelResize.width + event.clientX - panelResize.x))
    contigsPanel.style.width = `${width}px`
  }

  function panelResizeUp() { panelResize = null }

  function panelResizeKeyDown(event: KeyboardEvent) {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return
    event.preventDefault()
    resizePanelBy(event.key === 'ArrowLeft' ? -24 : 24)
  }

  function keyDown(event: KeyboardEvent) {
    if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement) return
    if (event.key === 'ArrowLeft') { event.preventDefault(); panBy(-0.2) }
    else if (event.key === 'ArrowRight') { event.preventDefault(); panBy(0.2) }
    else if (event.key === '+' || event.key === '=') { event.preventDefault(); zoomBy(0.6) }
    else if (event.key === '-') { event.preventDefault(); zoomBy(1 / 0.6) }
    else if (event.key === 'Home') { event.preventDefault(); resetView(undefined, true) }
  }

  function drawCanvas() {
    if (!canvas || !selectedReferenceData) return
    const cssWidth = canvas.clientWidth
    const cssHeight = canvas.clientHeight
    if (!cssWidth || !cssHeight) return
    if (canvasWidth !== cssWidth) canvasWidth = cssWidth
    const ratio = window.devicePixelRatio || 1
    const pixelWidth = Math.floor(cssWidth * ratio)
    const pixelHeight = Math.floor(cssHeight * ratio)
    if (canvas.width !== pixelWidth) canvas.width = pixelWidth
    if (canvas.height !== pixelHeight) canvas.height = pixelHeight
    const context = canvas.getContext('2d')!
    context.setTransform(ratio, 0, 0, ratio, 0, 0)
    context.clearRect(0, 0, cssWidth, cssHeight)
    context.fillStyle = '#f7fafc'; context.fillRect(0, 0, cssWidth, cssHeight)
    const basesPerPixel = (viewEnd - viewStart) / cssWidth
    const toX = (position: number) => (position - viewStart) / basesPerPixel
    context.fillStyle = '#315b6d'; context.font = '12px system-ui'
    context.fillText(`${Math.floor(viewStart + 1).toLocaleString()}–${Math.ceil(viewEnd).toLocaleString()} (1-based)`, 8, 16)
    const overviewLeft = 8; const overviewWidth = cssWidth - 16
    context.fillStyle = '#d2dde4'; context.fillRect(overviewLeft, 24, overviewWidth, 10)
    const overviewStart = overviewLeft + overviewWidth * viewStart / selectedReferenceData.length
    const overviewEnd = overviewLeft + overviewWidth * viewEnd / selectedReferenceData.length
    context.fillStyle = '#176d7d'; context.fillRect(overviewStart, 24, Math.max(2, overviewEnd - overviewStart), 10)
    if (basesPerPixel > DENSITY_BASES_PER_PIXEL && renderCurrentResults && alignmentDensity.length) {
      const maximum = Math.max(...alignmentDensity, 1)
      const densityTop = 58; const densityHeight = Math.min(90, cssHeight - densityTop - 8)
      context.fillStyle = '#315b6d'; context.fillText('Alignment density', 8, 48)
      context.fillStyle = '#176d7d'
      for (let index = 0; index < alignmentDensity.length; index++) {
        const height = densityHeight * alignmentDensity[index] / maximum
        const left = index * cssWidth / alignmentDensity.length
        const right = (index + 1) * cssWidth / alignmentDensity.length
        context.fillRect(left, densityTop + densityHeight - height, Math.max(1, right - left), height)
      }
      return
    }
    if (referenceBases && basesPerPixel <= 0.15) {
      context.fillStyle = '#172433'
      for (let index = 0; index < referenceBases.length; index++) {
        const position = referenceBasesStart + index
        if (position >= viewStart && position < viewEnd) context.fillText(referenceBases[index], toX(position) + 1, 48)
      }
    }
    const lanes: number[] = []
    for (const alignment of renderCurrentResults ? alignments : []) {
      if (alignment.end <= viewStart || alignment.start >= viewEnd) continue
      let lane = lanes.findIndex((end) => end <= alignment.start)
      if (lane === -1) { lane = lanes.length; lanes.push(alignment.end) } else lanes[lane] = alignment.end
      const y = (referenceBases ? 62 : 45) + lane * 19
      if (y > cssHeight - 10) continue
      const left = toX(Math.max(alignment.start, viewStart)); const right = toX(Math.min(alignment.end, viewEnd))
      context.fillStyle = alignment.flags.is_reverse ? '#6f58a7' : '#176d7d'
      context.fillRect(left, y, right - left, 13)
      if (basesPerPixel <= 1.5) {
        for (const block of alignment.blocks) {
          for (let index = 0; index < block.bases.length; index++) {
            const position = block.start + index
            if (position < viewStart || position >= viewEnd) continue
            const base = block.bases[index].toUpperCase()
            const referenceBase = referenceBaseAt(position)
            const colour = highlightDifferences && referenceBase === base
              ? '#aab7c0'
              : ({ A: '#4daf4a', C: '#377eb8', G: '#ffb000', T: '#e34a33' } as Record<string, string>)[base] ?? '#7f8c8d'
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

  function observeViewport(element: HTMLDivElement) {
    const observer = 'ResizeObserver' in window ? new ResizeObserver(drawCanvas) : null
    observer?.observe(element)
    return { destroy: () => observer?.disconnect() }
  }

  onMount(() => {
    if (!window.WebAssembly || !window.File || !window.CanvasRenderingContext2D) {
      unsupportedBrowser = true
      error = { message: 'bamviz requires WebAssembly, the File API, and Canvas 2D. Please use a current browser.' }
    }
    window.addEventListener('resize', drawCanvas)
    return () => {
      window.removeEventListener('resize', drawCanvas)
      if (viewportRefreshFrame !== null) cancelAnimationFrame(viewportRefreshFrame)
    }
  })
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
      alignmentDensity = result.density
      selectedAlignment = previousSelection
        ? alignments.find((alignment) => alignment.read_name === previousSelection.read_name && alignment.start === previousSelection.start && alignment.end === previousSelection.end) ?? null
        : null
      alignmentCount = result.total_count
      alignmentsTruncated = result.truncated
      alignmentState = 'ready'
      alignmentReady = true
    } catch (caught) {
      if (expectedLoadGeneration !== loadGeneration || generation !== queryGeneration) return
      alignments = []
      alignmentDensity = []
      alignmentReady = false
      error = { message: caught instanceof Error ? caught.message : String(caught) }
      state = 'error'
    }
  }

  function handleFiles(files: FileList | null) {
    if (unsupportedBrowser) return
    const file = files?.[0]
    if (file) void loadBam(file)
  }

  function loadDroppedFiles(files: FileList | null) {
    if (unsupportedBrowser) return
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
  <div class="file-loaders">
    <section class="file-loader" aria-label="BAM file loader" ondragover={(event) => event.preventDefault()} ondrop={drop}>
      <h2>Alignment files</h2>
      <p>Open a BAM and, optionally, its BAI index.</p>
      <div class="file-actions"><input bind:this={fileInput} type="file" accept=".bam,application/octet-stream" onchange={(event) => handleFiles(event.currentTarget.files)} /><button disabled={unsupportedBrowser} onclick={() => fileInput.click()}>Choose BAM</button>{#if filename}<small>{filename}</small>{/if}</div>
      <div class="file-actions"><input bind:this={baiInput} type="file" accept=".bai,application/octet-stream" onchange={(event) => event.currentTarget.files?.[0] && void loadBai(event.currentTarget.files[0])} /><button disabled={unsupportedBrowser} onclick={() => baiInput.click()}>Add BAI</button>{#if baiFilename}<small>{baiFilename}</small>{/if}</div>
      {#if baiStatus}<small role="status">{baiStatus}</small>{/if}
    </section>
    <section class="reference-loader" aria-label="Optional reference files">
      <h2>Reference files</h2>
      <p>Optionally open a FASTA and its FAI index.</p>
      <div class="file-actions"><input bind:this={fastaInput} type="file" accept=".fa,.fasta,.fna,text/plain" onchange={(event) => event.currentTarget.files?.[0] && void loadFasta(event.currentTarget.files[0])} /><button disabled={unsupportedBrowser} onclick={() => fastaInput.click()}>Choose FASTA</button>{#if fastaFilename}<small>{fastaFilename}</small>{/if}</div>
      <div class="file-actions"><input bind:this={faiInput} type="file" accept=".fai,text/plain" onchange={(event) => event.currentTarget.files?.[0] && void loadFai(event.currentTarget.files[0])} /><button disabled={unsupportedBrowser} onclick={() => faiInput.click()}>Choose FAI</button>{#if faiFilename}<small>{faiFilename}</small>{/if}</div>
      {#if fastaStatus}<small role="status">{fastaStatus}</small>{/if}
    </section>
  </div>

  {#if state === 'parsing'}<p role="status">Reading the header from {filename}…</p>{/if}
  {#if error}
    <section class="error" role="alert"><h2>{unsupportedBrowser ? 'Browser support required' : `Could not load ${filename || 'BAM file'}`}</h2><p>{error.message}</p></section>
  {/if}
  {#if state === 'ready'}
    <section class="file-facts" aria-label="Loaded BAM details">
      <strong>{filename}</strong><span>{fileSize.toLocaleString()} bytes</span><span>{references.length} reference{references.length === 1 ? '' : 's'}</span>
    </section>
    {#if references.length}
      <section bind:this={contigsPanel} class="contigs" aria-label="BAM references">
        <button class="panel-resize-grip" aria-label="Resize alignment panel" aria-describedby="viewport-resize" onpointerdown={panelResizeDown} onpointermove={panelResizeMove} onpointerup={panelResizeUp} onpointercancel={panelResizeUp} onkeydown={panelResizeKeyDown}></button>
        <label for="contig">Reference / contig</label>
        <select id="contig" bind:value={selectedReference} onchange={selectReference}>
          {#each references as reference}<option value={reference.name}>{reference.name} — {reference.length.toLocaleString()} bp</option>{/each}
        </select>
        {#if selectedReference}
          <p>Selected <strong>{selectedReference}</strong>.</p>
          {#if alignmentState === 'loading'}<p role="status">Scanning alignments…</p>{/if}
          {#if alignmentReady}
            <p><strong>{alignmentCount.toLocaleString()}</strong> mapped alignment{alignmentCount === 1 ? '' : 's'} found.</p>
            <fieldset class="alignment-filters"><legend>Alignment filters</legend><label>Minimum MAPQ <input type="number" min="0" max="255" bind:value={alignmentFilter.min_mapping_quality} onchange={applyFilter} /></label><label><input type="checkbox" bind:checked={alignmentFilter.include_secondary} onchange={applyFilter} /> Include secondary</label><label><input type="checkbox" bind:checked={alignmentFilter.include_supplementary} onchange={applyFilter} /> Include supplementary</label><label><input type="checkbox" bind:checked={alignmentFilter.include_duplicates} onchange={applyFilter} /> Include duplicates</label><label><input type="checkbox" bind:checked={highlightDifferences} disabled={!referenceBases} aria-describedby="difference-colour-help" /> Highlight differences</label><small id="difference-colour-help">{referenceBases ? 'Matches are grey; mismatches use nucleotide colours.' : 'Load a matching FASTA to compare read bases.'}</small></fieldset>
            <nav class="viewer-controls" aria-label="Alignment viewer controls"><button onclick={() => resetView(undefined, true)}>Whole contig</button><button onclick={() => zoomBy(.6)}>Zoom in</button><button onclick={() => zoomBy(1 / .6)}>Zoom out</button><output aria-label="Viewport coordinates" aria-live="polite">{Math.floor(viewStart + 1).toLocaleString()}–{Math.ceil(viewEnd).toLocaleString()} (1-based), {viewportBasesPerPixel.toLocaleString()} bp/px</output></nav>
            <div bind:this={viewport} class="alignment-viewport" use:observeViewport><canvas bind:this={canvas} class="alignment-canvas" aria-label="Alignment viewport" aria-describedby="viewport-shortcuts viewport-resize" aria-busy={alignmentState === 'loading'} data-colour-mode={highlightDifferences ? 'differences' : 'bases'} tabindex="0" onkeydown={keyDown} onwheel={zoomCanvas} onpointerdown={pointerDown} onpointermove={pointerMove} onpointerup={pointerUp} onpointercancel={pointerUp}></canvas></div><small id="viewport-shortcuts">Keyboard: ←/→ pan, +/− zoom, Home resets the full contig.</small><small id="viewport-resize">Drag the two-line grip on the panel’s right edge to resize the complete viewer. Arrow keys resize it when the grip is focused.</small>
            {#if densityVisible}<small>Alignment density is shown at this zoom level; zoom in to inspect individual reads.</small>{/if}
            {#if renderCurrentResults && alignments.length}
              <div class="alignment-list" aria-label="Mapped alignments">
                <div class="alignment-heading"><span>Read / position (1-based)</span><span>CIGAR / clipping</span><span>MAPQ</span><span>Flags</span></div>
                {#each alignments.slice(0, 100) as alignment}
                  <button class:selected={selectedAlignment === alignment} class="alignment" onclick={() => selectedAlignment = alignment}><span><strong>{alignment.read_name}</strong><br />{(alignment.start + 1).toLocaleString()}–{alignment.end.toLocaleString()}</span><span><code>{alignment.cigar || '*'}</code>{#if alignment.left_clip || alignment.right_clip}<br /><small>{alignment.left_clip}′ / {alignment.right_clip}′ clipped</small>{/if}</span><span>{alignment.mapping_quality}</span><span>{alignment.flags.is_reverse ? '−' : '+'}{alignment.flags.is_secondary ? ' secondary' : ''}{alignment.flags.is_supplementary ? ' supplementary' : ''}{alignment.flags.is_duplicate ? ' duplicate' : ''}</span></button>
                {/each}
              </div>
              {#if selectedAlignment}<section class="read-details" aria-label="Selected read details"><h3>{selectedAlignment.read_name}</h3><dl><dt>Position</dt><dd>{(selectedAlignment.start + 1).toLocaleString()}–{selectedAlignment.end.toLocaleString()} (1-based)</dd><dt>CIGAR</dt><dd><code>{selectedAlignment.cigar}</code></dd><dt>Mapping quality</dt><dd>{selectedAlignment.mapping_quality}</dd><dt>Flags</dt><dd>{selectedAlignment.flags.raw} ({selectedAlignment.flags.is_reverse ? 'reverse' : 'forward'} strand{selectedAlignment.flags.is_paired ? ', paired' : ''}{selectedAlignment.flags.is_proper_pair ? ', proper pair' : ''})</dd><dt>Clipping</dt><dd>{selectedAlignment.left_clip}′ left; {selectedAlignment.right_clip}′ right</dd><dt>Mate</dt><dd>{#if selectedAlignment.mate_reference && selectedAlignment.mate_start !== null}{selectedAlignment.mate_reference}:{(selectedAlignment.mate_start + 1).toLocaleString()}{selectedAlignment.flags.mate_is_reverse ? ' (reverse)' : ''}{:else}not available{/if}</dd></dl></section>{/if}
              {#if alignmentsTruncated}<small>Showing a deterministic sample of 100 alignments from this viewport.</small>{/if}
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
  .file-loaders { display:grid; grid-template-columns:repeat(2, minmax(0, 1fr)); gap:1rem; max-width:700px }.file-loader, .reference-loader { display: grid; align-content:start; gap: .55rem; border: 2px dashed #53788b; text-align: center } .file-loader p, .reference-loader p { margin:.1rem 0 .35rem }.file-loader input, .reference-loader input { display: none }.file-actions { display:grid; gap:.3rem; justify-items:center; min-height:3.8rem }.file-actions small { overflow-wrap:anywhere } button { justify-self: center; padding: .55rem 1rem; color: #fff; background: #176d7d; border: 0; border-radius: .3rem; cursor: pointer } button:disabled { cursor:not-allowed; opacity:.55 } small { color: #536473 }
  .file-facts { display: flex; flex-wrap: wrap; gap: 1rem }.contigs { position:relative; display: grid; gap: .7rem; width:min(48rem, 100%); min-width:min(20rem, 100%); max-width:100% }.contigs select { padding: .45rem }.panel-resize-grip { position:absolute; top:7rem; right:.35rem; width:1.3rem; height:5rem; cursor:ew-resize; touch-action:none; border:0; border-radius:.25rem; background:linear-gradient(90deg, transparent 35%, #53788b 35% 42%, transparent 42% 58%, #53788b 58% 65%, transparent 65%) }.panel-resize-grip:focus-visible { outline:2px solid #176d7d; outline-offset:2px }.error { border-color: #bc4545; color: #702222 }
  .alignment-filters { display:flex; flex-wrap:wrap; gap:.7rem; border:1px solid #d2dde4; border-radius:.25rem }.alignment-filters label { display:flex; align-items:center; gap:.25rem }.alignment-filters input[type=number] { width:5rem }.alignment-list { overflow-x: auto; border: 1px solid #d2dde4; border-radius: .25rem; font-variant-numeric: tabular-nums }.alignment, .alignment-heading { display: grid; grid-template-columns: 1.4fr 1fr .7fr .9fr; gap: .7rem; padding: .45rem .6rem; min-width: 29rem }.alignment { width:100%; color:inherit; text-align:left; background:#fff; border:0; border-radius:0 }.alignment:nth-child(odd) { background: #f4f8fa }.alignment.selected { outline:2px solid #176d7d; outline-offset:-2px }.alignment-heading { color: #fff; background: #315b6d; font-weight: 700 }.read-details { margin-top:.7rem; background:#f4f8fa }.read-details h3 { margin-top:0 }.read-details dl { display:grid; grid-template-columns:max-content 1fr; gap:.35rem .8rem; margin:0 }.read-details dt { font-weight:700 }.read-details dd { margin:0 }
  .viewer-controls { display:flex; flex-wrap:wrap; align-items:center; gap:.5rem }.viewer-controls button { justify-self:auto }.viewer-controls output { margin-left:auto; color:#536473 }.alignment-viewport { width:100%; overflow:hidden; border:1px solid #b8c8d1; border-radius:.3rem }.alignment-canvas { display:block; width:100%; height:90vh; touch-action:none; cursor:grab }
  @media (max-width: 700px) { header { align-items: flex-start; flex-direction: column }.file-loaders { grid-template-columns:1fr; max-width:none } }
</style>
