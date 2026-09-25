// Dev-only harness: mounts MarkdownEditor in a plain browser page so the
// editor can be driven and screenshotted without launching Tauri.
// `?md=<url>` picks the fixture; fixtures are served from harness/decks/
// (gitignored). Add `?theme=dark` to exercise the dark palette.
import { mount } from "svelte";
import "../src/app.css";
import MarkdownEditor from "../src/files/MarkdownEditor.svelte";
import ContextMenu from "../src/lib/ui/ContextMenu.svelte";

const params = new URLSearchParams(location.search);
const md = params.get("md") ?? "/sample.md";
document.documentElement.dataset.theme = params.get("theme") ?? "light";

const res = await fetch(md);
if (!res.ok) throw new Error(`${res.status} fetching ${md}`);
const initial = await res.text();

// App.svelte mounts the shared context menu once at the app root; the harness
// has to do the same or the editor's right-click menus have nowhere to render.
mount(ContextMenu, { target: document.body });

mount(MarkdownEditor, {
  target: document.getElementById("app")!,
  props: {
    initial,
    onchange: (markdown: string) => {
      // Exposed so Playwright can assert on the serialized document.
      (window as unknown as Record<string, unknown>).__markdown = markdown;
    },
  },
});
