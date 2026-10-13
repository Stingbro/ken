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

- **Slice 5: kinds and roles** (`52ec843`). Decisions 17, 18, 19, and the summary cap the lead decided.
  - Kinds: `ContentType::Decision` (a page named `DECISIONS.md`, or in a `decisions` folder) and `ContentType::Note` (a page in `Inbox/`, `Ingested/`, `Ingestion/` or `notes/`; a meeting note there is a note). `authority::label_of` reads the kind from `contenttype::of`; the `decisions.md` special case is gone. The search overlay's kind filter, the MCP `type` argument and `api.ts` have both.
  - Tests are not indexed: `kenignore::TEST_PATTERNS` in the built-ins (`test/`, `tests/`, `__tests__/`, `e2e/`, `*.test.[jt]s(x)`, `*.spec.[jt]s(x)`, `*_test.go`, `*_test.py`, `test_*.py`, `*_test.rs`, `*_spec.rb`, `*Test.java`, `*Tests.cs`…). A `!` line in `.kenignore` brings them back. The semantic rebuild now folds the built-ins too. Tests already indexed leave on the next scan, like any newly ignored path (said in the v24 migration and the scan comment).
  - Code roles (`coderole.rs`):
    - The built-in core, no model: controller, service, repository, handler, model, component, view, route, middleware, config, migration, util, script, store, client. Read from the last word of the file name (`PaymentService`, `user_controller`), else the nearest folder (`services/`, `controllers/`).
    - Per-repo rules, pattern to role, under the repo's `repos` entry in the knowledge base's `.ken/knowledge.json` (`"roles": { "src/net/**": "protocol" }`); the longest pattern wins, the core fills in. Only code files have a role.
    - `knowledge_settings.rs` finds the settings a source reads: its own `.ken/knowledge.json`, else the one in a registered repo whose `repos` lists it (nearest on disk when several do). Ken writes nothing into a code repo.
    - Stored in `files.role` by every scan (`coderole::refresh`), by `refresh_path` and by `scan_paths`. In the names field and the embedded header (`chunker::embed_text` takes a role: `src/pay/PayService.java › PayService.settle (service)`). A changed role rewrites the file's names and drops its vectors, so the next rebuild embeds it again.
  - Repo analysis (`repo_analysis.rs`): after set-up's Confirm (both forms) and `set_project_kind`, a background pass over the open workspace's sources. For each source with a kind, a knowledge base that lists it and no analysis saved yet: one Claude call (`assistant::oneshot_answer`, no tools) over its file list (up to 600 code files and pages, sampled evenly, pages with their titles). It proposes role rules for code and reference repos and up to 5 controls for every source. Saved into `.ken/knowledge.json` only where none are saved (`knowledge_settings::save_proposals`), then the source is scanned again. Without Claude nothing is asked and the core roles apply; never the local model.
  - One label per hit: `HybridHit.label` and `RoutedHit.label`, for every hit; `HitPage.label` is gone. Pages keep `kind · tier · date`. Other files say `kind · role or tier · date`, the date from the stored mtime (`authority::file_label`: `code · service · 2026-10-01`, `config · reference · 2026-09-14`). The MCP result line leads with the label (`[code · service · 2026-10-01] path`), with the old page notes only where there is no label. The search overlay shows the label in place of the kind. The chat guide says so.
  - Vocabulary: rewrites come from the Vocabulary page and the decisions log only. A page's frontmatter title and aliases are in the names field of its own chunks (`db::names_extra`, with the role); `set_page_meta` rewrites them when they change. `Db::page_aliases` is gone. `vocab::add_rename` (and the pure `with_rename`) adds a row to the Renames table of `Reference/Vocabulary.md`, making the table when there is none, for slices 7 and 8 to call.
  - Controls (`controls.rs`): `controls: [{ question, expect, source }]` in `.ken/knowledge.json`; no `source` means the repo holding the file. A control passes when its file is in the top 5 of `routing::search_member` (`controls::TOP`). Run after every semantic rebuild with the question embedded (`engine.rs`), and at the end of a scan by keyword while the source has no meaning index. Results stored in the index (`meta` `controls`). The template's `knowledge.json` gains `controls` and notes on roles and controls.
  - One health line: `db::IndexHealth` folds the old `IndexHealth` and `FileHealthCounts`: files, indexed, failed, skipped (name only: minified, binary, cloud placeholder, video with no transcript), queued, capped, chunks, embedded, controls passed of total, model missing (set by the caller from `embedder::selected_profile`). The extraction counts are dropped. `IndexHealth::line` is the MCP `list_projects` line; the `index_health` command sums a team's repos (`TeamIndexHealthDto`), and Your day says it in one line.
  - Summary cap (carried over from slice 4): at most 20 summary calls to Claude in any rolling hour across the app (`summary::take_call`, `MAX_CALLS_PER_HOUR`), on top of the background pace and the `backgroundExtraction` switch. Slice 11's throttle replaces it.
  - Schema v24: `files` gains `role`; `chunks_fts` is filled again so the names carry page titles, aliases and roles; the stored vocabulary is dropped and built again without page aliases. `backfill_page_meta` no longer rewrites names (it runs inside older migrations; v24 fills them).
  - Choices:
    - Proposals are written straight into `.ken/knowledge.json`, where a person edits them; there is no confirm step yet (see Open for Chris).
    - A code hit's date is the file's mtime, not its last commit: no git call per hit.
    - Tests come back with a `.kenignore` line only; index rules in `.ken/knowledge.json` come with slice 11 (decision 34).
    - A control's file must be in the top 5. Roles are lowercase words (`legacy protocol`).
  - The compile check is clean apart from the known warnings. `npm run check`: 0 errors, 8 warnings (the known ones).
  - Not run: any test, and the app. The v24 migration refills the keyword index once on first open. The first scan after it drops indexed tests, works out every code file's role (dropping those files' vectors, so the next rebuild re-embeds all code with a role), and runs the controls by keyword. The first set-up or kind change after this makes one Claude call per source that has a knowledge base and no analysis yet.

