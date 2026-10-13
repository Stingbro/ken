# Knowledge core: to do

What's left of the rebuild on `knowledge-core`. The numbers point to [DECISIONS.md](DECISIONS.md).

The approach is **import, then remove.** The whole `knowledge-layer` feature set was brought in first. Each slice then reshapes or removes code to match the decisions. Each slice ends with a compile check, a commit and a push. A full verification pass comes at the end.

## How to pick it up

- **Build check (compile only):**
  - Rust: `scripts\win-build.cmd cargo check --release --workspace --all-targets --message-format short`, run from the checkout root.
  - Frontend: `npm run check`, with its output redirected to a file. Piping it through PowerShell exits with code 144.
- **Known warnings:**
  - `hooks.rs`: an unreachable pattern.
  - `ocr`: an unused attribute.
  - `src-tauri/src/lib.rs`: an unused `profile` field.
  - `svelte-check`: 8 warnings.
- **A fresh checkout:** run `scripts\win-build.cmd sidecar` once before the Rust check, or the app's build script stops on the missing `ken-mcp` binary.
- **Tests:** don't run test binaries or anything that spawns shells without asking first. Endpoint security on the dev machine flags it.

## Done

- **Slice 1a: one repo, the new sections, a smaller template** (`89536c3`). Decisions 3, 4, 6, 8, 35.
  - The knowledge base holds `tickets/`, `Decisions/DECISIONS.md`, `.ken/knowledge.json` and `log.md`.
  - The team-repo setup step is gone, along with `teamnew.rs` and `setup_create_team_repo`.
  - `pagemeta::Section` is {Rules, Decisions, Current, Design, Reference, Research}. Legacy folders still map: ways-of-working and conventions → Rules, platform → Reference, work → Current.
  - `Roles::of_repo` reads `.ken/knowledge.json`, falling back to `.wright/team.json`.
  - Ingest writes rulings to `Decisions/DECISIONS.md`, and new rules go in `Rules/`.
- **Slice 2a: five tiers and the nudges removed** (`5373ec3`). Decisions 23, 24.
  - `authority::Tier` is {Binding, Current, Reference, Record, Superseded}, with weights +0.75 / +0.4 / 0 / −0.5 / −1.25.
    - Done and closed tickets are Record.
    - Cancelled, wontfix, duplicate and rejected tickets are Superseded.
    - Notes (`Inbox/`, `Ingested/`, `notes/`) and drafts are Record.
  - The people role is gone. The legacy names `rule` and `closed` parse as binding and superseded.
  - A page hit's label is `kind · tier · date`: `decision D-410 · binding · 2026-10-06`, `ticket · superseded · 2026-07-21`.
    - A ruling marked `[SUPERSEDED BY]`, or found superseded by the pass, reads superseded.
    - A ticket's own status is no longer on the label.
    - A code hit has no label yet; its role comes in slice 5.
    - The chat guide, the MCP tests and `api.ts` use the new form.
  - Removed: the band fallback and tie-break, the staleness and template nudges, `query_intent`, `find_symbols`, `follow_links` and `W_REFERENCE_ASSET`. The blob penalty stays.
  - Index time:
    - `Templates/` is a built-in exclusion.
    - Minified code and data (a line over 2,000 characters with under 3% whitespace) are searchable by name only.
  - The compile check is clean apart from the known warnings.
  - `npm run check` was not run: the worktree had no `node_modules`, and the only frontend change is a doc comment.
