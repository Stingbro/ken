<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import type { PageViewport, PDFDocumentProxy, PDFPageProxy } from "pdfjs-dist";
  import type { TextItem } from "pdfjs-dist/types/src/display/api";
  // pdf.js's own stylesheet — it styles the form-field widgets (.annotationLayer)
  // we mount over the canvas. Its other rules are namespaced to pdf.js class
  // names Ken doesn't use.
  import "pdfjs-dist/web/pdf_viewer.css";
  import { api, type OcrRegion } from "../../lib/api";
  import { MATCH_CAP, findTextMatches } from "../../lib/find";
  import { find, type FindAdapter } from "../../lib/find.svelte";
  import { createFormSaver, type FormStorage } from "./pdfForm";
  import PreviewLoading from "./PreviewLoading.svelte";

  let {
    relPath,
    onfillable,
    onchange,
    onsaved,
    onerror,
  }: {
    relPath: string;
    /** The PDF carries fillable form fields, so it isn't read-only after all. */
    onfillable?: () => void;
    /** A field was edited and the change isn't on disk yet. */
    onchange?: () => void;
    onsaved?: (mtime: number) => void;
    onerror?: (message: string) => void;
  } = $props();

  let host: HTMLDivElement;
  let error = $state<string | null>(null);
  let loading = $state(true);
  let cancelled = false;

  interface Page {
    number: number;
    overlay: HTMLDivElement;
    wrapper: HTMLDivElement;
    viewport: PageViewport;
    /** Text runs, fetched the first time someone searches this document. */
    items?: TextItem[];
    /** The pdf.js form-widget layer, on a fillable PDF. */
    layer?: { destroy?: () => void };
    /** Keeps the widget layer's scale in step with the displayed canvas. */
    observer?: ResizeObserver;
  }

  /** A hit on a page's pdf.js text layer. */
  interface TextHit {
    kind: "text";
    page: number; // index into `pages`
    item: number;
    start: number;
  }

  /** A hit on an OCR region — for scanned pages with no text layer. */
  interface OcrHit {
    kind: "ocr";
    page: number; // index into `pages`
    /** `[x, y, w, h]`, normalized, top-left origin — same convention as the overlay. */
    bbox: [number, number, number, number];
  }

  type Hit = TextHit | OcrHit;

  let doc: PDFDocumentProxy | null = null;
  /** Set only for a fillable PDF: debounces field edits into a save. */
  let saver: ReturnType<typeof createFormSaver> | null = null;
  let pages: Page[] = [];
  let hits: Hit[] = [];
  let boxes: HTMLElement[] = [];
  /** OCR regions indexed by 0-based page, for pages whose text layer is empty. */
  let ocrByPage = new Map<number, OcrRegion[]>();

  // A PDF is rendered to canvas, so a hit has to be drawn as a box over the
  // page. Text is read per page, and only for the first TEXT_PAGE_CAP pages —
  // pulling the text layer out of a 900-page scan would stall the UI.
  const TEXT_PAGE_CAP = 200;

  async function loadText(): Promise<void> {
    if (!doc) return;
    for (const page of pages.slice(0, TEXT_PAGE_CAP)) {
      if (page.items || cancelled) continue;
      const content = await doc.getPage(page.number).then((p) => p.getTextContent());
      page.items = content.items.filter(
        (item): item is TextItem => "str" in item,
      );
    }
  }

  function clearBoxes() {
    for (const box of boxes) box.remove();
    boxes = [];
  }

  /**
   * Where a run of characters sits on the page. pdf.js gives the position and
   * width of a whole text run, not of each glyph, so the offset inside the run
   * is interpolated by character count — exact for the monospaced and near
   * enough for everything else to land the highlight on the right words.
   */
  /** Whether a page carries a real pdf.js text layer (vs. a bare scan). Pages
      past TEXT_PAGE_CAP have no items loaded and count as text-less, so OCR
      covers them. */
  function pageHasText(p: number): boolean {
    const items = pages[p].items;
    return !!items && items.some((it) => it.str.trim().length > 0);
  }

  /** An OCR hit's box: the normalized top-left bbox maps straight to the
      per-page overlay's percentage coordinates — same convention `.page-overlay`
      already uses, so no transform math. */
  function drawOcr(hit: OcrHit, current: boolean): HTMLElement {
    const page = pages[hit.page];
    const [x, y, w, h] = hit.bbox;
    const box = document.createElement("div");
    box.className = current ? "pdf-hit current" : "pdf-hit";
    box.style.left = `${x * 100}%`;
    box.style.top = `${y * 100}%`;
    box.style.width = `${w * 100}%`;
    box.style.height = `${h * 100}%`;
    page.overlay.appendChild(box);
    boxes.push(box);
    return box;
  }

  function draw(hit: TextHit, queryLength: number, current: boolean): HTMLElement | null {
    const page = pages[hit.page];
    const item = page.items?.[hit.item];
    if (!item || !item.str.length) return null;

    const [, , c, d, e, f] = mul(
      page.viewport.transform as unknown as number[],
      item.transform,
    );
    const height = Math.hypot(c, d) || item.height;
    const width = item.width * page.viewport.scale;
    const left = e + (width * hit.start) / item.str.length;
    const boxWidth = Math.max((width * queryLength) / item.str.length, 2);

    // Positioned in percentages, not pixels: a narrow pane scales the canvas
    // down (max-width: 100%) and the boxes have to scale with it.
    const pw = page.viewport.width;
    const ph = page.viewport.height;
    const box = document.createElement("div");
    box.className = current ? "pdf-hit current" : "pdf-hit";
    box.style.left = `${(left / pw) * 100}%`;
    box.style.top = `${((f - height) / ph) * 100}%`;
    box.style.width = `${(boxWidth / pw) * 100}%`;
    box.style.height = `${((height * 1.15) / ph) * 100}%`;
    page.overlay.appendChild(box);
    boxes.push(box);
    return box;
  }

  /** 2D matrix multiply — pdf.js's Util.transform, without importing the module. */
  function mul(a: number[], b: number[]): number[] {
    return [
      a[0] * b[0] + a[2] * b[1],
      a[1] * b[0] + a[3] * b[1],
      a[0] * b[2] + a[2] * b[3],
      a[1] * b[2] + a[3] * b[3],
      a[0] * b[4] + a[2] * b[5] + a[4],
      a[1] * b[4] + a[3] * b[5] + a[5],
    ];
  }

  // Bumped on every search so a slow one (loadText awaits pdf.js) that resolves
  // after the user has already typed more can't overwrite the newer result's
  // boxes. The controller drops the stale total; this keeps the drawn boxes and
  // `hits` in step with the query the reader can actually see.
  let searchGen = 0;

  $effect(() => {
    const adapter: FindAdapter = {
      async search(query) {
        const gen = ++searchGen;
        if (!query.trim()) {
          clearBoxes();
          hits = [];
          return { total: 0 };
        }
        await loadText();
        if (gen !== searchGen) return { total: 0 }; // superseded; leave shared state alone

        // Page-then-position order: for each page, use its pdf.js text layer if
        // it has one, else fall back to OCR regions (a scanned page). Never both
        // — a page with a real text layer would otherwise double-count. A match
        // split across two text runs ("in-" / "voice") isn't found; the runs are
        // what pdf.js gives us, and stitching them would misplace the box.
        const found: Hit[] = [];
        for (let p = 0; p < pages.length && found.length < MATCH_CAP; p++) {
          if (pageHasText(p)) {
            const items = pages[p].items ?? [];
            for (let i = 0; i < items.length && found.length < MATCH_CAP; i++) {
              for (const start of findTextMatches(
                items[i].str,
                query,
                MATCH_CAP - found.length,
              ).starts) {
                found.push({ kind: "text", page: p, item: i, start });
              }
            }
          } else {
            for (const region of ocrByPage.get(p) ?? []) {
              if (found.length >= MATCH_CAP) break;
              const count = findTextMatches(
                region.text,
                query,
                MATCH_CAP - found.length,
              ).starts.length;
              for (let i = 0; i < count && found.length < MATCH_CAP; i++) {
                found.push({ kind: "ocr", page: p, bbox: region.bbox });
              }
            }
          }
        }

        // Commit in one synchronous step: concurrent searches each build their
        // own list, so an older one can never mix its hits into a newer count or
        // leave orphaned boxes on the page.
        clearBoxes();
        hits = found;
        for (const hit of found) {
          if (hit.kind === "ocr") drawOcr(hit, false);
          else draw(hit, query.length, false);
        }
        return {
          total: found.length,
          capped: found.length >= MATCH_CAP,
          note:
            pages.length > TEXT_PAGE_CAP
              ? `Searching the first ${TEXT_PAGE_CAP} pages of ${pages.length}.`
              : undefined,
        };
      },
      reveal(index, opts) {
        boxes.forEach((box, i) => box.classList.toggle("current", i === index));
        if (opts?.scroll !== false) {
          boxes[index]?.scrollIntoView({ block: "center", inline: "nearest" });
        }
      },
      clear() {
        hits = [];
        clearBoxes();
      },
    };
    find.register(adapter);
    return () => {
      clearBoxes();
      find.unregister(adapter);
    };
  });

  /**
   * Mounts pdf.js's interactive widget layer over a rendered page.
   *
   * Geometry note: each field is positioned and sized in PERCENTAGES of the
   * layer (see AnnotationElement._createContainer in pdf.mjs), so pinning the
   * layer to the page box — which is `max-width: 100%` and therefore shrinks
   * with a narrow pane — makes every field track the canvas for free. What does
   * NOT scale that way is text: font sizes and corner radii are written as
   * `calc(Npx * var(--total-scale-factor))`. So we own that variable and keep it
   * at the canvas's *displayed* scale via a ResizeObserver — approach (a). The
   * width/height `setLayerDimensions` writes (absolute px off
   * `--total-scale-factor`) would fight the shrinking canvas, so they're
   * replaced with 100%; the call is still made for its `data-main-rotation`.
   */
  async function mountFormLayer(
    pdfjs: typeof import("pdfjs-dist"),
    entry: Page,
    page: PDFPageProxy,
    canvasMap: Map<string, HTMLCanvasElement>,
  ): Promise<void> {
    if (!doc) return;
    const { wrapper, overlay, viewport } = entry;

    const layerDiv = document.createElement("div");
    layerDiv.className = "annotationLayer";
    // After the canvas, before the overlay: find highlights stay on top, and the
    // overlay is pointer-events:none so clicks still reach the fields.
    wrapper.insertBefore(layerDiv, overlay);

    pdfjs.setLayerDimensions(layerDiv, viewport);
    layerDiv.style.width = "100%";
    layerDiv.style.height = "100%";

    const rescale = () => {
      const shown = wrapper.clientWidth || viewport.width;
      const effective = viewport.scale * (shown / viewport.width);
      wrapper.style.setProperty("--total-scale-factor", String(effective));
      wrapper.style.setProperty("--scale-factor", String(effective));
    };
    rescale();
    const observer = new ResizeObserver(rescale);
    observer.observe(wrapper);
    entry.observer = observer;

    // pdf.js insists on a link service even for a form-only layer. Ken has no
    // in-document navigation, so links resolve to nothing rather than jumping.
    const linkService = {
      getDestinationHash: () => "#",
      getAnchorUrl: () => "#",
      addLinkAttributes: () => {},
      executeNamedAction: () => {},
      executeSetOCGState: () => {},
      goToDestination: async () => {},
      getAttachmentContent: async () => null,
      eventBus: null,
      isInPresentationMode: false,
      externalLinkEnabled: false,
      pagesCount: doc.numPages,
      page: entry.number,
      rotation: 0,
    };

    const annotations = await page.getAnnotations({ intent: "display" });
    const layer = new pdfjs.AnnotationLayer({
      div: layerDiv,
      page,
      viewport,
      accessibilityManager: null,
      // The very map page.render() just filled: AnnotationLayer.render moves
      // each widget's canvas into its <section> and empties the map as it goes.
      annotationCanvasMap: canvasMap,
      annotationEditorUIManager: null,
      structTreeLayer: null,
      commentManager: null,
      linkService,
      annotationStorage: doc.annotationStorage,
    });
    await layer.render({
      viewport,
      div: layerDiv,
      annotations,
      page,
      linkService,
      renderForms: true,
      annotationStorage: doc.annotationStorage,
      imageResourcesPath: "",
    } as never);
    entry.layer = layer as unknown as { destroy?: () => void };
  }

  onMount(async () => {
    try {
      const pdfjs = await import("pdfjs-dist");
      pdfjs.GlobalWorkerOptions.workerSrc = new URL(
        "pdfjs-dist/build/pdf.worker.min.mjs",
        import.meta.url,
      ).toString();

      const bytes = await api.readFileBytes(relPath);
      doc = await pdfjs.getDocument({ data: new Uint8Array(bytes) }).promise;
      loading = false;

      // A PDF with AcroForm fields is fillable in Ken: the widgets render as
      // real HTML inputs over the canvas and every edit auto-saves. Everything
      // else stays a plain read-only render.
      let fillable = false;
      try {
        const fields = await doc.getFieldObjects();
        fillable = !!fields && Object.keys(fields).length > 0;
      } catch {
        // A malformed AcroForm shouldn't cost the reader the whole render.
      }
      if (fillable) {
        onfillable?.();
        // pdf.js only reports edits through the storage; the saver turns that
        // stream into the same debounce-and-save rhythm as the text editors.
        saver = createFormSaver({
          storage: doc.annotationStorage as unknown as FormStorage,
          serialize: () => doc!.saveDocument(),
          write: (b) => api.saveFileBytes(relPath, b),
          onchange,
          onsaved,
          onerror,
        });
      }

      for (let i = 1; i <= doc.numPages && !cancelled; i++) {
        const page = await doc.getPage(i);
        const scale = 1.4;
        const viewport = page.getViewport({ scale });
        const canvas = document.createElement("canvas");
        const ratio = window.devicePixelRatio || 1;
        canvas.width = viewport.width * ratio;
        canvas.height = viewport.height * ratio;
        canvas.style.width = `${viewport.width}px`;
        const ctx = canvas.getContext("2d")!;

        // Canvas plus an overlay the find boxes are drawn into, in page pixels.
        const wrapper = document.createElement("div");
        wrapper.className = "page-box";
        wrapper.style.width = `${viewport.width}px`;
        const overlay = document.createElement("div");
        overlay.className = "page-overlay";
        wrapper.append(canvas, overlay);
        host.appendChild(wrapper);
        const entry: Page = { number: i, wrapper, overlay, viewport };
        pages.push(entry);

        // A checkbox or radio button always draws itself into its own canvas
        // (ButtonWidgetAnnotation sets hasOwnCanvas), one per appearance state.
        // page.render() only produces those canvases if it is handed a map to
        // put them in — without it the boxes render blank, ticked or not. The
        // same map instance then goes to the AnnotationLayer, which moves each
        // canvas into its widget's <section>, where pdf.js's CSS shows the
        // "checked" or "unchecked" one to match the live <input>.
        const canvasMap = new Map<string, HTMLCanvasElement>();

        // ENABLE_FORMS keeps the widgets' baked-in appearances off the canvas so
        // the live HTML fields aren't drawn twice, once stale.
        await page.render({
          canvasContext: ctx,
          viewport,
          canvas,
          annotationMode: fillable
            ? pdfjs.AnnotationMode.ENABLE_FORMS
            : pdfjs.AnnotationMode.ENABLE,
          annotationCanvasMap: fillable ? canvasMap : undefined,
          // HiDPI is expressed as a render transform, NOT a pre-applied
          // ctx.scale(): pdf.js reads outputScaleX/Y off this matrix
          // (beginDrawing) and sizes each widget's own canvas from it
          // (beginAnnotation: ceil(w * outputScaleX * viewportScale)). A scaled
          // context alone leaves those at 1, so a check mark came out at 1x in
          // the corner of a 2x box. The page pixels are identical either way,
          // and the find-highlight math still uses the unscaled `viewport`.
          transform: ratio !== 1 ? [ratio, 0, 0, ratio, 0, 0] : undefined,
        }).promise;

        if (fillable && !cancelled) {
          try {
            await mountFormLayer(pdfjs, entry, page, canvasMap);
          } catch {
            // That page's widgets stay non-interactive; the rest of the
            // document still renders and the other pages still fill in.
          }
        }
      }
      find.refresh(); // pages arrived after the user already typed a query
    } catch (e) {
      loading = false;
      error = `Couldn't render this PDF — ${e}. Try “Open in default app”.`;
    }

    // OCR is independent of rendering — makes scanned pages (no text layer)
    // findable. A failure (non-macOS, still processing) just leaves the PDF as a
    // pure text-layer search.
    try {
      const regions = await api.getOcrRegions(relPath);
      if (regions.length) {
        const byPage = new Map<number, OcrRegion[]>();
        for (const region of regions) {
          const list = byPage.get(region.page);
          if (list) list.push(region);
          else byPage.set(region.page, [region]);
        }
        // Position order within each page (top-to-bottom, then left-to-right) so
        // OCR match navigation is coherent.
        for (const list of byPage.values()) {
          list.sort((a, b) => a.bbox[1] - b.bbox[1] || a.bbox[0] - b.bbox[0]);
        }
        ocrByPage = byPage;
        find.refresh(); // regions arrived after the user already typed a query
      }
    } catch {
      // Leave the text-layer-only behavior in place.
    }
  });

  onDestroy(() => {
    cancelled = true;
    // Flush before unhooking, so a half-typed field isn't lost on tab close —
    // the same flush-on-close the text editors do.
    if (saver) {
      void saver.flush();
      saver.dispose();
      saver = null;
    }
    for (const page of pages) {
      page.observer?.disconnect();
      page.layer?.destroy?.();
    }
  });
