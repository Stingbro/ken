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

<div class="editor-scroll">
  <div class="measure" bind:this={host}></div>
</div>

<style>
  .editor-scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
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

  /* Slightly smaller than Crepe's defaults, keeping the scale gentle. */
  .measure :global(.milkdown h1) {
    font-size: 1.7em;
  }
  .measure :global(.milkdown h2) {
    font-size: 1.35em;
  }
  .measure :global(.milkdown h3) {
    font-size: 1.15em;
  }
  .measure :global(.milkdown h4) {
    font-size: 1.02em;
  }
</style>
