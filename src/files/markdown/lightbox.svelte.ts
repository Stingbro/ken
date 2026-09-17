/**
 * The Markdown editor's lightbox state.
 *
 * A single overlay serves both kinds of artwork in a document: a rendered
 * Mermaid diagram (handed over as cloned SVG markup, so the overlay never
 * shares a node with the live preview) and an image (handed over as the
 * already-resolved `src`).
 */

/** What the overlay is showing. */
export type LightboxContent =
  | { kind: "svg"; svg: string; title?: string }
  | { kind: "img"; src: string; alt?: string };

class LightboxStore {
  content = $state<LightboxContent | null>(null);

  get open(): boolean {
    return this.content !== null;
  }

  show(content: LightboxContent): void {
    this.content = content;
  }

  close(): void {
    this.content = null;
  }
}

/** The app's single lightbox; `MarkdownEditor` mounts the component for it. */
export const lightbox = new LightboxStore();
