# Cloud-files Follow-ups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ignore `.bak` backup files, background-hydrate cloud-only images so they reach OCR, preview and index `.drawio` diagrams, and pin filename search over cloud-only files with a regression test.

**Architecture:** Three small ken-core changes (a junk-name rule in the scanner/watcher, a broadened background-hydration predicate, a new `.drawio` file kind with label extraction) plus one frontend preview component that wraps draw.io's vendored standalone viewer. No new pipelines: everything rides the existing scan → background-hydrate → OCR/extract machinery.

**Tech Stack:** Rust (ken-core, rusqlite/FTS5, quick-xml, flate2, base64), Svelte 5 (runes) + Tauri 2 frontend, vendored `viewer-static.min.js` from diagrams.net.

**Spec:** `docs/superpowers/specs/2026-08-31-cloud-files-followups-design.md`

## Global Constraints

- Never read a cloud placeholder's bytes implicitly (`cloud::is_placeholder` guards stay as they are).
- No network at runtime except the existing model downloader; the drawio viewer JS is vendored into the repo, never fetched by the app.
- Rust tests: `cargo test -p ken-core --no-default-features` (whisper/llama features need long native builds; the touched code is feature-independent).
- Frontend tests: `npm test` (vitest). Frontend build check: `npm run check`.
- Commit after each task; message style `feat(scope): …` / `test(scope): …` matching recent history.
- Delegated implementation in this repo runs on Opus subagents (user preference).

---

### Task 1: Ignore `.bak` backup files in scanner and watcher

**Files:**
- Modify: `crates/ken-core/src/scan.rs` (near `is_office_lock_name`, ~line 73; `filter_entry` ~line 125; `refresh_path` excluded check ~line 264; tests module)
- Modify: `crates/ken-core/src/watch.rs:135-145` (`relevant_path`)

**Interfaces:**
- Produces: `pub fn is_backup_name(name: &str) -> bool` in `ken_core::scan` — later tasks don't consume it, but the watcher does.

- [ ] **Step 1: Write the failing tests**

In the `#[cfg(test)] mod tests` at the bottom of `scan.rs`, add:

```rust
#[test]
fn backup_names_are_junk() {
    // Timestamped backups written by external tools next to live documents.
    assert!(is_backup_name("ATT Op Model Dev V2.xlsx.bak-2026-07-16T11-55-35-773Z"));
    assert!(is_backup_name("notes.md.bak"));
    assert!(is_backup_name("NOTES.MD.BAK")); // case-insensitive
    // Words that merely contain "bak" are not backups.
    assert!(!is_backup_name("bakery-roadmap.md"));
    assert!(!is_backup_name("a.baker"));
    assert!(!is_backup_name("bak")); // no dot — not a suffix segment
}

#[test]
fn scan_skips_backup_files() {
    let dir = tempfile::tempdir().unwrap();
    let project = Project::create(dir.path(), "Fixture").unwrap();
    std::fs::write(dir.path().join("live.md"), "# keep me").unwrap();
    std::fs::write(dir.path().join("live.md.bak-2026-07-16T11-55-35-773Z"), "old").unwrap();
    let mut db = Db::open_in_memory().unwrap();
    scan(&project, &mut db).unwrap();
    let rels: Vec<String> = db.list_files().unwrap().into_iter().map(|f| f.rel_path).collect();
    assert_eq!(rels, vec!["live.md"]);
}
```

If the existing test module lacks `Project`/`Db` imports for this, mirror how the neighboring scan tests construct them (they already use `Project::create` + `Db::open_in_memory`).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ken-core --no-default-features scan::tests::backup -- --nocapture`
Expected: FAIL to compile — `is_backup_name` not found.

- [ ] **Step 3: Implement the rule and wire it in**

In `scan.rs`, directly below `is_office_lock_name`:

```rust
/// Timestamped backup files (`report.xlsx.bak`, `report.xlsx.bak-2026-07-16T…Z`)
/// written by external tools beside live documents. Pure noise in a knowledge
/// index — and in a cloud folder they pile up as cloud-only rows that inflate
/// the offline count — so they are ignored everywhere, like Office lock files.
pub fn is_backup_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".bak") || lower.contains(".bak-")
}
```

Wire it into the scan walker's `filter_entry` (the closure currently checking `.ken` / lock / junk names):

```rust
            name != ".ken"
                && !is_office_lock_name(&name)
                && !is_backup_name(&name)
                && !(e.path().is_dir() && is_junk_dir_name(&name))
