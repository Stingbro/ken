<script lang="ts">
  import { onDestroy, onMount, tick } from "svelte";
  import { Crepe } from "@milkdown/crepe";
  import { editorViewCtx, remarkStringifyOptionsCtx } from "@milkdown/kit/core";
  import type { Ctx } from "@milkdown/kit/ctx";
  // Namespace import: Svelte reserves the `$` prefix, so `$prose` can only be
  // reached as a property.
  import * as milkdown from "@milkdown/kit/utils";
  import { Plugin, PluginKey, TextSelection } from "@milkdown/kit/prose/state";
  import type { Node as ProseNode } from "@milkdown/kit/prose/model";
  import { Decoration, DecorationSet, type EditorView } from "@milkdown/kit/prose/view";
  import "@milkdown/crepe/theme/common/style.css";
  import "@milkdown/crepe/theme/frame.css";
  import {
    blockquoteSchema,
    paragraphSchema,
  } from "@milkdown/kit/preset/commonmark";
  import {
    addGithubAlertMenuGroup,
    githubAlertPlugins,
  } from "./markdown/githubAlert";
  import "./markdown/githubAlert.css";
  import { disableHeadingDowngrade, headingBackspace } from "./markdown/headingBackspace";
  import {
    mermaidPreviewToggleText,
    renderMermaidPreview,
  } from "./markdown/mermaid";
  import { slashShortcutInputRule } from "./markdown/slashShortcuts";
  import { anchorLinkPlugin } from "./markdown/anchors";
  import { headingLinkPlugin } from "./markdown/headingLink";
  import { tableContextMenu } from "./markdown/tableMenu";
  import { tableFullWidthPlugins } from "./markdown/tableFullWidth";
  import { tableLayoutPlugin } from "./markdown/tableLayout";
  import ImageLightbox from "./markdown/ImageLightbox.svelte";
  import { lightbox } from "./markdown/lightbox.svelte";
  import { CURRENT_CLASS, MARK_CLASS } from "../lib/find-dom";
  import { MATCH_CAP, findTextMatches } from "../lib/find";
  import { find, type FindAdapter } from "../lib/find.svelte";

  let {
    initial,
    onchange,
  }: { initial: string; onchange: (markdown: string) => void } = $props();

  let host: HTMLDivElement;
  let crepe: Crepe | undefined;
  let view = $state<EditorView | null>(null);

  // Milkdown owns this DOM: wrapping hits in <mark> would make ProseMirror
  // reconcile markup it never produced, and could corrupt the document. Find is
  // therefore a ProseMirror decoration plugin — highlights live in the view
  // layer, the document and its selection are never touched, and the
  // decoration-only transactions stay out of history and out of autosave.
  interface Hit {
    from: number;
    to: number;
  }
  interface FindState {
    query: string;
    current: number;
    hits: Hit[];
    decorations: DecorationSet;
  }

  const findKey = new PluginKey<FindState>("ken-find");
  const EMPTY: FindState = {
    query: "",
    current: 0,
    hits: [],
    decorations: DecorationSet.empty,
  };

  function collect(doc: ProseNode, query: string): Hit[] {
    const hits: Hit[] = [];
    if (!query.trim()) return hits;
    doc.descendants((node, pos) => {
      if (hits.length >= MATCH_CAP) return false;
      if (!node.isText || !node.text) return true;
      const { starts } = findTextMatches(
        node.text,
        query,
        MATCH_CAP - hits.length,
      );
      for (const start of starts) {
        hits.push({ from: pos + start, to: pos + start + query.length });
      }
      return true;
    });
    return hits;
  }

  function build(doc: ProseNode, query: string, current: number): FindState {
    const hits = collect(doc, query);
    const decorations = DecorationSet.create(
      doc,
      hits.map((hit, i) =>
        Decoration.inline(hit.from, hit.to, {
          class: i === current ? `${MARK_CLASS} ${CURRENT_CLASS}` : MARK_CLASS,
        }),
      ),
    );
    return { query, current, hits, decorations };
  }

  const findPlugin = milkdown.$prose(
    () =>
      new Plugin<FindState>({
        key: findKey,
        state: {
          init: () => EMPTY,
          apply(tr, previous) {
            const meta = tr.getMeta(findKey) as
              | { query: string; current: number }
              | undefined;
            if (meta) return build(tr.doc, meta.query, meta.current);
            if (tr.docChanged && previous.query) {
              return build(tr.doc, previous.query, previous.current);
            }
            return previous;
          },
        },
        props: {
          decorations: (state) => findKey.getState(state)?.decorations,
        },
      }),
  );

  /** Decoration-only transaction: no doc change, no history entry, no save. */
  function paint(query: string, current: number): number {
    if (!view) return 0;
    const tr = view.state.tr.setMeta(findKey, { query, current });
    tr.setMeta("addToHistory", false);
    view.dispatch(tr);
    return findKey.getState(view.state)?.hits.length ?? 0;
  }

  $effect(() => {
    if (!view) return;
    const adapter: FindAdapter = {
      search(query) {
        const total = paint(query, 0);
        return { total, capped: total >= MATCH_CAP };
      },
      async reveal(index) {
        paint(find.query, index);
        await tick();
        host
          ?.querySelector(`.${CURRENT_CLASS}`)
          ?.scrollIntoView({ block: "center", inline: "nearest" });
      },
      clear() {
        paint("", 0);
      },
    };
    find.register(adapter);
    return () => find.unregister(adapter);
  });

  /**
   * Replace the current block with `blockquote(paragraph)` and put the cursor
   * inside it. Crepe's stock Quote item clears the block and then *wraps* it,
   * which silently does nothing whenever `findWrapping` cannot fit a blockquote
   * around the current block range; replacing works from any textblock.
   */
  function insertQuote(ctx: Ctx) {
    const editorView = ctx.get(editorViewCtx);
    const { state } = editorView;
    // Svelte reserves the `$` prefix, so the resolved position cannot be
    // destructured as `$from` here.
    const pos = state.selection.$from;
    const depth = pos.depth;
    const blockquote = blockquoteSchema.type(ctx);
    const paragraph = paragraphSchema.type(ctx);
    const node = blockquote.createAndFill(null, paragraph.create());
    if (!node) return;
    const start = pos.before(depth);
    const tr = state.tr.replaceWith(start, pos.after(depth), node);
    tr.setSelection(TextSelection.near(tr.doc.resolve(start + 2)));
    editorView.dispatch(tr.scrollIntoView());
    editorView.focus();
  }

  // --- Lightbox ------------------------------------------------------------
  // Clicks are delegated from the scroll container rather than wired into the
  // node views: Crepe owns those, and a click handler there would fight its
  // image-block selection and resize handling.
  let pointerStart: { x: number; y: number } | null = null;

  function notePointer(event: PointerEvent) {
    pointerStart = { x: event.clientX, y: event.clientY };
  }

  /** A click that dragged was a selection or a resize, not a request to zoom. */
  function wasDrag(event: MouseEvent): boolean {
    if (!pointerStart) return false;
    return (
      Math.abs(event.clientX - pointerStart.x) > 4 ||
      Math.abs(event.clientY - pointerStart.y) > 4
    );
  }

  function maybeOpenLightbox(event: MouseEvent) {
    if (event.defaultPrevented || event.button !== 0 || wasDrag(event)) return;
    const target = event.target;
    if (!(target instanceof Element)) return;
    // Never steal a click from a control: the caption and resize affordances
    // sit right on top of the image.
    if (target.closest("button, input, .image-resize-handle, .operation")) return;

    const diagram = target.closest(".mermaid-preview");
    const svg = diagram?.querySelector("svg");
    if (svg) {
      event.preventDefault();
      // A clone: the live SVG belongs to the preview and is re-rendered.
      lightbox.show({ kind: "svg", svg: svg.outerHTML });
      return;
    }

    // Crepe's image block layers a wrapper over the picture, so a plain click
    // often lands on the wrapper rather than the `<img>`; resolve either to
    // the block's image.
    const block = target.closest(".milkdown-image-block, .image-wrapper");
    const image =
      target instanceof HTMLImageElement ? target : block?.querySelector("img");
    if (image instanceof HTMLImageElement && (image.currentSrc || image.src)) {
      event.preventDefault();
      lightbox.show({
        kind: "img",
        src: image.currentSrc || image.src,
        alt: image.alt,
      });
    }
  }

  onMount(async () => {
    crepe = new Crepe({
      root: host,
      defaultValue: initial,
      // Disable LaTeX/math so `$…$` (e.g. two dollar amounts in one sentence)
      // stays literal text instead of being parsed as an inline math node.
      features: {
        [Crepe.Feature.Latex]: false,
      },
      featureConfigs: {
        [Crepe.Feature.CodeMirror]: {
          renderPreview: renderMermaidPreview,
          // Only has an effect where a preview exists, i.e. mermaid fences.
          previewOnlyByDefault: true,
          previewToggleText: mermaidPreviewToggleText,
        },
        [Crepe.Feature.BlockEdit]: {
          buildMenu: (builder) => {
            const quote = builder
              .build()
              .find((group) => group.key === "text")
              ?.items.find((item) => item.key === "quote");
            if (quote) quote.onRun = insertQuote;
            addGithubAlertMenuGroup(builder);
          },
        },
      },
    });
    crepe.on((listener) => {
      listener.markdownUpdated((_ctx, markdown, prev) => {
        if (markdown !== prev) onchange(markdown);
      });
    });
    crepe.editor.use(findPlugin);
    crepe.editor.use(githubAlertPlugins);
    crepe.editor.use(headingBackspace);
    crepe.editor.use(slashShortcutInputRule);
    crepe.editor.use(anchorLinkPlugin);
    crepe.editor.use(headingLinkPlugin);
    // After the GFM preset: the table schema extension replaces the preset's
    // own `table` node, so it has to be registered last.
    crepe.editor.use(tableFullWidthPlugins);
    crepe.editor.use(tableLayoutPlugin);
    crepe.editor.use(tableContextMenu);
    crepe.editor.config(disableHeadingDowngrade);
    // House style is `- ` bullets; remark-stringify would otherwise write `*`.
    crepe.editor.config((ctx) => {
      ctx.update(remarkStringifyOptionsCtx, (opts) => ({
        ...opts,
        bullet: "-" as const,
      }));
    });
    await crepe.create();
    crepe.editor.action((ctx) => {
      view = ctx.get(editorViewCtx);
    });
  });

  onDestroy(() => {
    void crepe?.destroy();
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="editor-scroll" onpointerdown={notePointer} onclick={maybeOpenLightbox}>
  <div class="measure" bind:this={host}></div>
</div>
<ImageLightbox />

<style>
  .editor-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    /* Makes the pane a query container so a table can size itself against the
       pane rather than the 720px text column (`100cqw` below). The pane's own
       width comes from the flex parent, so inline-size containment is safe. */
    container-type: inline-size;
  }
  .measure {
    max-width: 720px;
    margin: 0 auto;
    padding: 24px clamp(20px, 5%, 48px) 80px;
  }

  /* Paper & Ink over Crepe's frame theme */
  .measure :global(.milkdown) {
    --crepe-color-background: var(--surface);
    --crepe-color-on-background: var(--ink);
    --crepe-color-surface: var(--surface);
    --crepe-color-surface-low: var(--paper);
    --crepe-color-on-surface: var(--ink);
    --crepe-color-on-surface-variant: var(--ink-secondary);
    --crepe-color-outline: var(--border-strong);
    --crepe-color-primary: var(--accent);
    --crepe-color-secondary: var(--sunken);
    --crepe-color-on-secondary: var(--ink);
    --crepe-color-inverse: var(--ink);
    --crepe-color-on-inverse: var(--paper);
    --crepe-color-inline-code: var(--accent-deep);
    --crepe-color-error: var(--danger);
    --crepe-color-hover: var(--sunken);
    --crepe-color-selected: var(--selection-bg);
    --crepe-color-inline-area: var(--sunken);
    --crepe-font-title: var(--font-serif);
    --crepe-font-default: var(--font-sans);
    --crepe-font-code: var(--font-mono);
    --crepe-shadow-1: var(--shadow-card);
    --crepe-shadow-2: var(--shadow-overlay);
    background: transparent;
  }
  .measure :global(.milkdown .ProseMirror) {
    padding: 0;
    font-size: 14.5px;
    line-height: 1.85;
  }
  .measure :global(.milkdown .ProseMirror p) {
    line-height: 1.85;
  }
  .measure :global(.milkdown h1),
  .measure :global(.milkdown h2),
  .measure :global(.milkdown h3),
  .measure :global(.milkdown h4) {
    font-family: var(--font-serif);
    font-weight: 500;
    letter-spacing: -0.01em;
    /* A touch more breathing room after headings. */
    margin-bottom: 0.55em;
  }

  /* --- Mermaid ------------------------------------------------------------ */
  /* In diagram mode (preview only: the CodeMirror host is hidden) the toolbar
     is a distraction, so it only fades in while the block is hovered or has
     focus. In code mode it stays put, as it always has. */
  .measure
    :global(.milkdown .milkdown-code-block:has(.codemirror-host.hidden) .tools) {
    opacity: 0;
    transition: opacity 120ms ease;
  }
  .measure
    :global(
      .milkdown .milkdown-code-block:has(.codemirror-host.hidden):hover .tools
    ),
  .measure
    :global(
      .milkdown
        .milkdown-code-block:has(.codemirror-host.hidden):focus-within
        .tools
    ) {
    opacity: 1;
  }
  .measure :global(.milkdown .mermaid-preview) {
    display: flex;
    justify-content: center;
  }
  .measure :global(.milkdown .mermaid-preview svg) {
    max-width: 100%;
    height: auto;
    background: transparent;
  }
  /* Mermaid sizes its label boxes itself and renders the text in a
     foreignObject; the editor's generous paragraph line-height would overflow
     those boxes and clip descenders. */
  .measure :global(.milkdown .mermaid-preview foreignObject p),
  .measure :global(.milkdown .mermaid-preview foreignObject div),
  .measure :global(.milkdown .mermaid-preview foreignObject span) {
    margin: 0;
    line-height: inherit;
  }
  .measure :global(.milkdown .mermaid-error) {
    padding: 8px 12px;
    border-left: 3px solid var(--danger);
    color: var(--danger);
    font-family: var(--font-mono);
    font-size: 12.5px;
  }

  /* --- Tables ------------------------------------------------------------- */
  /* A table never scrolls sideways and never pushes the pane wider: it shrinks
     to the space it is given and its cells wrap. By default that space is the
     text column. The hover toggle (and the right-click menu) switch a table
     into full-width mode, where it fills the pane's gutters up to 1200px and
     centres on the text column — `left`/`translateX` rather than auto margins,
     so it stays centred even when it is wider than its containing block. The
     choice is stored in the markdown as an HTML comment (see
     markdown/tableFullWidth.ts) and arrives here as `data-full-width`. */
  .measure :global(.milkdown .milkdown-table-block) {
    position: relative;
    /* Shrink-to-fit, capped at the text column. */
    width: fit-content;
    max-width: 100%;
    /* Tables need more air than a paragraph to read as their own block. */
    margin: 1.5em 0;
  }
  .measure :global(.milkdown .milkdown-table-block[data-full-width="true"]) {
    width: min(calc(100cqw - 2 * clamp(20px, 5cqw, 48px)), 1200px);
    max-width: min(calc(100cqw - 2 * clamp(20px, 5cqw, 48px)), 1200px);
    left: 50%;
    transform: translateX(-50%);
  }
  .measure :global(.milkdown .milkdown-table-block .table-wrapper) {
    /* Deliberately not `auto`: a table that outgrows its cap wraps its text
       rather than hiding columns behind a scrollbar. */
    overflow: visible;
  }
  /* ProseMirror's stylesheet pins tables to `width: 100%; table-layout: fixed`,
     which divides the column evenly however long the headings are. `auto` plus
     a `max-width` lets the browser size columns to their content and then
     squeeze them back inside the cap.

     That auto layout is only the starting point a table is painted with:
     markdown/tableLayout.ts then measures each column's content and writes
     exact widths to a `<colgroup>`, switching the table to
     `table-layout: fixed` inline so they are honoured to the pixel. See that
     file for why the browser's own auto layout cannot be left to do it. */
  .measure :global(.milkdown .milkdown-table-block table.children) {
    width: auto;
    min-width: 0;
    max-width: 100%;
    table-layout: auto;
  }
  .measure
    :global(
      .milkdown .milkdown-table-block[data-full-width="true"] table.children
    ) {
    width: 100%;
  }

  /* Crepe's cell borders are the outline colour at 20% opacity, which all but
     disappears on paper. Ken draws the full hairline instead, with a heavier
     rule under the header row. Long cells wrap; `anywhere` is what lets a
     single unbroken token (a URL, an id) give way rather than spill out of a
     column that tableLayout.ts has had to make narrower than it. */
  .measure :global(.milkdown .milkdown-table-block th),
  .measure :global(.milkdown .milkdown-table-block td) {
    border: 1px solid var(--border-strong);
    /* Roomy on a wide pane; tighter on a narrow one, where the padding of a
       many-columned table is what would stop it fitting at all. */
    padding: 6px clamp(7px, 1.2cqw, 16px);
    white-space: normal;
    overflow-wrap: anywhere;
    word-break: normal;
  }
  .measure :global(.milkdown .milkdown-table-block th) {
    background: var(--sunken);
    border-bottom-color: var(--ink-tertiary);
  }

  /* Hover-only, like the mermaid toolbar: the width toggle sits just above the
     table's top-right corner so it never covers a cell. */
  .measure :global(.milkdown .milkdown-table-block .ken-table-width-toggle) {
    position: absolute;
    top: -12px;
    right: 0;
    z-index: 40;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--surface);
    color: var(--ink-tertiary);
    box-shadow: var(--shadow-card);
    opacity: 0;
    transition: opacity 120ms ease;
  }
  .measure
    :global(.milkdown .milkdown-table-block:hover .ken-table-width-toggle),
  .measure
    :global(
      .milkdown .milkdown-table-block .ken-table-width-toggle:focus-visible
    ) {
    opacity: 1;
  }
  .measure
    :global(.milkdown .milkdown-table-block .ken-table-width-toggle:hover) {
    color: var(--ink);
    border-color: var(--ink-tertiary);
  }

  /* Crepe draws the row/column handles in `--crepe-color-outline`, which maps
     to Ken's hairline `--border-strong` and left them all but invisible against
     the paper. Give the pills a readable glyph and an edge so they read as
     controls; everything else in the table block is Crepe's. */
  .measure :global(.milkdown .milkdown-table-block .cell-handle),
  .measure :global(.milkdown .milkdown-table-block .line-handle .add-button) {
    color: var(--ink-tertiary);
    border: 1px solid var(--border-strong);
  }
  .measure :global(.milkdown .milkdown-table-block .cell-handle svg),
  .measure
    :global(.milkdown .milkdown-table-block .line-handle .add-button svg) {
    fill: var(--ink-tertiary);
  }
  .measure :global(.milkdown .milkdown-table-block .cell-handle:hover),
  .measure
    :global(.milkdown .milkdown-table-block .line-handle .add-button:hover) {
    border-color: var(--ink-tertiary);
  }
  /* The align/delete popup floats over body text, so it needs a real edge. */
  .measure :global(.milkdown .milkdown-table-block .cell-handle .button-group) {
    border: 1px solid var(--border);
  }
  .measure
    :global(.milkdown .milkdown-table-block .cell-handle .button-group svg) {
    fill: var(--ink-secondary);
  }

  /* --- GitHub-flavored markdown ------------------------------------------ */
  .measure :global(.milkdown .ProseMirror del) {
    color: var(--ink-secondary);
  }
  /* A ticked task reads as done. */
  .measure :global(.milkdown .list-item:has(> .label-wrapper .label.checked)) {
    color: var(--ink-secondary);
  }
  /* Footnote definitions read as an aside, not as body copy. */
  .measure :global(.milkdown dl[data-type="footnote_definition"]) {
    border-top: 1px solid var(--border);
    margin-top: 24px;
    padding-top: 12px;
    font-size: 13px;
    color: var(--ink-secondary);
  }

  /* A wider scale than Crepe's, so the outline of a document reads at a glance.
     Line height tightens as the type grows, and the space above each heading
     grows with it, so nothing crowds. */
  .measure :global(.milkdown h1) {
    font-size: 2.25em;
    line-height: 1.18;
    margin-top: 0.9em;
  }
  .measure :global(.milkdown h2) {
    font-size: 1.75em;
    line-height: 1.24;
    margin-top: 1.1em;
  }
  .measure :global(.milkdown h3) {
    font-size: 1.45em;
    line-height: 1.3;
    margin-top: 1.2em;
  }
  .measure :global(.milkdown h4) {
    font-size: 1.25em;
    line-height: 1.36;
    margin-top: 1.3em;
  }
  .measure :global(.milkdown h5) {
    font-size: 1.1em;
    line-height: 1.42;
    margin-top: 1.4em;
  }
  .measure :global(.milkdown h6) {
    font-size: 1em;
    line-height: 1.5;
    margin-top: 1.5em;
    color: var(--ink-secondary);
  }
  /* The document's own title has nothing to separate itself from. */
  .measure :global(.milkdown .ProseMirror > h1:first-child),
  .measure :global(.milkdown .ProseMirror > h2:first-child),
  .measure :global(.milkdown .ProseMirror > h3:first-child) {
    margin-top: 0;
  }

  /* --- Heading links ------------------------------------------------------ */
  /* A hover-only "copy link" button in the text column's left gutter. It is a
     ProseMirror widget, so it is never part of the document.

     Crepe's block drag/insert handle already sits in that gutter, from 82px to
     16px left of the text, and appears on the same hover — so the button goes
     further out still, leaving both clickable. How far out is what the gutter
     allows: `(100cqw - 100%) / 2` is the distance from the text column's edge
     to the pane's, against `.editor-scroll` as the query container. Wide
     panes give the full 110px, which clears Crepe's handle; narrower ones
     slide the button in towards the text until it is flush with the pane
     edge, where it still clears the text (and sits above Crepe's handle,
     which is itself clipped at those widths). */
  .measure :global(.milkdown h1),
  .measure :global(.milkdown h2),
  .measure :global(.milkdown h3),
  .measure :global(.milkdown h4),
  .measure :global(.milkdown h5),
  .measure :global(.milkdown h6) {
    position: relative;
  }
  .measure :global(.milkdown .ken-heading-link) {
    position: absolute;
    left: calc(-1 * clamp(28px, (100cqw - 100%) / 2 - 4px, 110px));
    /* Centred on the heading's first line, whatever its size. A button does
       not inherit the font size by default, so `em` would otherwise resolve
       against the UA's 13.3px rather than the heading's type. */
    font-size: inherit;
    top: 0.62em;
    transform: translateY(-50%);
    z-index: 5;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: none;
    border-radius: 6px;
    background: transparent;
    box-shadow: none;
    color: var(--ink-tertiary);
    opacity: 0;
    transition: opacity 120ms ease;
    user-select: none;
    line-height: 1;
  }
  .measure :global(.milkdown h1:hover .ken-heading-link),
  .measure :global(.milkdown h2:hover .ken-heading-link),
  .measure :global(.milkdown h3:hover .ken-heading-link),
  .measure :global(.milkdown h4:hover .ken-heading-link),
  .measure :global(.milkdown h5:hover .ken-heading-link),
  .measure :global(.milkdown h6:hover .ken-heading-link),
  .measure :global(.milkdown .ken-heading-link:focus-visible) {
    opacity: 1;
  }
  .measure :global(.milkdown .ken-heading-link:hover) {
    color: var(--ink);
    background: var(--sunken);
  }
  .measure :global(.milkdown .ken-heading-link.copied) {
    opacity: 1;
    color: var(--accent-deep);
  }

  /* --- Lightbox affordance ------------------------------------------------ */
  .measure :global(.milkdown .mermaid-preview),
  .measure :global(.milkdown .milkdown-image-block img),
  .measure :global(.milkdown .milkdown-image-block .image-wrapper) {
    cursor: zoom-in;
  }
</style>