- **Slice 2b: routing, and one index** (`0a9d8d2`). Decisions 20, 15, and the title control of 19.
  - `routing::plan_route` searches every source; a source named in the question, or a repo or group the person chose (`RoutePlan::chosen`), narrows it.
    - A reference repo is one whose kinds are all Reference; a repo whose kind is not said yet is searched like any other.
    - It is held back in `RoutePlan::reserve` unless the question is about the platform (`asks_about_platform`) or names it.
    - "Nothing else answers" means every searched source came back empty after the filters; the reserve is then searched and merged too (`needs_reserve`).
  - One merge (`merge_routed`): by score, ties to the hit's rank in its own source, then the plan's order (registry or manifest order).
  - Removed: the graph planner (with `rank_projects_for_entities` and `strongly_related` in `workspace_kg_db.rs`), the 5-repo cap, ordering by recent activity, `KB_LEAD` and the knowledge-base share, the RRF tie-break and the `kg://` breadcrumbs.
  - Reported, never dropped (`routing::search_target`, `MemberStatus`):
    - Building: the vector extension is there but the meaning index is not built. The source is searched by keyword and reported with `embedded` of `chunks` passages.
    - Unavailable: its index did not open.
    - Slow: its search took 2 s or more (`SLOW_MILLIS`). There is no time budget; every source is waited for.
    - The app's coverage notes, `route_query` and `list_projects` say so.
  - One index (15):
    - Schema v21 drops the `search` FTS table, its triggers and `contents.name`. `Db::search`, `SearchHit` and its ranker (file-name tiers, phrase bonus, recency blend) are gone.
    - Commands removed: `search`, `hybrid_search`, `search_all_projects`. "This repo" in ⌘K is `route_search` pinned to the focused member, so the overlay's search-only badge went with `hybrid_search` (decision 34 makes indexing two-state).
    - The quick answer searches through `routing::search_member`.
    - MCP: `search_knowledge` is removed now rather than in slice 10; `semantic_search` and `route_query` stay until slice 10 makes them `search`. The chat guide and allowed list follow.
    - `crates/ken-core/tests/qa_probe.rs`, a probe of the old index, is deleted (38).
  - The title-query control is retired (19): `scan::control_query`, `ControlResult` and `IndexHealth.control` are gone. Slice 5 adds the new controls.
  - What only the old index did, now done by the chunk index:
    - OCR text is chunked when it lands (`mark_ocr_done`).
    - A move (`rename_prefix`) moves the chunks, their keyword rows' path words, page facts, tiers, links and the code map.
    - A reindex drops the chunks of files that are gone (`drop_orphan_chunks`), keeping the vectors of the rest.
  - Lost: a file with no text (a cloud placeholder, a failed, binary or minified file) is no longer found by its name alone. Restored in slice 3.
  - The compile check is clean apart from the known warnings. `npm run check`: 0 errors, 8 warnings (the known ones).
  - Not run: any test, and the app. The v21 migration rewrites `contents` once (`DROP COLUMN`) on first open, which may take a moment on a large index.