```

Into `refresh_path`'s excluded check:

```rust
    let excluded = project.is_excluded(rel)
        || is_hidden_rel(rel)
        || rel.rsplit('/').next().is_some_and(|n| is_office_lock_name(n) || is_backup_name(n));
```

And into `watch.rs` `relevant_path`'s per-component check, alongside the lock-file test:

```rust
                s.starts_with('.')
                    || crate::scan::is_junk_dir_name(s)
                    || crate::scan::is_office_lock_name(s)
                    || crate::scan::is_backup_name(s)
```

No migration step: the scan's removal pass deletes indexed rows absent from the walk, so existing `.bak` rows disappear on the next scan.

- [ ] **Step 4: Run the ken-core tests**

Run: `cargo test -p ken-core --no-default-features`
Expected: PASS (including the two new tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ken-core/src/scan.rs crates/ken-core/src/watch.rs
git commit -m "feat(scan): ignore .bak backup files like Office lock files"
```

---

### Task 2: Background-hydrate cloud-only images so they reach OCR

**Files:**
- Modify: `crates/ken-core/src/scan.rs` (make `is_vector_image` public, ~line 57)
- Modify: `crates/ken-core/src/bg_hydrate.rs` (`wants_background_index`, module docs, tests)

**Interfaces:**
- Consumes: `scan::MAX_OCR_IMAGE_BYTES: i64`, `scan::is_vector_image(&str) -> bool` (public after this task).
- Produces: no new symbols; `wants_background_index(row, excluded)` now also returns true for raster images ≤ 25 MB.

- [ ] **Step 1: Write the failing tests**

In `bg_hydrate.rs` tests (reusing the existing `row(rel, status, size)` helper):

```rust
/// A cloud-only raster image is worth pulling down: once local, the rescan
/// re-indexes it and the OCR queue makes its pixels searchable.
#[test]
fn selects_a_cloud_only_image_for_ocr() {
    assert!(wants_background_index(
        &row("decks/arch-diagram.png", scan::STATUS_CLOUD_ONLY, 4096),
        false,
    ));
    assert!(wants_background_index(
        &row("photos/whiteboard.JPG", scan::STATUS_CLOUD_ONLY, 4096),
        false,
    ));
}

/// Images the OCR pipeline itself would reject are not worth the bandwidth:
/// vector SVGs (Vision can't rasterize them) and files over the OCR size cap.
#[test]
fn rejects_images_ocr_would_skip() {
    assert!(!wants_background_index(
        &row("logos/brand.svg", scan::STATUS_CLOUD_ONLY, 512),
        false,
    ));
    assert!(!wants_background_index(
        &row("scans/huge.png", scan::STATUS_CLOUD_ONLY, scan::MAX_OCR_IMAGE_BYTES + 1),
        false,
    ));
}
```

Also extend the existing video-rejection test (or confirm it still passes unchanged) — videos must stay rejected.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p ken-core --no-default-features bg_hydrate -- --nocapture`
Expected: `selects_a_cloud_only_image_for_ocr` FAILS (images rejected by `!kind.has_content()`); the svg/oversize test may pass vacuously — that's fine, it pins the boundary.

- [ ] **Step 3: Implement**

In `scan.rs`, change `fn is_vector_image` to `pub fn is_vector_image` (doc comment unchanged).

In `bg_hydrate.rs`, replace the kind/size tail of `wants_background_index`:

```rust
    let kind = FileKind::from_path(Path::new(&row.rel_path));
    if kind == FileKind::Video {
        return false;
    }
    // Raster images carry no extractable text (has_content() is false) but the
    // OCR pipeline makes their pixels searchable — worth pulling down under the
    // same limits OCR itself enforces (no SVG, ≤ MAX_OCR_IMAGE_BYTES).
    if kind == FileKind::Image {
        return !scan::is_vector_image(&row.rel_path)
            && row.size >= 0
            && row.size <= scan::MAX_OCR_IMAGE_BYTES;
    }
    if !kind.has_content() {
        return false;
    }
    // A negative or oversized placeholder is skipped: nothing sane to download.
    row.size >= 0 && (row.size as u64) <= MAX_BACKGROUND_BYTES
