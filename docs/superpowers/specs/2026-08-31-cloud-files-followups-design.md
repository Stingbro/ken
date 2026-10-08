# Cloud-files follow-ups: .bak ignore, cloud-image OCR, drawio preview, name-search regression test

**Date:** 2026-08-31
**Status:** Approved

## Context

The AT&T AI Op Model project lives in OneDrive. 118 files are indexed as
`cloud_only` (bytes not on disk, marked `SF_DATALESS`). Reviewing that set
surfaced four items:

1. 34 of the rows are `*.xlsx.bak-<timestamp>Z` backups written by an external
   tool — noise that inflates the cloud-only count.
2. 19 cloud-only images never get OCR'd because the background hydration
   worker only pulls text-bearing documents, so their bytes never arrive.
3. `.drawio` diagrams have no preview (they fall through to
   `FallbackPreview`).
4. Filename search over cloud-only files must keep working (verified working
   today; needs a pinning test).

## 1. Ignore `.bak` backup files

Built-in junk rule in `crates/ken-core/src/scan.rs`, alongside
`is_office_lock_name`:

- `is_backup_name(name)` — true when the filename contains a `.bak` suffix
  segment: it ends with `.bak`, or contains `.bak-` (covers
  `Foo.xlsx.bak-2026-07-16T11-55-35-773Z`). Case-insensitive.
- Applied in the scan walker's `filter_entry` and in the watcher's
  single-path reindex, exactly like office lock files, so `.bak` files never
  enter the index and watcher churn on them is free.
- No migration needed: the scan's removal pass already deletes indexed rows
  that no longer appear in the walk, so the 34 existing rows disappear on the
  next scan and the footer's cloud-only count drops.

## 2. OCR for cloud-only images

The OCR pipeline exists (macOS Vision bridge, background queue,
`MAX_OCR_IMAGE_BYTES` = 25 MB cap, SVG excluded) and local images are already
enqueued by `index_one`. The only gap: `wants_background_index` in
`crates/ken-core/src/bg_hydrate.rs` rejects images (`!kind.has_content()`),
so cloud-only images are never hydrated and never reach OCR.

Change `wants_background_index` to also accept:

- `FileKind::Image`, not SVG (`is_vector_image`), `0 <= size <=
  scan::MAX_OCR_IMAGE_BYTES`.

Everything downstream is untouched: the background worker hydrates the file,
the rescan sees bytes present (`needs_retry` on `cloud_only` + not dataless),
`index_one` re-indexes and enqueues OCR by the existing rule. Documents keep
their existing `MAX_BACKGROUND_BYTES` cap; images use the OCR cap.

## 3. Drawio preview (bundled viewer)

- Vendor draw.io's standalone viewer (`viewer.min.js`, GraphViewer) into the
  frontend bundle, loaded from local app assets only — never the network,
  consistent with the app's offline posture. Pin the version; note the
  Apache-2.0 license in the vendored file's header.
- New `src/files/previews/DrawioPreview.svelte`: reads the file's text via
  the existing file-read API, hands the XML to GraphViewer. The viewer
  natively handles deflate+base64-compressed content and multi-page files
  (built-in page tabs). Reuse `PreviewLoading` and the size gate.
- Route in `src/files/PreviewPane.svelte` by `ext === "drawio"` (extension
  routing, like `.ipynb`/`.url`, since the backend kind is coarse).
- Indexing companion: add `FileKind::Drawio` in
  `crates/ken-core/src/extract.rs` with `has_content() == true`. Extraction
  decompresses each `<diagram>` payload (base64 → raw-deflate → URI-decode)
  when compressed, then strips tags and keeps element label text/attribute
  values, so diagram labels are content-searchable. Malformed or
  undecodable payloads degrade to metadata-only, never `failed` retries.
  As a text-bearing kind under the size cap, cloud-only `.drawio` files are
  picked up by the background hydration worker automatically.

## 4. Filename search for cloud-only files — regression test

Verified against the live index: `upsert_file` writes `name_tokens(rel_path)`
into the FTS `name` column even when `text` is empty, and `Db::search` adds a
query-time rel_path substring pass, so `cloud_only` rows are name-searchable
today. Add a regression test in `crates/ken-core/src/db.rs` tests: upsert a
row with `STATUS_CLOUD_ONLY` and empty text, assert `search` finds it by a
filename token and by a mid-word substring. No behavior change.

## Testing

- Unit: `is_backup_name` accept/reject table (`a.bak`, `a.xlsx.bak-<ts>`,
  reject `bakery.md`, `a.baker`); `wants_background_index` image cases
  (png accepted, svg rejected, oversized rejected, video still rejected);
  drawio label extraction (compressed and uncompressed fixtures, malformed
  input); cloud-only name-search regression.
- App: build and open the AI Op Model project — `.bak` rows gone from the
  count, a `.drawio` file renders in preview, a cloud-only image eventually
  gains OCR text (background worker) — via the `verify` skill.

## Out of scope

- OCR on Windows (the Vision bridge is macOS-only today; unchanged).
- Background hydration of videos (unchanged; on-open only).
- User-configurable ignore patterns (the `.bak` rule is built-in, like
  `~$` lock files and junk dirs).
