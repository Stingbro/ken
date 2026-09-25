// Dev-only harness: mounts PdfPreview in a plain browser so a fillable PDF can
// be exercised without launching Tauri. `?pdf=<url>` picks the file (served
// from harness/decks/). Saves POST the re-serialized bytes to the harness dev
// server, which writes them to harness/out/saved.pdf (gitignored) for
// inspection with pypdf or another viewer.
import { mount } from "svelte";
import "../src/app.css";
import { api } from "../src/lib/api";
import PdfPreview from "../src/files/previews/PdfPreview.svelte";

const params = new URLSearchParams(location.search);
const pdf = params.get("pdf") ?? "/form.pdf";
const status = document.getElementById("status")!;
const log = (msg: string) => {
  status.textContent = `${new Date().toISOString().slice(11, 19)} ${msg}`;
  console.log(`[harness] ${msg}`);
};

const patched = api as unknown as Record<string, unknown>;
patched.readFileBytes = async (path: string) => {
  const res = await fetch(path);
  if (!res.ok) throw new Error(`${res.status} fetching ${path}`);
  return res.arrayBuffer();
};
patched.getOcrRegions = async () => [];
patched.saveFileBytes = async (_path: string, bytes: Uint8Array) => {
  const res = await fetch("/__save", { method: "POST", body: bytes });
  if (!res.ok) throw new Error(`${res.status} saving`);
  log(`saved ${bytes.length} bytes → harness/out/saved.pdf`);
  return Date.now() / 1000;
};

mount(PdfPreview, {
  target: document.getElementById("app")!,
  props: {
    relPath: pdf,
    onfillable: () => log("fillable"),
    onchange: () => log("editing…"),
    onsaved: (mtime: number) => log(`onsaved mtime=${mtime}`),
    onerror: (m: string) => log(`ERROR ${m}`),
  },
});