```

Update the module doc's first paragraph ("cloud-offline *documents*") to say documents *and OCR-able images*. Then grep the app-layer worker for a second kind filter that would undo this:

Run: `grep -n "pending_documents\|has_content" src-tauri/src/lib.rs`
Expected: the worker consumes `pending_documents` only; no extra kind filtering. If any exists, align it with the new predicate.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ken-core --no-default-features`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ken-core/src/scan.rs crates/ken-core/src/bg_hydrate.rs
git commit -m "feat(cloud): background-hydrate cloud-only images so OCR can index them"
```

---

### Task 3: Regression test — cloud-only files stay name-searchable

**Files:**
- Modify: `crates/ken-core/src/db.rs` (tests module, near `search_finds_and_highlights`)

**Interfaces:**
- Consumes: `Db::open_in_memory()`, `Db::upsert_file(rel, kind, size, mtime, status, error, text)`, `Db::search(query, limit)`, `crate::scan::STATUS_CLOUD_ONLY`.
- Produces: nothing — pins existing behavior.

- [ ] **Step 1: Write the test (expected to pass immediately — it's a pin, not TDD of new behavior)**

```rust
/// A cloud placeholder is indexed with empty text but must remain findable by
/// name: `upsert_file` writes name tokens into FTS even for contentless rows,
/// and the query-time rel_path LIKE pass catches mid-word fragments. This
/// pins the guarantee scan.rs relies on ("Name-searchable; content arrives
/// when the user opens the file").
#[test]
fn cloud_only_rows_are_name_searchable() {
    let mut db = Db::open_in_memory().unwrap();
    db.upsert_file(
        "meetings/Standup Recording 06.15.mp4",
        "video", 999, 0, crate::scan::STATUS_CLOUD_ONLY, None, "",
    )
    .unwrap();
    // Whole-token match via the FTS name column.
    let hits = db.search("standup", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].status, crate::scan::STATUS_CLOUD_ONLY);
    // Mid-word fragment via the filename substring pass.
    let hits = db.search("cording", 10).unwrap();
    assert_eq!(hits.len(), 1, "filename LIKE pass must catch mid-word fragments");
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p ken-core --no-default-features cloud_only_rows_are_name_searchable -- --nocapture`
Expected: PASS. If it fails, STOP — that's a real bug in `upsert_file`/`search`; investigate before proceeding, do not adjust the test to fit.

- [ ] **Step 3: Commit**

```bash
git add crates/ken-core/src/db.rs
git commit -m "test(db): pin name-search over cloud-only (contentless) rows"
```

---

### Task 4: `.drawio` file kind with label extraction

**Files:**
- Create: `crates/ken-core/src/drawio.rs`
- Modify: `crates/ken-core/src/lib.rs` (add `pub mod drawio;` alongside the other module declarations)
- Modify: `crates/ken-core/src/extract.rs` (FileKind variant + dispatch)
- Modify: `crates/ken-core/src/scan.rs` (`index_one`: empty-label drawio → metadata_only)
- Modify: `crates/ken-core/Cargo.toml` (add `flate2 = "1"`, `base64 = "0.22"`)

**Interfaces:**
- Consumes: `quick_xml` (already a dep), new deps `flate2`, `base64`.
- Produces:
  - `FileKind::Drawio` with `as_str() == "drawio"`, `from_path` on ext `drawio`, `has_content() == true`.
  - `pub fn drawio::extract_labels(raw: &str) -> String` — human text (diagram names + cell labels), newline-joined; empty string on malformed/undecodable input (never an error).

- [ ] **Step 1: Add the deps**

In `crates/ken-core/Cargo.toml` `[dependencies]`, after `quick-xml`:

```toml
# .drawio label extraction: diagrams store deflate(raw)+base64 payloads.
flate2 = "1"
base64 = "0.22"
```

- [ ] **Step 2: Write the failing tests**

Create `crates/ken-core/src/drawio.rs` with the tests first (module body can be a stub returning `String::new()` so tests compile and fail):

```rust
//! `.drawio` (diagrams.net) label extraction.
//!
//! An mxfile wraps one `<diagram>` per page. A page's content is either
//! inline `<mxGraphModel>` XML or a compressed payload:
//! base64( raw-deflate( percent-encode( xml ) ) ). The searchable text of a
//! diagram is its page names and the `value` attributes of its cells (which
//! may themselves contain HTML markup to strip). Malformed input yields an
//! empty string — a diagram we can't decode is metadata-only, never an error
//! the scanner would retry.