- **Slice 3: frontmatter, six keys** (`3a018a1`). Decision 5, with two carry-overs from 2b the lead decided.
  - `pagemeta::PageMeta` reads `title`, `aliases`, `status`, `updated`, `sources` and `checked`, plus an optional `reviewed_by`.
    - `checked` is `pagemeta::Checked`, a date and a commit. The one written form is `checked: 2026-10-13 f00218c`; `@` or a comma between them also reads, and a date alone is a check with no commit.
    - `status` is draft | current | superseded. `retired` and `deprecated` still read as superseded. A ticket keeps its own statuses (decision 8). `date` still reads as `updated`.
    - A list of reviewers reads as one line (`Ana, Ben`).
  - Dropped: `audience` everywhere (the type and helpers, `HitPage.audience`, `route_search`'s argument and fetch widening, the MCP property and `audience_arg`, the overlay's reader buttons, drift's business-page split and `DriftRun.business_pages`, `audience: business` in the note template); `verified` (it is not read at all now: an old stamp counts for nothing until a check writes `checked`); `HitPage.band` and `pagemeta::band`. `lens`, `changed_by` and `pin` were never read; only a test names them, to show they are ignored.
  - Drift, the part this slice needs: every page with sources is measured alike. A checked page is pinned at its checked commit where the cited repo has it, else at the last commit on its checked date. A page never checked keeps its citations' own `repo@sha` pins, else its `updated` date. The age rule and its finding say "checked"; the Team tag is "unchecked".
  - Schema v22 makes `page_meta` again (`checked`, `reviewed_by`; no `verified`, `audience`), fills it from the stored text, and refreshes the tiers. The table and that fill moved out of v14 and v19, which no longer touch it.
  - Proposals drop `checked:`, `reviewed_by:` and `verified:` from a draft. The ingest edit prompt keeps `checked:` and `reviewed_by:` lines.
  - Templates: `Finding.md` and `Ingested-note.md` say `status: current`. The note's `source:` became a `sources:` link (`[[Research/Ingestion/Ingested/…/file]]`), which `finish_note` writes; who was there moved into the body as a `Present:` line, read by `ingest::present_of` (the older frontmatter key still reads) for `kind_of` and the card. The template README lists the six keys.
  - Carry-over 1: a file with no readable text (a cloud placeholder, a failed, binary, minified or empty file) has one chunk holding its path (`chunker::name_chunk`), so the names column and the path rerank find it by name again.
    - Its own chunks replace it when it gains text (scan or OCR), and it goes with the file. A move rewrites its text to the new path.
    - The semantic rebuild now walks every file: indexed ones are chunked again, the rest keep their chunks and only have them embedded, so the embedded count can reach the chunk count.
  - Carry-over 2: `replaced_by`, `superseded_by` and `generated` are read as legacy keys, never written and not offered in templates or the UI, so older pages keep their rank and label.
  - Choices:
    - `checked`'s commit is one sha for the page. A page citing several repos is pinned at that commit only in the repo that has it; the others fall back to the checked date.
    - `kind:` on a note (set by the ingest prompt, read by `kind_of` and the card) is not one of the six keys. It stays until slice 7 reworks ingest.
  - The compile check is clean apart from the known warnings. `npm run check`: 0 errors, 8 warnings (the known ones).
  - Not run: any test, and the app. The v22 migration rebuilds `page_meta` and the tiers once on first open, from stored text.

## Open for Chris

- **How a retired page names its successor.** Decision 7 says a retired page names its successor, but decision 5's six keys have no field for it. For now `status: superseded` retires a page and the brief says to put a banner naming the replacement in its text; `replaced_by` is read from older pages only. Choose: keep the banner, or add a seventh key (say `superseded_by`) that Ken writes and search shows.

## Next: slice 4, chunking (16)

Nothing has been edited yet.

- [ ] Code chunks by definition (`chunker.rs` `chunk_code` is line blocks today):
  - Cut a code file at the definitions the code index already parses (`codemap::map_file`, `code_symbols` with `line`, `end_line`, `docs`). The scan has the map before it chunks; pass it in rather than parsing twice.
  - Each chunk carries its enclosing symbol and line range. Store the symbol on the chunk (a `symbol` column on `chunks`, or in the names field); `line` stays the start.
  - Line blocks only where there is no grammar (`codemap::Lang::of` is None). JSON keeps its key-outline headers (`with_json_headers`).
  - A definition longer than the chunk size is cut at line ends inside it, each part keeping the symbol.
- [ ] Code chunk fields: path, symbol, doc comment and role. Path and symbol go in the names field (`db::fts_names` already takes path words and declared names); the doc comment goes in the title field below. Role is empty until slice 5 and must not block it.
- [ ] Heading breadcrumb on every prose chunk (`chunk_prose`, `chunk_log`): page title (frontmatter `title`, else the first `# `) › the headings above the chunk.
  - Embedded with the chunk: prepend the breadcrumb to the text that is embedded, not to the text shown as the snippet. Decide whether that means a separate embed-text or storing it in the new column and joining at embed time (`engine.rs`, `chunks_missing_vectors`).
  - A new FTS5 column `title` on `chunks_fts`, weighted about 2–3× the body in `bm25(chunks_fts, …)` (`search_chunks_fts`, `W_NAMES_ANY`). Schema v23 remakes `chunks_fts` (`rebuild_chunks_fts`) and marks prose files for one re-read, as v16 and v17 did.
- [ ] One-line summaries for wiki pages, notes, decisions and tickets only (not code), cached by chunk `content_hash`, used for retrieval only (in the title field or embed text, never shown as the snippet).
  - They need a model. Decide the writer (the Claude CLI at write time, or the local model in the background throttle of decision 34) and keep it off by default until the throttle exists; the slice can land the table and the hook with no writer.
- [ ] Report the per-file chunk cap in index health: count files cut at `CHUNK_CAP` (200) or `LOG_CHUNK_CAP`, and carry the count in `IndexHealth` / `file_health_counts` and the health line (`api.ts` follows).
- [ ] Re-chunking keeps vectors where text is unchanged (`upsert_chunks` diffs by hash); the breadcrumb changes every prose chunk's embed text once, so the first rebuild after v23 re-embeds them. Say so in the migration comment.
- [ ] Tests to update or add: `chunker.rs` (code by definition, breadcrumbs, the cap), `db.rs` (`fts_names`, the title column and its weight, the v23 migration), `engine.rs` (embed text), `routing.rs` (a breadcrumb or symbol hit ranks), `scan.rs` (the cap count).


## Later slices

- [ ] **5, kinds and roles (17, 18, 19):**
  - Add the `decision` and `note` kinds.
  - Tests are not indexed by default.
  - Code roles: a built-in core plus per-repo rules proposed by a repo analysis.
  - Query rewrites from Vocabulary and the decisions-log aliases only.
  - Controls in `.ken/knowledge.json`.
  - One health line.
- [ ] **6, code index (26, 27):**
  - Rename "code map" to "code index" (`codemap.rs` and the UI strings).
  - Each hit names its symbol and role and offers the next calls.
  - Plain words match symbol names.
  - "Use before grep" in the tool text.
  - One git helper shared by history and drift.
- [ ] **7, ingest rework (9–14):**
  - Context from the real ranker.
  - Contradiction checks against the wiki, the log, the tickets and the code, each a finding.
  - Section-replacement edits with the hold rules.
  - One commit per ingest.
  - Claim a source when its note is written.
  - One ruling entry format.
  - The note headings and where each goes.
  - Cut: idea files, escalation files, Your day tasks, method-change tickets, "me" matching.
  - A note's `kind:` frontmatter (set by the ingest prompt, read by `kind_of` and the card) is not one of decision 5's keys: fold it into the kind folder or the body.
- [ ] **8, drift, lint and supersession (25, 28–30):**
  - Drift runs on each default-branch move, plus a weekly age rule.
  - It stamps `checked`, or re-reads and fixes. A ruling becomes a finding.
  - One commit and a `log.md` line per run.
  - One config.
  - The lint passes.
  - Supersession markers read on every scan, with Claude on new entries only.
  - Remove the Cited-in-Code page.
- [ ] **9, project graph and Timeline (21, 37):**
  - Graph edges from exact sources only, plus topic tags at write time.
  - Expose it as `related`.
  - Remove the Map and Explore.
  - The Timeline becomes the raid log.
- [ ] **10, MCP tools (22):**
  - The tools: `search`, `read`, `related`, `code_definition`, `code_usages`, `code_outline`, `code_imports`, `history`, `tickets`, `propose_decision`, `add_source`, `repos`.
  - Server `instructions` from the brief (1).
  - Memory, task and family tools move off the knowledge server.
- [ ] **11, setup and building (32–34):**
  - The three-step setup.
  - The Files tab splits into Wiki and Sources.
  - The build queue with two passes.
  - Map an existing wiki after a dry run.
  - Index rules in one place.
  - One throttle and one status panel.
  - Don't carry over `wikidraft.rs`, `wikiverify.rs` or `checkout.rs`.
- [ ] **12, Wright leftovers (35–38):**
  - Remove escalations, families and the team bus, OpenSpec, the people, owner and roster code (`people.rs`), and the evals and `KEN_EVAL_*` hooks.
  - The Team screen becomes workspace settings.
  - Keep Your day, tasks and memory.
- [ ] **13, the method pages in WoW:** finish the remaining pages on `docs/knowledge-core` in the WoW repo.
- [ ] **14, verify:**
  - The full test suite (ask first, see above).
  - An end-to-end run: set up a new knowledge base, ingest a source, search, drift on a commit.
