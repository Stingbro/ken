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

- **Slice 4: chunking** (`740940c`). Decision 16.
  - Code by definition (`chunker::chunk_with`): a file with a grammar is cut at the definitions the code index parsed. The scan makes the map before it chunks and passes it in; the semantic rebuild reads the stored one (`code_symbols`).
    - The lines above a definition (comments, attributes, imports) go with it. Small neighbours share a chunk up to the target size; a definition too long for one is cut at its members, else at its line ends, each part keeping the symbol.
    - Line blocks only where there is no grammar or the parse found no definition. JSON keeps its key-outline headers.
  - Each chunk carries `end_line`, a `title` and, for code, its `symbol`: `Store.open`, or `save, load` (up to five, then `…`) for packed top-level definitions. Path and symbol are in the names field (`fts_names`); a code chunk's title is its doc comment. Role is left to slice 5.
  - Breadcrumbs: every prose chunk's title is page title (frontmatter `title`, else the first `# `) › the headings above it. A top heading that repeats the title is said once.
  - Search: `chunks_fts` gains a `title` field, weighted 2.5 times the body (`W_TITLE`) in both the all-words and the any-word query.
  - Embedding: what is embedded is `chunker::embed_text`: the breadcrumb, or path › symbol and the doc comment, then the summary, a blank line, the chunk. The snippet is the chunk alone. The content hash covers title and symbol (`chunk_hash`); a chunk with neither keeps its old hash and vector.
  - The cap in index health: `Chunked::capped` → `files.chunk_capped` → `FileHealthCounts.capped` → the team index health, `api.ts` and the Your day line ("N files too long to index whole").
  - One-line summaries (`summary.rs`), per the lead's ruling:
    - Claude writes them, never the local model: batches of up to 40 passages (48 KB) per call through `assistant::oneshot_answer` (the same CLI path as the map, with no tools).
    - Only Markdown pages of a knowledge-base repo (kind team or wiki): pages, notes, decisions, tickets. Not code, not `Inbox/` or `Ingestion/Raw/`.
    - Cached by chunk content hash (`chunk_summaries`); only chunks with no cached summary are sent. A passage the reply skips is cached empty, so it is not asked again. Summaries of text no chunk holds are pruned.
    - Retrieval only: in the `title` field at once, and in the embedded text the next time the chunk is embedded. Never shown.
    - Run by `extraction_worker` (`summarize_by_claude`): one batch per turn while chunks wait, behind `wait_for_extraction_slot` / `EXTRACT_PACING`, off with `backgroundExtraction`, ten minutes' backoff after a failed call. Slice 11 folds it into the build queue.
  - Schema v23: `chunks` gains `end_line`, `title`, `symbol`; `files` gains `chunk_capped`; `chunk_summaries` and an index on `chunks.content_hash` are made; `chunks_fts` is made again with `title` (the rebuild moved here from v16). Prose and code with a grammar are read again once by the next scan.
  - Choices:
    - Vectors are not dropped when a summary arrives later; the summary joins the embedding on the chunk's next embed.
    - No bigger chunks for long code (prose has them): a code file past 200 chunks loses its end and is counted.
  - The compile check is clean apart from the known warnings. `npm run check`: 0 errors, 8 warnings (the known ones).
  - Not run: any test, and the app. The first scan after v23 re-reads every prose and code file, and the next semantic rebuild re-embeds their chunks. The first background turns after that send a knowledge base's pages to Claude in batches until every chunk has a summary.

## Open for Chris

- **How a retired page names its successor.** Decision 7 says a retired page names its successor, but decision 5's six keys have no field for it. For now `status: superseded` retires a page and the brief says to put a banner naming the replacement in its text; `replaced_by` is read from older pages only. Choose: keep the banner, or add a seventh key (say `superseded_by`) that Ken writes and search shows.

- **How fast summaries may spend Claude.** The first pass over a knowledge base sends every page chunk to Claude, 40 to a call, with only the 2 s background pace between calls (about 125 calls for 5,000 chunks, back to back). Choose: leave it to the pace and the `backgroundExtraction` switch, or add an hourly cap until slice 11's throttle (Gentle / Normal / Fast) exists.
- **The title weight in plain-word questions.** The breadcrumb field counts 2.5 times the body in the any-word fallback too, where names were cut to a quarter because one common word in a name outranked the body. If a common heading word ("Overview", "Notes") starts winning plain questions, lower `W_TITLE` there; it needs a measurement first (decision 38 left measurement to be designed).

## Next: slice 5, kinds and roles (17, 18, 19)

Nothing has been edited yet.

- [ ] The `decision` and `note` kinds (17): add `ContentType::Decision` and `ContentType::Note` (`contenttype.rs`). `Decisions/DECISIONS.md` (and any log `chunker::is_log` reads) is a decision; `Inbox/`, `Ingested/`, `notes/` and `Research/Ingestion/` are notes. `authority::label_of` then reads the kind from `contenttype::of` instead of its `decisions.md` special case. `api.ts` and the search filters follow.
- [ ] Tests are not indexed by default (17): a built-in rule in `kenignore::built_in_rule_sets` for test paths (the `contenttype` test dirs and names: `tests/`, `__tests__/`, `*_test.go`, `*.test.ts`, `test_*.py`…). A `!` line in `.kenignore`, or an index rule in `.ken/knowledge.json`, brings them back. Mark test files for removal on first open (or let the next scan drop them), and say so in the migration or scan comment.
- [ ] Code roles (17): a code file's kind is its role.
  - A small built-in core (service, controller, repository, model, handler, component, util, config…) read from the path and file name, no model.
  - Per-repo role rules (glob → role) in `.ken/knowledge.json`, read at index time beside the tier overrides (`authority::Roles`), editable.
  - A repo analysis proposes rules when a code repo is added: one Claude call over the file tree, the person confirms (the setup flow's repo step; the full flow is slice 11).
  - Store the role per file (a `file_roles` table or a column), put it in the chunk's embedded header (`chunker::embed_text` takes a role) and in the names field. Changing a role re-embeds that file's chunks.
- [ ] One label per hit (17): `kind · tier or role · date`. A code hit gets `code · service · 2026-10-01` (date from git or mtime); pages keep `kind · tier · date`. `authority::hit_label` stops returning None for code; the chat guide, the MCP text and `api.ts` follow.
- [ ] Vocabulary (18): query rewrites (`vocab::Vocabulary`) come from the Vocabulary page and the decisions log's aliases only. Page titles and aliases (`db.page_aliases`) leave the rewrites and go into the names field of the page's chunks instead (`fts_names`, from `page_meta`). Rename rows: ingest and drift add rows to the Vocabulary page; leave a function for slices 7 and 8 to call.
- [ ] Controls (19): a few known questions per source, each with the file it must return near the top, in `.ken/knowledge.json` (`controls: [{ question, expect, source }]`). Run through `routing::search_member` after every rebuild; store pass/fail. Setup proposes them (one Claude call over the source's pages); a person edits them.
- [ ] One health line (19): indexed, failed or skipped; meaning built (`vector_count` of `chunk_count`); controls passing; model missing; files cut at the chunk cap (slice 4). One struct in `db.rs` (fold `IndexHealth` and `FileHealthCounts`), one command, one line in Your day and in the MCP `list_projects`. Drop the extraction counts from it.
- [ ] Tests to update or add: `contenttype.rs` (decision, note), `kenignore.rs` (tests excluded and brought back), `authority.rs` (the code label), `vocab.rs` (no page aliases), `db.rs` (aliases in names, the health struct), a controls test through `routing`.

## Later slices

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