pub fn extract_labels(raw: &str) -> String {
    String::new() // stub — replaced in Step 4
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNCOMPRESSED: &str = r#"<mxfile><diagram name="Page-1">
        <mxGraphModel><root>
          <mxCell id="0"/><mxCell id="1" parent="0"/>
          <mxCell id="2" value="API Gateway" vertex="1"/>
          <mxCell id="3" value="&lt;b&gt;Auth&amp;nbsp;Service&lt;/b&gt;" vertex="1"/>
          <mxCell id="4" value="" edge="1"/>
        </root></mxGraphModel></diagram></mxfile>"#;

    #[test]
    fn extracts_page_names_and_cell_labels() {
        let text = extract_labels(UNCOMPRESSED);
        assert!(text.contains("Page-1"));
        assert!(text.contains("API Gateway"));
        // HTML in labels is stripped to its words, entities decoded.
        assert!(text.contains("Auth Service"), "got: {text:?}");
        assert!(!text.contains("<b>"));
    }

    #[test]
    fn extracts_compressed_diagram_payloads() {
        // Build a compressed page the way draw.io does:
        // percent-encode → raw deflate → base64.
        use base64::Engine;
        use flate2::write::DeflateEncoder;
        use flate2::Compression;
        use std::io::Write;
        let inner = r#"<mxGraphModel><root>
            <mxCell id="2" value="Billing%20Cutover" vertex="1"/>
        </root></mxGraphModel>"#;
        // draw.io percent-encodes BEFORE deflating; our fixture's inner text
        // already carries the %20 that decoding must turn into a space.
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
        enc.write_all(inner.as_bytes()).unwrap();
        let payload = base64::engine::general_purpose::STANDARD.encode(enc.finish().unwrap());
        let raw = format!(r#"<mxfile><diagram name="Flow">{payload}</diagram></mxfile>"#);
        let text = extract_labels(&raw);
        assert!(text.contains("Flow"));
        assert!(text.contains("Billing Cutover"), "got: {text:?}");
    }

    #[test]
    fn malformed_input_degrades_to_empty() {
        assert_eq!(extract_labels("not xml at all"), "");
        assert_eq!(extract_labels("<mxfile><diagram>!!!not-base64!!!</diagram></mxfile>"), "");
        assert_eq!(extract_labels(""), "");
    }
}
```

Add `pub mod drawio;` to `crates/ken-core/src/lib.rs` next to the other `pub mod` lines.

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p ken-core --no-default-features drawio -- --nocapture`
Expected: the first two tests FAIL (stub returns "").

- [ ] **Step 4: Implement `extract_labels`**

Replace the stub in `drawio.rs`:

```rust
use quick_xml::events::Event;
use quick_xml::Reader;

pub fn extract_labels(raw: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut reader = Reader::from_str(raw);
    reader.config_mut().trim_text(true);
    let mut in_diagram = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                if e.name().local_name().as_ref() == b"diagram" {
                    // Only an open <diagram> can carry a compressed text
                    // payload as its child text node.
                    in_diagram = true;
                }
                collect_attr_labels(&e, &mut out);
            }
            Ok(Event::Empty(e)) => collect_attr_labels(&e, &mut out),
            Ok(Event::End(e)) if e.name().local_name().as_ref() == b"diagram" => {
                in_diagram = false;
            }
            // A compressed page is the diagram element's text payload.
            Ok(Event::Text(t)) if in_diagram => {
                if let Ok(txt) = t.unescape() {
                    if let Some(inner) = decode_payload(txt.trim()) {
                        // Recurse over the decoded mxGraphModel XML.
                        let nested = extract_labels(&inner);
                        if !nested.is_empty() {
                            out.push(nested);
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break, // malformed: keep whatever was gathered
            _ => {}
        }
    }
    out.join("\n")
}

/// Page names (`name` on `<diagram>`) and cell labels (`value`/`label`
/// attributes) — the only attributes that carry human text.
fn collect_attr_labels(e: &quick_xml::events::BytesStart, out: &mut Vec<String>) {
    let name = e.name();
    let is_diagram = name.local_name().as_ref() == b"diagram";
    for attr in e.attributes().flatten() {
        let key = attr.key.local_name();
        if (is_diagram && key.as_ref() == b"name")
            || key.as_ref() == b"value"
            || key.as_ref() == b"label"
        {
            if let Ok(v) = attr.unescape_value() {
                push_clean(out, &v);
            }
        }
    }
}

/// base64 → raw deflate → percent-decode. None when any stage fails.
fn decode_payload(payload: &str) -> Option<String> {
    use base64::Engine;
    use std::io::Read;
    if payload.is_empty() {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(payload).ok()?;
    let mut xml = String::new();
    flate2::read::DeflateDecoder::new(bytes.as_slice())
        .read_to_string(&mut xml)
        .ok()?;
    Some(percent_decode(&xml))
}

/// Minimal %XX decoder (draw.io percent-encodes the XML before deflating).
/// Invalid escapes pass through untouched; '+' is NOT a space in this scheme.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let (Some(h), Some(l)) = (
                bytes.get(i + 1).and_then(|b| (*b as char).to_digit(16)),
                bytes.get(i + 2).and_then(|b| (*b as char).to_digit(16)),
            ) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Strip HTML tags and decode the handful of entities draw.io emits in rich
/// labels, then push the cleaned text if anything is left.
fn push_clean(out: &mut Vec<String>, value: &str) {
    let mut text = String::with_capacity(value.len());
    let mut in_tag = false;
    for c in value.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                text.push(' ');
            }
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    let text = text
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if !text.is_empty() {
        out.push(text);
    }
}
```

Note for the implementer: this is a working sketch, but the tests are the contract — if quick-xml 0.38's API differs in detail (e.g. `config_mut().trim_text`), adapt the code, not the tests.

- [ ] **Step 5: Run the drawio tests**

Run: `cargo test -p ken-core --no-default-features drawio -- --nocapture`
Expected: PASS (all three).

- [ ] **Step 6: Wire the FileKind**

In `extract.rs`:

- Add `Drawio,` to `enum FileKind` (after `Url`), with doc comment `/// A diagrams.net drawing; its searchable content is its text labels.`
- `as_str`: `FileKind::Drawio => "drawio",`
- `from_path`: add `"drawio" => FileKind::Drawio,` (before the `_ => Binary` arm). Note: `.drawio` must NOT match the `xml` Code arm — it won't, the extension is `drawio`.
- `has_content()` stays `!matches!(self, FileKind::Image | FileKind::Binary)` — Drawio is content-bearing with no change.
- Dispatch in `extract()`:

```rust
        FileKind::Drawio => extract_drawio(path),
```

and beside `extract_url`:

```rust
/// A drawio file's searchable content is its diagram labels. Undecodable
/// content yields empty text (→ metadata_only in the scanner), never an error.
fn extract_drawio(path: &Path) -> Result<Extracted> {
    let bytes = fs::read(path).map_err(|e| Error::io(path, e))?;
    Ok(Extracted {
        text: crate::drawio::extract_labels(&String::from_utf8_lossy(&bytes)),
        title: None,
    })
}
```

In `scan.rs` `index_one`, extend the video empty-text arm's neighborhood with a drawio arm (before the generic `has_content` arm):

```rust
        // A drawio file whose labels couldn't be decoded is searchable by name
        // only — metadata-only, not an empty-content "indexed" row.
        Ok(out) if kind == FileKind::Drawio && out.text.trim().is_empty() => {
            (STATUS_METADATA_ONLY, None, out.text)
        }
```

Add a test in `extract.rs`'s test module:

```rust
#[test]
fn drawio_classifies_and_extracts_labels() {
    assert_eq!(FileKind::from_path(Path::new("d/arch.drawio")), FileKind::Drawio);
    assert_eq!(FileKind::Drawio.as_str(), "drawio");
    assert!(FileKind::Drawio.has_content());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("arch.drawio");
    std::fs::write(&path, r#"<mxfile><diagram name="P1"><mxGraphModel><root>
        <mxCell id="2" value="Data Lake" vertex="1"/>
    </root></mxGraphModel></diagram></mxfile>"#).unwrap();
    let out = extract(&path).unwrap();
    assert!(out.text.contains("Data Lake"));
}
```

Existing rows note: already-indexed local `.drawio` files keep kind `binary` until their mtime changes or a full reindex — acceptable; the preview routes by extension (Task 5) so it works regardless.

- [ ] **Step 7: Run the full ken-core suite**

Run: `cargo test -p ken-core --no-default-features`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/ken-core/Cargo.toml Cargo.lock crates/ken-core/src/drawio.rs crates/ken-core/src/lib.rs crates/ken-core/src/extract.rs crates/ken-core/src/scan.rs
git commit -m "feat(extract): index .drawio diagrams by their text labels"
```

---

### Task 5: Drawio preview with the vendored diagrams.net viewer

**Files:**
- Create: `src/files/previews/vendor/drawio-viewer.min.js` (vendored, see Step 1)
- Create: `src/files/previews/vendor/drawio-viewer-LICENSE.txt`
- Create: `src/files/previews/DrawioPreview.svelte`
- Modify: `src/files/PreviewPane.svelte` (import + route `ext === "drawio"`)
- Modify: `src/files/previews/sizeGate.ts` (+ its test `sizeGate.test.ts`)

**Interfaces:**
- Consumes: `api.readFile(relPath): Promise<string>` (existing), `PreviewLoading` component, `isPreviewTooLarge`.
- Produces: `DrawioPreview.svelte` with props `{ relPath: string }`; `previewFormat` now returns `"drawio"` for `.drawio` paths and `PreviewFormat` gains `"drawio"`.

- [ ] **Step 1: Vendor the viewer**

```bash
curl -fsSL https://viewer.diagrams.net/js/viewer-static.min.js \
  -o src/files/previews/vendor/drawio-viewer.min.js
ls -la src/files/previews/vendor/drawio-viewer.min.js  # expect ~1.5-2 MB
```

Prepend a header comment to the file recording source URL, fetch date (2026-08-31), and `shasum -a 256` of the original. Create `drawio-viewer-LICENSE.txt` beside it containing the Apache-2.0 notice for jgraph/drawio (copy the license header from https://github.com/jgraph/drawio — it's Apache-2.0). The file is loaded from the app bundle only; the app never fetches it at runtime.

- [ ] **Step 2: Size-gate `.drawio` (test first)**

In `sizeGate.test.ts`, add:

```ts
it("gates .drawio at the default cap", () => {
  expect(previewFormat("d/arch.drawio", "binary")).toBe("drawio");
  expect(isPreviewTooLarge("d/arch.drawio", "binary", PREVIEW_CAP_BYTES + 1)).toBe(true);
  expect(isPreviewTooLarge("d/arch.drawio", "drawio", 1024)).toBe(false);
});
```

Run: `npm test -- sizeGate` → FAIL. Then in `sizeGate.ts`: add `"drawio"` to `PreviewFormat`, and in `previewFormat` add `if (ext === "drawio") return "drawio";` next to the ipynb extension match (extension, not kind: stale indexes still call these rows "binary"). `capForFormat` needs no change (default cap). Re-run → PASS.

- [ ] **Step 3: Write `DrawioPreview.svelte`**

```svelte
<script lang="ts">
  // Renders a .drawio file with diagrams.net's own standalone viewer, vendored
  // into the bundle (never fetched). The viewer reads a div's data-mxgraph
  // config and replaces it with an interactive canvas: page tabs for
  // multi-page files, zoom, pan. It also decompresses draw.io's
  // base64+deflate page payloads itself, so the raw file text is handed over
  // untouched.
  import { api } from "../../lib/api";
  import PreviewLoading from "./PreviewLoading.svelte";
  import viewerUrl from "./vendor/drawio-viewer.min.js?url";

  let { relPath }: { relPath: string } = $props();

  let xml = $state<string | null>(null);
  let error = $state<string | null>(null);
  let host = $state<HTMLDivElement | null>(null);

  // The viewer script defines window.GraphViewer once per webview; subsequent
  // previews reuse it. A single module-level promise keeps double-injection out.
  let loader: Promise<void> | null = null;
  function loadViewer(): Promise<void> {
    if ((window as any).GraphViewer) return Promise.resolve();
    if (!loader) {
      loader = new Promise((resolve, reject) => {
        const s = document.createElement("script");
        s.src = viewerUrl;
        s.onload = () => resolve();
        s.onerror = () => reject(new Error("viewer failed to load"));
        document.head.appendChild(s);
      });
    }
    return loader;
  }

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
      (window as any).GraphViewer.createViewerForElement(div);
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
    background: var(--paper);
  }
  .host {
    flex: 1;
    min-height: 0;
    padding: 12px;
  }
  .note {
    text-align: center;
    color: var(--danger);
    font-size: 13px;
    padding: 20px;
  }
