<script module lang="ts">
  // The viewer is a 4 MB classic script that defines window.GraphViewer once per
  // webview. Injection state therefore belongs to the module, not to a component
  // instance: two .drawio files opened in quick succession must share one
  // in-flight promise rather than each appending their own <script> and
  // re-evaluating the bundle mid-load.
  import viewerUrl from "./vendor/drawio-viewer.min.js?url";

  type Viewer = { createViewerForElement(el: HTMLElement): void };
  const win = window as unknown as {
    GraphViewer?: Viewer;
    MathJax?: unknown;
    onDrawioViewerLoad?: () => void;
    Editor?: { MathJaxRender?: (el: HTMLElement) => void };
  };

  let loader: Promise<void> | null = null;

  function loadViewer(): Promise<void> {
    if (win.GraphViewer) return Promise.resolve();
    if (!loader) {
      // Two globals set before the script runs, both to keep it offline and
      // self-contained. `MathJax` already being defined makes its bootstrap
      // skip appending a <script> for MathJax from viewer.diagrams.net — the
      // one unconditional network call the viewer would otherwise make on
      // load. `onDrawioViewerLoad` replaces its own document-wide scan for
      // .mxgraph divs; this component initialises its container itself.
      win.MathJax ??= {};
      win.onDrawioViewerLoad ??= () => {};
      loader = new Promise((resolve, reject) => {
        const s = document.createElement("script");
        s.src = viewerUrl;
        // A failure must not poison the rest of the session: drop the promise
        // and the dead tag so the next .drawio preview injects afresh. Only
        // success is cached.
        const fail = (message: string) => {
          loader = null;
          s.remove();
          reject(new Error(message));
        };
        s.onload = () => {
          if (!win.GraphViewer) {
            fail("viewer loaded without defining GraphViewer");
            return;
          }
          // Skipping the MathJax bootstrap also skips its definition of this
          // hook, which the viewer calls for a math-enabled diagram. Labels
          // with TeX in them stay plain text rather than throwing.
          if (win.Editor) win.Editor.MathJaxRender ??= () => {};
          resolve();
        };
        s.onerror = () => fail("viewer failed to load");
        document.head.appendChild(s);
      });
    }
    return loader;
  }
</script>

<script lang="ts">
  // Renders a .drawio file with diagrams.net's own standalone viewer, vendored
  // into the bundle (never fetched). The viewer reads a div's data-mxgraph
  // config and replaces it with an interactive canvas: page tabs for
  // multi-page files, zoom, layers. It also decompresses draw.io's
  // base64+deflate page payloads itself, so the raw file text is handed over
  // untouched.
  import { api } from "../../lib/api";
  import PreviewLoading from "./PreviewLoading.svelte";

  let { relPath }: { relPath: string } = $props();

  let xml = $state<string | null>(null);
  let error = $state<string | null>(null);
  let host = $state<HTMLDivElement | null>(null);

  let generation = 0;
  $effect(() => {
    const path = relPath;
    const mine = ++generation;
    xml = null;
    error = null;
    void (async () => {
      try {
        const [raw] = await Promise.all([api.readFile(path), loadViewer()]);
        if (mine !== generation) return;
        xml = raw;
      } catch (e) {
        if (mine === generation) error = `Couldn't open this diagram — ${e}.`;
      }
    })();
  });

  // Re-render whenever the xml lands or the host div (re)mounts.
  $effect(() => {
    const el = host;
    const content = xml;
    if (!el || content === null) return;
    el.innerHTML = "";
    const div = document.createElement("div");
    div.className = "mxgraph";
    div.dataset.mxgraph = JSON.stringify({
      xml: content,
      toolbar: "pages zoom layers",
      "toolbar-position": "top",
      "auto-fit": true,
      resize: true,
      nav: true,
    });
    el.appendChild(div);
    try {
      win.GraphViewer?.createViewerForElement(div);
    } catch (e) {
      error = `Couldn't render this diagram — ${e}.`;
    }
  });
</script>

<div class="wrap">
  {#if error}
    <div class="note">{error}</div>
  {:else if xml === null}
    <PreviewLoading label="Opening diagram…" />
  {/if}
  <!-- The viewer measures this container to fit the diagram, so it needs a
       height before it draws; hidden (not absent) keeps that measurement out
       of the loading state. -->
  <div class="host" bind:this={host} hidden={xml === null || error !== null}></div>
</div>

<style>
  .wrap {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }
  .host {
    flex: 1;
    min-height: 320px;
    padding: 12px;
    /* Diagram strokes and text are authored against a light canvas, so the
       sheet stays white even in the dark theme. */
    background: #fff;
  }
  .note {
    text-align: center;
    color: var(--danger);
    font-size: 13px;
    padding: 20px;
  }
</style>