## Open for Chris

- **How a retired page names its successor.** Decision 7 says a retired page names its successor, but decision 5's six keys have no field for it. For now `status: superseded` retires a page and the brief says to put a banner naming the replacement in its text; `replaced_by` is read from older pages only. Choose: keep the banner, or add a seventh key (say `superseded_by`) that Ken writes and search shows.

- **The title weight in plain-word questions.** The breadcrumb field counts 2.5 times the body in the any-word fallback too, where names were cut to a quarter because one common word in a name outranked the body. If a common heading word ("Overview", "Notes") starts winning plain questions, lower `W_TITLE` there; it needs a measurement first (decision 38 left measurement to be designed).
- **Confirming what the repo analysis proposes.** Role rules and controls are written into `.ken/knowledge.json` as soon as Claude proposes them, applied on the next scan, and a person edits them there (the settings screen is slice 12). Choose: keep that, or hold them as a proposal card in the Inbox until a person accepts them.

## Next: slice 6, code index (26, 27)

Nothing has been edited yet.

- [ ] Rename "code map" to "code index" (26): the module `codemap.rs` (to `codeindex.rs`, with its uses in `chunker.rs`, `db.rs`, `engine.rs`, `scan.rs`, `summary.rs`, `src-tauri/src/lib.rs`), `Db::backfill_code_map` and `CODE_MAP_BACKFILLED` (keep the stored meta key so an index is not filled twice), the comments, `api.ts` and `src/files/CodeMap.svelte` (and `EditorPane.svelte`'s use of it). The tables (`code_symbols` …) keep their names.
- [ ] Each code hit names its symbol and role and offers the next calls (26): the hit already carries its role in its label; add the chunk's `symbol` to `HybridHit` / `RoutedHit` (from `chunks.symbol`, read with the line) and to `RoutedHitDto` and `api.ts`. The MCP result line says `symbol` and, for a code hit, the next calls by name (`find_usages <symbol>`, `file_outline <path>`, `related_files <path>`; slice 10 renames them `code_usages`, `code_outline`, `code_imports`).
- [ ] Ken's chat card shows the symbol (26): where the chat shows a search hit or citation (`src/chat/ToolCard.svelte`, `toolCard.ts`), show the symbol beside the path.
- [ ] Plain words match symbol names for code questions (26): "save world" finds `saveWorld`, `save_world`, `SaveWorld`. In `routing::search_member`, a query of two to four words is also looked up as one joined name in the code index's definitions (`Db::code_symbols` by a case- and separator-insensitive name), and the chunks holding those definitions join the keyword hits. A test in `routing.rs`.
- [ ] "Use before grep" (26): the MCP tool descriptions of `find_definition`, `find_usages`, `file_outline` and `related_files`, and the code part of the chat guide (`chat.rs`), say to use the code index before Grep and Glob; the server `instructions` follow in slice 10.
- [ ] One git helper shared by history and drift (27): a `git.rs` module in ken-core with the quiet, no-prompt runner (`proc::quiet`, `GIT_TERMINAL_PROMPT=0`) and the log reader the MCP `history` tool uses (`crates/ken-mcp/src/main.rs`, the `git log` runs around `history`); `drift.rs`'s `git()` and its other `Command::new("git")` calls, `wikinew::repo_facts` and `day.rs`'s git call use it. Commits citing tickets or rulings as graph edges wait for slice 9 (decision 21's graph); leave a function that lists a commit range's cited ids for it.
- [ ] Tests to update or add: the renamed module's tests, the symbol on a routed hit, the joined-name lookup through `routing`, the git helper (a temp repo, as `drift.rs`'s tests do).

## Later slices

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