</script>

<div class="scroll">
  {#if loading}
    <PreviewLoading label="Rendering PDF…" />
  {/if}
  {#if error}
    <div class="note error">{error}</div>
  {/if}
  <div class="pages" bind:this={host}></div>
</div>

<style>
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    background: var(--sunken);
    padding: 24px;
  }
  .pages {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
  }
  .pages :global(.page-box) {
    position: relative;
    max-width: 100%;
  }
  .pages :global(.page-overlay) {
    position: absolute;
    inset: 0;
    pointer-events: none;
    /* Above the form widgets, which pdf.js stacks from z-index 0 upwards, so a
       find highlight is never hidden behind a field. */
    z-index: 1000;
  }
  /* Form widgets sit between the canvas and the find overlay, pinned to the page
     box so they scale with the canvas (see mountFormLayer). */
  .pages :global(.annotationLayer) {
    position: absolute;
    inset: 0;
    z-index: 1;
  }
  /* Nothing may clip a field: a multi-line value has to scroll inside its own
     textarea rather than be cut off by the page box. */
  .pages :global(.annotationLayer textarea) {
    overflow-y: auto;
    white-space: pre-wrap;
    overflow-wrap: break-word;
  }
  .pages :global(.pdf-hit) {
    position: absolute;
    background: color-mix(in srgb, var(--needs-input) 45%, transparent);
    border-radius: 2px;
    mix-blend-mode: multiply;
  }
  .pages :global(.pdf-hit.current) {
    background: color-mix(in srgb, var(--accent) 50%, transparent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  /* The page canvas only — NOT the little per-widget appearance canvases pdf.js
     puts inside .annotationLayer sections, which must stay unstyled. */
  .pages :global(.page-box > canvas) {
    display: block;
    max-width: 100%;
    box-shadow: var(--shadow-card);
    border-radius: 4px;
    background: white;
  }
  .note {
    text-align: center;
    color: var(--ink-tertiary);
    font-size: 13px;
    padding: 20px;
  }
  .note.error {
    color: var(--danger);
  }
</style>
