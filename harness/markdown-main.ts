// Dev-only harness: mounts MarkdownEditor in a plain browser page so the
// editor can be driven and screenshotted without launching Tauri.
// `?md=<url>` picks the fixture; fixtures are served from harness/decks/
// (gitignored). Add `?theme=dark` to exercise the dark palette, and
// `?rel=<path>` to set the document's project path (default `sample.md`).
import { mount } from "svelte";
import "../src/app.css";
import MarkdownEditor from "../src/files/MarkdownEditor.svelte";
import ContextMenu from "../src/lib/ui/ContextMenu.svelte";
import { drag } from "../src/files/dnd.svelte";
import { installDropGuard } from "../src/lib/dropGuard";
import { docDir, type AttachmentAdapter } from "../src/files/markdown/attachments";

const params = new URLSearchParams(location.search);
const md = params.get("md") ?? "/sample.md";
const relPath = params.get("rel") ?? "sample.md";
document.documentElement.dataset.theme = params.get("theme") ?? "light";

const res = await fetch(md);
if (!res.ok) throw new Error(`${res.status} fetching ${md}`);
const initial = await res.text();

// App.svelte mounts the shared context menu once at the app root; the harness
// has to do the same or the editor's right-click menus have nowhere to render.
mount(ContextMenu, { target: document.body });
// Likewise the window-level guard against stray OS file drops.
installDropGuard(window);

// Stand-in for `write_new_file` / `media_src`: "saved" files live in memory
// as blob URLs, deduped like the backend (`name-1.png`), so pasted and dropped
// files go through the same relative-path code as in the app.
const saved = new Map<string, string>();
const attachments: AttachmentAdapter = {
  async save(file) {
    const dir = docDir(relPath);
    const dot = file.name.lastIndexOf(".");
    const stem = dot > 0 ? file.name.slice(0, dot) : file.name;
    const ext = dot > 0 ? file.name.slice(dot) : "";
    let target = "";
    for (let n = 0; ; n++) {
      const name = n === 0 ? file.name : `${stem}-${n}${ext}`;
      target = dir ? `${dir}/${name}` : name;
      if (!saved.has(target)) break;
    }
    saved.set(target, URL.createObjectURL(file));
    return target;
  },
  resolve(target) {
    const url = saved.get(target);
    if (!url) throw new Error(`not saved: ${target}`);
    return url;
  },
};

const w = window as unknown as Record<string, unknown>;
// Exposed so Playwright can fake a drag from the file tree and inspect saves.
w.__drag = drag;
w.__saved = saved;

mount(MarkdownEditor, {
  target: document.getElementById("app")!,
  props: {
    initial,
    relPath,
    attachments,
    onchange: (markdown: string) => {
      // Exposed so Playwright can assert on the serialized document.
      w.__markdown = markdown;
    },
  },
});