</style>
```

Implementer notes (verify against the vendored file, adapt if its API differs):
- `GraphViewer.createViewerForElement(div)` is the documented per-element entry point; `GraphViewer.processElements()` is the scan-the-document fallback.
- If the viewer draws at zero height, give `.host` an explicit `min-height` — the viewer measures its container.

- [ ] **Step 4: Route it in `PreviewPane.svelte`**

```svelte
import DrawioPreview from "./previews/DrawioPreview.svelte";
```

and in the chain, after the `.url` branch:

```svelte
{:else if ext === "drawio"}
  <DrawioPreview {relPath} />
```

(The `tooLarge` branch already sits first, and Task 5 Step 2 made `.drawio` size-gated, so a giant file shows `TooLargeNotice` before this branch is reached.)

- [ ] **Step 5: Frontend checks**

Run: `npm test` → PASS. Run: `npm run check` → no new errors.

- [ ] **Step 6: Verify in the running app**

Use the project's `verify` skill (build + launch the Tauri app). In the AI Op Model project:
- Open `05 - Technical/Architecture/MGO-OS-Architecture Diagram.drawio` — it's cloud-only, so expect hydrate-then-render; the diagram should draw with page tabs if multi-page.
- Confirm the `.bak` rows are gone: the Archive folder shows no `.bak-…` files and the footer's cloud-only count dropped by ~34.
- If the viewer script is blocked, check `csp` in `src-tauri/tauri.conf.json` — a bundled asset should be allowed by `'self'`; adjust only if the console shows a CSP refusal.

- [ ] **Step 7: Commit**

```bash
git add src/files/previews/vendor src/files/previews/DrawioPreview.svelte src/files/PreviewPane.svelte src/files/previews/sizeGate.ts src/files/previews/sizeGate.test.ts
git commit -m "feat(files): preview .drawio diagrams with the vendored diagrams.net viewer"
```
