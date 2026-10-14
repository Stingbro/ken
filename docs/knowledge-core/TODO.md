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
  - `svelte-check`: 6 warnings (since slice 9; the old Timeline screen's two unused-style warnings went with it).
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
    - A move (`rename_prefix`) moves the chunks, their keyword rows' path words, page facts, tiers, links and the code map (the code index since slice 6).
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

- **Slice 6: code index** (`299a665`). Decisions 26, 27.
  - Renamed: `codemap.rs` is `codeindex.rs`; `Db::backfill_code_index`, `set_code_index`, `code_indexed_files` and `CODE_INDEX_BACKFILLED` (its stored meta key stays `code_map_backfilled`, so no index is filled twice); `src/files/CodeIndex.svelte`. Comments, MCP messages and `api.ts` say code index. The tables keep their names.
  - Plain words reach a definition: `codeindex::joined_names` joins every run of two to four significant words in a row (stopwords dropped, at most 12 words read), plus a word that is itself a compound name (`saveWorld` as typed); `Db::definitions_by_joined_name` matches definitions whose name, ASCII lowercase without underscores, is one of them. "save world" finds `saveWorld`, `save_world` and `SaveWorld`. One plain word alone is not looked up.
    - `routing::search_member` makes the chunk holding each such definition (at most 8, one per file, `codeindex::NAMED_MAX`) its file's keyword hit, cited at the definition's own line. "Where is get_current_user defined" leads with its definition at line 4 again (the test slice 2a removed is back in `routing.rs`).
  - Each code hit names its symbol: `HybridHit.symbol` and `RoutedHit.symbol`, from `chunks.symbol` (`Db::chunk_line_symbol`), filled before the rerank. A definition the words named is shown under its own name when its chunk packs several. `RoutedHitDto` and `api.ts` carry it; the search overlay shows it beside the file name.
  - The MCP result line names a code hit's symbol in backticks after its path, and a `next:` line under the snippet offers `find_usages <symbol> · file_outline <path> · related_files <path>` (`codeindex::next_calls`; files the index does not parse get none). `route_query`'s description says so.
  - Use before grep: the four code tools' descriptions and the chat guide say to use the code index before Grep and Glob. The server `instructions` follow in slice 10.
  - Chat card: a search's card lists its hits while folded, each place with its symbol beside it (`cardHits` in `toolCard.ts`, `ToolCard.svelte`); the engine's preview of a search result keeps the hit lines (up to 10) instead of the first eight lines. A code tool's card names the symbol it looked up (`summarize_tool` reads `name`).
  - One git helper (`git.rs`): `command`, `output`, `run` and `global_config`, quiet and never prompting (`GIT_TERMINAL_PROMPT=0`, `GCM_INTERACTIVE=never`), and the log reader `log` / `parse_log`. The MCP `history` tool, drift's `git()` and `fetch`, `wikinew::repo_facts` and its commit, and `day::git_me` use it. `cited_ids` and `commits_citing` list the `PREFIX-number` ids a commit range names, for slice 9.
  - Schema v25: `code_symbols_joined`, an index on definitions by their joined name. Nothing is read again.
  - Choices:
    - The symbol's words count as name words in the rerank, like the path's (the names field holds both). That is the base score of decision 24, not a symbol nudge: there is no flat bonus for a definition.
    - Runs of words anywhere in the question, not only a whole question of two to four words, so "how do we save world" still joins `save world`.
    - `cited_ids` returns candidates by shape; slice 9 keeps the ids the knowledge base has as tickets or rulings (so `UTF-8` drops out there).
    - `sync.rs` and `setup.rs` keep their own git calls: the sync engine is not history or drift, and slice 11 reworks setup.
  - The compile check is clean apart from the known warnings. `npm run check`: 0 errors, 8 warnings (the known ones).
  - Not run: any test, and the app. The v25 migration builds one index on `code_symbols` on first open.

- **Slice 7: ingest rework** (`ec98e85`). Decisions 9 to 14, with 18 (renames) and 36 (Ken's commits).
  - Context from the real ranker (9): `ingest::context` searches the knowledge base with the source's subject (`subject`: frontmatter title, first heading, else file name) and its key terms (`key_terms`: what `db::significant_tokens` keeps, said twice or more, top eight) through `routing::search_member_with` and the workspace vocabulary. Pages, the decisions log and tickets, each headed by its path and label; `Research/Ingestion` and `Templates/` left out. The note and the plan both get it.
  - Contradictions (10): the context also holds code hits (code files only, at most 4 per repo and 10 in all) from every other repo of the open workspace (`CodeSource`; the pass opens their indexes read only) and the definitions of names the source writes like code (`mentioned_names`, `codeindex::definitions_named`). A Contradictions line names what it contradicts in backticks (`` `D-012`: how they differ — "the words" ``); each becomes a finding, and so does each Open Question.
  - One findings list, started (`findings.rs`): Review items of kind `finding` with a `Finding` payload (kind, source note, against and its kind, text, quote). `team_findings` lists them first, so the Inbox shows them with Open the page and Mark as done (`findingItem` names Contradiction and Open question). Slice 8 moves drift and lint onto it.
  - Page edits (11): plan, then edit, then write or hold. An edit is one section (`edit_prompt` asks for it, `finish_section`, `replace_section`); Ken sets `updated:` (`stamp_updated`) and cites the note. Held (`hold`, `Held`): a page changed during the run (held against the person's version), a Rules page, an edit the plan says goes `against` a ruling or a code file, more than a fifth of the page, and a page holding a person's uncommitted work. New pages start from their section's blank (`section_blank`; `page_prompt` and `finish_page` moved here from `proposal.rs`) and join their section's index with one line (`add_to_index`). Caps: `MAX_UPDATES` and `MAX_NEW`, 10 each. `Decisions/`, `tickets/`, `.ken/`, indexes and `log.md` are never edited by a plan.
  - Git is the undo (12): `finish_ingest` adds the `log.md` line (`append_log`, `## [date] ingest | note title`) and makes one commit through `git::commit_paths`: the note, the source's move and every file written, path by path, never `git add -A`, ignored paths left out, a person's staged work left staged, never pushed. The message names the note and lists pages changed and made, held edits, contradictions, open questions, renames and what waits (`commit_message`). The card records the commit or why there is none, and says `git revert`.
  - Removed: the undo records (`Written`, `WriteKind`, `Listed`, `undo_write`, `undo_write_at`, `undo_all`, `undo`, `UndoReport`, `withdraw`), the `ingest_undo`, `ingest_undo_write` and `ingest_read_again` commands, their `api.ts` wrappers and types, and the card's Undo, Undo all and Read again.
  - Claim on write (12): `claim` moves the source beside its note as the note is written; the note is taken back if the source cannot move. `ingest_file` became `ingest_seen`, which marks the card seen and files a source only an older card still holds in Raw (`in_review` now takes the root).
  - One ruling entry format (13): `decisions_entry` writes `## D-nnn · date · topic`, the bold ruling, the quote with who (free text), Why, sources, aliases and supersedes, from a `RulingEntry` the plan fills (`RulingDraft`; a note's own bullets are the fallback). Every parser reads it: `chunker::unbold_id` takes heading marks off an id, `entry_head` reads a dated heading, `chunk_entry` names the entry by its bold ruling, so drift, vocab and supersede follow; `supersede::marked` also reads the `supersedes:` line; a fenced example is never log context. No decider check: one click adds a ready entry (`ruling_is_mine`, `can_accept` and the Inbox's decider lines are gone). The template's `Decisions/DECISIONS.md` shows the format and drops its Log heading.
  - Note headings (14): Summary, Key Takeaways, What It Overturns, Contradictions, What Was Said, Rulings, Actions, Open Questions in the template and the prompt; "Rulings Said in the Room", "Actions and Requests" and "Escalations" still read (an older library's template gives way to the bundled one). Actions become drafted tickets in the knowledge base's `tickets/` (`ticket_text`: id, title, status, type, assignee, updated). Overturns lead the plan's updates.
  - Cut (14): idea files, escalation files, Your day tasks from ingest (`day::BY_INGEST` too), method-change tickets and "me" matching (`is_me`, `Targets`), with their tests, card rows, `api.ts` types and UI. Your day and tasks themselves stay.
  - The note's kind is its folder (`kind_at`); `kind_of` reads Present and the file type for a fresh reply; `finish_note` drops a `kind:` key.
  - Renames (18): the plan lists them and `vocab::add_rename` adds the rows, in the same commit.
  - Applied later: a held edit, a ruling or a ticket applied from the card or the Inbox is its own commit of that page (`ingest::accepted`), only for proposals that came from an ingest. A held new page joins its index then.
  - Choices:
    - A section edit must start with a heading; a heading the page lacks adds that section at its end. An H1 section runs to the next H1, so a page with one H1 is one section; the fifth rule measures changed lines, not the section's size.
    - What a contradiction is against is classed by its name (`against_kind`): `D-nnn` a ruling, a ticket id or `tickets/` path a ticket, Markdown a page, a file with another extension or `repo:path` code.
    - A page with a person's uncommitted work is held. A section index, the Vocabulary page or `log.md` with uncommitted work is written to but left out of the commit, for the person to commit with their own change.
    - Ticket keys come from the knowledge base's existing tickets, else its folder's initials; `type` is left for the team.
  - The compile check is clean apart from the known warnings. `npm run check`: 0 errors, 8 warnings (the known ones).
  - Not run: any test, and the app. Nothing migrates: older cards still read (their undo records are ignored), and a source an older card holds in Raw waits for Seen.

- **Slice 8: drift, lint and supersession** (`c1a652d`). Decisions 25, 28, 29, 30, with 36 (Ken's commits).
  - When drift runs (28): `drift::due` is true when a cited repo's branch head is not the one the last run measured (`branch_moved`, from the run's `PinCache`), a week after the last run (`AGE_RUN_DAYS`, the age rule's turn for pages whose sources do not move), or an hour after a run that left pages waiting for a re-read (`RETRY_SECS`). Gone: `DriftConfig.interval_days`, `DEFAULT_INTERVAL_DAYS` and the interval check. The app's background worker looks every five minutes (`knowledge_checks`, `CHECKS_EVERY`); the open scan no longer sweeps. Run drift now (Team screen) forces a run.
  - What a run does (28), `drift::act` after the sweep (`drift::run` does both and records the run):
    - Whitespace or comments only: the page's `checked:` advances silently (`stamp_checked`: today and the head of the page's first cited repo, 12 hex digits).
    - A real change, or a page past the age rule: Claude re-reads it (`reread_prompt`: the page, then the diff of each changed citation from its pin, or the cited files as they are now) and answers CHECKED or one section. A section goes through the ingest rules (`replace_section`, `stamp_updated`, `ingest::hold`, the size rule measured without Ken's stamps); a held fix is a page proposal from `drift` (`HELD_FROM`), and the page is not read again while it waits (`held_open`). No Claude, the switch off or the hourly cap reached: the page waits (`DriftRun.waiting`).
    - A renamed file: followed when git names one new path between the pin and the branch (`renamed_to`); the citation is rewritten and, when the file's name changed, a Vocabulary rename row added.
    - A ruling whose cited code changed or is gone: a `drift` finding against the ruling; a ruling is never edited. A cited source that is gone and not followed: a `drift` finding against the page. A void run: one finding, nothing stamped or fixed; the next run that holds resolves it.
    - A page with a person's uncommitted work is left for a later run.
  - One commit and a `log.md` line per run that writes (28, 36): `ingest::append_log(root, today, "drift", "N checked, N fixed, N held, N renames followed")` and `git::commit_paths` with the pages stamped, fixed or followed and the Vocabulary page (`drift::commit_message`). `DriftRun` records `stamped`, `fixed`, `held`, `renamed`, `findings`, `waiting`, `commit` and `commit_error`; `Mismatch` keeps its `pin` and `branch`. Applying a held drift edit is its own commit (`ingest::accepted`, "drift: apply the held edit to …").
  - One config (28): `DriftConfig::for_repo` reads `drift` in the knowledge base's `.ken/knowledge.json` (`controlMoved`, `controlClean`, `minPages`, `fetch`) through `knowledge_settings`; `DriftConfig::of` (project.json) is gone and an old `intervalDays` is ignored. Nothing else reads a drift config. The template's `knowledge.json` has `drift: {}` and a note.
  - Lint (29), new module `lint`:
    - On every scan of a knowledge base (`scan::after_scan`, also after a watcher scan that touched a page): broken links, missing pages (opening the first page that asks), ambiguous names and orphans (`lint::orphans`: a page nothing links to, not an index, START-HERE, `log.md`, the brief, the decisions log, a note, a ticket or a template). Filed and resolved by `findings::sync`, at most 50 a kind (`MAX_PER_KIND`).
    - After an edit: an ingest (`finish_ingest`) and a drift run queue the pages they wrote (`queue_after_edit`); one Claude call compares them with their link neighbours, 12 pages at most (`after_edit_next`, `after_edit_done`); each contradiction is a `contradiction` finding from `lint`.
    - Weekly (`weekly_due`, `weekly_next`, `weekly_done`): the binding and current pages, the decisions log left out, read together for contradictions and up to five `suggested-page` findings.
    - Removed: `links::findings` and `links::Finding`.
  - Rulings cited in code (30): `lint::unknown_rulings` reads the code files of the workspace's other repos (read-only indexes, `Db::code_texts_containing`) for comments citing an id in the log's own shape (its ids' prefixes, `git::cited_ids` per line, `code_citations`), and files the ones the log lacks as `unknown-ruling` findings (synced, so a fixed one resolves). Run after each drift run. The graph edges wait for slice 9. The generated Cited-in-Code page: confirmed nothing on the branch writes it; its name is gone from the chunker and db test fixtures (a comment keeps the measurement history).
  - One findings list (29): `Finding` gains `subject` (the page, missing name, later ruling or code line) and `relation`; repeats fold by kind, page and target (`Finding::key`, `findings::file`); `findings::sync` for checks that own their kinds; `findings::ever`; `Finding::page` is the page to open. `team_findings` reads stored findings only (no sweep, no link report); `drift::findings` and `drift::Finding` are gone. The Inbox (`FINDING_KINDS`) and Team screen (`findingLabel`, `orderFindings`) name the new kinds; the dead draft and proposal kinds are gone from both.
  - Supersession wired in (25):
    - Markers on every scan (`supersede::store_marked`, every repo): the pairs each decisions log marks itself are stored, replacing the log's last ones, so search labels them outside any harness and a marker taken out leaves search too.
    - The Claude pass on new entries only (`next_cluster`, `judged`): a cluster is asked about only when it holds an entry the pass has not read (kept in `Db::check_state("supersede_seen")`); one call per worker turn.
    - Each pair is a `supersession` finding (later id as subject, earlier as target, the evidence quoted), not stored as a supersession; a pair the log marks, or one filed before, is not filed again.
    - Confirm (Inbox, `confirm_finding`, `supersede::confirm`, `mark_in_log`) writes `supersedes:` (or `refines:`) on the later entry and `[SUPERSEDED BY D-nnn]` (or `[REFINED BY …]`) on the earlier, in every log form (plain and bold one-line entries, and `## D-nnn · date · topic`), as one commit of the log. Dismiss marks it done.
    - Removed: `supersede::run`, `Report` and `Judge`.
  - Every background Claude call (re-reads, both lints, the pass) goes through the CLI with no tools (`background_claude`), never the local model, behind `wait_for_extraction_slot` / `EXTRACT_PACING`, the hourly cap `summary::take_call` (now shared by every background call, 20 an hour) and `backgroundExtraction`. A refused or failed call backs the checks off ten minutes (`CHECKS_BACKOFF`).
  - `Db::check_state` / `store_check_state` keep the checks' state in `meta` (`check:` keys). No schema change.
  - Choices:
    - A run writes its `log.md` line and commit only when it wrote a page; a run that changed nothing (or only filed findings) leaves no line (see Open for Chris).
    - Ken does not fetch a code repo only to look for a move: it sees a move when the branch moves locally (a person's pull, any fetch), and every run fetches first, so the weekly run catches up.
    - A page past the age rule is re-read by Claude like a real change, against the cited files as they are.
    - The first time the pass sees a log, every entry is new to it, so the whole log is clustered once (at the shared hourly cap).
    - `checked`'s commit is the head of the page's first cited repo (slice 3's one sha per page).
  - The compile check is clean apart from the known warnings. `npm run check`: 0 errors, 8 warnings (the known ones).
  - Not run: any test, and the app. Nothing migrates. The first scan of a knowledge base after this files the scan lint's findings and stores its markers; the worker's first turn makes a drift run when one is due (a cited branch moved since the last sweep, or a week since it) and starts the supersession pass over the whole decisions log, one cluster per turn.

- **Slice 9: project graph and Timeline** (`373cead`). Decisions 21, 37.
  - One graph per knowledge base, from exact sources only (`graph.rs`). Nodes are written `kind:id`: a page, note or ticket by its path in the knowledge base, a ruling by id (`decision:D-012`), a code file or symbol by repo and path (`code:game:src/save.rs`, `symbol:game:src/save.rs#save`), a topic, a repo, and a commit that cites something (`commit:game@1a2b3c4`). Edges live in the knowledge base's own index (`graph_edges`, schema v26), each with what made it (`origin`):
    - the scan (`graph::on_scan`, from `scan::after_scan` in a knowledge base, no model, made again every scan): page links (only when one Markdown page answers), `sources:` citations (`drift::classify`: a code file, a `[[note]]` one page answers, a ruling), each ruling's `in` edge to its log and its `sources:` line, the supersessions the log marks (`supersedes`, `refines`), and note to page `changed` from the ingest cards;
    - code citations: `lint::rulings_in_code` reads the other repos' comments once after each drift run; the ids the log has become `code-cites` edges (`store_code_citations`), the rest stay `unknown-ruling` findings (`unknown_of`);
    - commits: `graph::read_commits` reads each workspace repo (the knowledge base too) from the head it read last time (`check_state` `graph_commits:<repo>`; the newest 500 the first time, `FIRST_COMMITS`), in every checks turn, one git call for a repo whose head has not moved. Only ids the knowledge base has (`known_ids`: tickets by id or file name, log entries) become `commit` edges, so `UTF-8` drops out. `git::commits_citing` takes a limit and returns `git::Citing` (sha, date, subject, ids);
    - imports and usages: read live from each repo's code index when `related` is asked (`codeindex::related`, `codeindex::usages`), never copied.
  - Topic tags at write time only: `finish_ingest` queues the pages it wrote and the note, a drift run its fixed pages (`graph::queue_topics`). The checks turn then makes one Claude call for up to 12 of them (`topics_next`, `topics_done`) through `background_claude`: the CLI with no tools, the shared pace, the hourly cap, `backgroundExtraction`. At most 5 topics a page, lowercase, the topics in use offered first. Stored as `topic` edges, never in frontmatter; a move carries them (`Db::rename_prefix`), a scan drops those of pages no longer on disk, a reindex keeps them.
  - Exposed as `related`: `graph::related(kb, code, node, limit)` gives neighbours by edge kind (`EDGE_ORDER`: supersedes, refines, cites, changed, code-cites, link, commit, imports, uses, in, topic), one hop then two through stored edges, each node once at its nearest.
    - App: `graph_related(path, projectId)` (`api.graphRelated`) feeds a Related row under a page in Files (`PageLinks.svelte`), plain links left out since they show above; a neighbour that is a file opens in its repo.
    - MCP: `related_files` answers from it, for a file of any repo or a ruling or ticket id, with ken:// addresses; the chat guide says so. `kg_search` is gone, from the server, the allowed list and the guide.
  - Removed (21): the background local-model extraction (the `extractions` queue and its `Db` calls, the worker's local path, `profile_prompt_addendum`, the open-time extraction backfill), the Claude map build (`knowledge_model.rs`, `refresh_knowledge_model`, `start_knowledge_build`, `map_by_claude`, `AutoBuildTracker` and its signals), the federation (`federation.rs`, `workspace_kg_db.rs`, the `workspace_kg_*` commands and state, `federated_kg_enabled`), and person-centred entities (the `entities`, `entity_edges` and `events` tables and read models). v26 drops the tables and the build stamp. `content_hash` moved to `db`, `normalize_name` to `routing`. The worker is `background_worker`: summaries and checks only. The `backgroundExtraction` flag's text now says what it governs.
  - The Map and Explore are gone (37): `MapScreen`, `ExploreScreen`, `EntityWikiPanel`, `knowledge.svelte.ts`, `knowledge.ts`, `workspaceKg.svelte.ts`, their api wrappers and events, Your day's map-build listener. `features::BUILT_IN` keeps the name `federatedKg` so an old value is still ignored.
  - The Timeline is the raid log (37): `raidlog::raid_log` lists `log.md` runs (ingest, drift, lint), rulings (dated log entries, the bold ruling), ticket changes (`updated`, else the file's date, with status), the knowledge base's commits and the other repos' commits that cite its ids (from the graph, one entry a commit), findings filed and resolved (timed), and every repo's tags as releases (`git::tags`), newest first, each with its file and line, or its repo and sha or tag. The `raid_log` command reads the workspace's knowledge base (else the focused repo); `TimelineScreen` is its own sidebar screen with kind chips, search and day groups (`timeline.ts`).
  - Choices:
    - The graph lives in the knowledge base's index, not a new store; the edges a code repo's index already holds (imports, usages) are read live rather than copied.
    - Topic tags are their own background call after the write, queued like the lint after an edit, rather than a field in the ingest and re-read prompts (see Open for Chris).
    - A commit is linked only to ids the knowledge base has when it is read; a ticket made later does not reach back (see Open for Chris).
    - The raid log's commits are the knowledge base's own and the other repos' commits that cite a ticket or ruling, not every commit of every repo.
    - `related` goes two hops through stored edges only; the code index is asked for the node itself.
  - The compile check is clean apart from the known warnings. `npm run check`: 0 errors, 6 warnings (the two the old Timeline screen had are gone with it).
  - Not run: any test, and the app. The v26 migration drops the old model's tables on first open. The first scan of a knowledge base makes its edges; the first checks turn reads up to 500 commits per repo (one `git log` each), and the topic queue starts with the next ingest or drift fix.

- **Slice 10: MCP tools** (`4fa5318`). Decision 22, with the server instructions of decision 1.
  - The knowledge server (`ken-mcp`, the default) lists exactly decision 22's twelve, in its order (`knowledge_tools`):
    - `search` replaces `route_query` and `semantic_search`: one tool on `routing`, every repo merged once. `project` (names, ids or folders, comma-separated; a registered repo outside the workspace too) narrows it through `RoutePlan::chosen`, and the route line says "the project asked for". A server locked with `--project` still searches every repo, its own included. Hits keep the label, locator, symbol and `next:` line.
    - `read` replaces `read_document`; given a folder (`.` for the whole repo) it lists the indexed files, so `list_documents` folds in (`list_folder`). `split_address` reads a ken:// address for `read` and `add_source`.
    - `related` is `related_files`. `code_definition`, `code_usages`, `code_outline` are the old three; `code_imports` is new: imports, importers and libraries from `codeindex::related`. `codeindex::next_calls` says `code_usages <symbol> · code_outline <path> · code_imports <path>`.
    - `history` stays. `tickets` replaces `ticket_list`, read only, over the workspace's repos from the registry (no workspace needed).
    - `propose_decision` (decision 13): `ruling` and `why` required; `topic`, `quote`, `who`, `aliases`, `supersedes`, `source` (a note's name, written as its `sources:` line). It is filed in the knowledge base's Inbox as ingest files one (`proposal::file_page_proposal`, `append` set, `from: chat`, `proposal::FROM_CHAT`), opening the knowledge base's index read-write for that one row and only if the app made it. The log is never written; Apply adds it with the next number and `ingest::accepted` commits "decisions: add the ruling proposed in chat". `decisions_entry` writes no `sources:` line, and a Why of "proposed in a chat with Ken", for a ruling from no note.
    - `add_source`: a file of a registered repo (a ken:// address or a path with `project`; refused outside a repo, for a folder, and for a file already in Raw) is copied, or a pasted `text` (at most `MAX_SOURCE_CHARS`) is written as a note (`ingest::note_source`), into the knowledge base's `Research/Ingestion/Raw/` under a free name. `raw_target` moved from the app into `ingest` for both. Nothing is ingested inline; the app's Raw watcher or the next pass reads it.
    - `repos` replaces `list_projects`: the one health line, and the knowledge base marked `(knowledge base)`.
    - The knowledge base is the open workspace's first repo with an inbox, else the first `registry::is_library` (`knowledge_base`).
  - Server `instructions` (decision 1): where the open workspace's knowledge base is (name, folder, ken:// root, or that there is none yet), then `BRIEF`: the sections, "the code is the truth, the library complements it", the code index before Grep and Glob, the label and how to weigh the tiers, a ruling is settled, the code wins on what was built, an empty search proves nothing, Research is evidence at its date, cite ken:// addresses, and that rulings and sources wait for a person.
  - The personal set (lead's ruling): `ken-mcp --personal`, server name `ken-personal`, lists `task_list`, `task_create`, `task_update`, `memory_write`, `journal_append`, and `open_in_ken` when the app's listener is in its environment, with a short `instructions` of its own. Neither set answers the other's tools (-32602). Only Ken's own chat registers it: `chat_mcp_config` (src-tauri) writes both servers into the chat's `--mcp-config`, the listener's URL and token only on the personal set. Claude Code's registration (`mcp_info`'s command, JSON and instruction text) is the knowledge server only.
  - Callers: `KEN_GUIDE` keeps the app's rules only (edits as diffs, clickable citations, search with Ken and look yourself before saying "not there", the knowledge base changes through a person, tasks and memory, opening a file); the label, tier weighing, "code wins" and "a ruling is settled" moved to the instructions. `KEN_MCP_ALLOWED` is the twelve, `KEN_PERSONAL_ALLOWED` the personal six, `KEN_SERVER` and `PERSONAL_SERVER` their server names; `ken_mcp_tool` reads both prefixes, the chat's permission handler allows both sets, and `assistant::chat_args` allows both (not `open_in_ken`). The tool card's comments and tests, `api.ts`, the README and code comments say the new names.
  - Choices:
    - `tickets` lists every assignee by default (it is knowledge now, decision 8), with `assignee: me` for the person's; the count of the person's linked tasks is gone from it (tasks are the personal set's).
    - `add_source` takes files of registered repos only, not any path on disk: a source is read by Claude and lands in a repo the team shares. A file elsewhere goes in through the Ingest screen.
    - `propose_decision` requires `why` (decision 13 says Ken fills it properly).
    - A ruling or source filed from outside the app shows in the Inbox at its next refresh; nothing signals the open app.
    - The sidecar was not re-staged: `scripts\win-build.cmd dev` and `build` stage it first, and the check passed against the staged one.
  - The compile check is clean apart from the known warnings. `npm run check`: 0 errors, 6 warnings (the known ones).
  - Not run: any test, and the app. Nothing migrates. A Claude Code session registered with the old tool names keeps its registration (`ken`, `--project <root>`) and sees the new tools at its next start.

## Open for Chris

- **How a retired page names its successor.** Decision 7 says a retired page names its successor, but decision 5's six keys have no field for it. For now `status: superseded` retires a page and the brief says to put a banner naming the replacement in its text; `replaced_by` is read from older pages only. Choose: keep the banner, or add a seventh key (say `superseded_by`) that Ken writes and search shows.

- **The title weight in plain-word questions.** The breadcrumb field counts 2.5 times the body in the any-word fallback too, where names were cut to a quarter because one common word in a name outranked the body. If a common heading word ("Overview", "Notes") starts winning plain questions, lower `W_TITLE` there; it needs a measurement first (decision 38 left measurement to be designed).
- **Confirming what the repo analysis proposes.** Role rules and controls are written into `.ken/knowledge.json` as soon as Claude proposes them, applied on the next scan, and a person edits them there (the settings screen is slice 12). Choose: keep that, or hold them as a proposal card in the Inbox until a person accepts them.

- **How much a symbol counts in the rerank.** Slice 6 counts a code chunk's symbol words like path words (1.0 each, beside 0.5 for the text and 3.0 for the file name), which brings a named definition above a note that only says the words. If definitions still lose to pages that merely mention them, they could count like the file name instead; that wants the measurement decision 38 left open.

- **Commits for what is applied later.** A held edit, a ruling or a ticket applied after the ingest is its own commit. Choose: keep one commit per click, or gather them into one commit per card when it is marked seen.

- **Mark as seen.** With the source claimed as its note is written, Seen only closes the card. Choose: keep it, or close a card by itself once nothing waits from it.

- **A log line for a drift run that changed nothing.** Decision 2 says one `log.md` line per drift run; a run now writes its line (and commit) only when it stamped, fixed or followed a page, so a retry that changes nothing leaves no trace. Choose: keep that, or a line for every run (a commit each time too).

- **Re-reading pages past the age rule.** A page not checked in 30 days is re-read by Claude like a real change, one call each, sharing the 20-an-hour cap with summaries. A knowledge base with hundreds of old pages takes days to catch up the first week. Choose: keep it, or file aged pages as findings and re-read only pages whose code moved.

- **The supersession pass's first read.** The first time the pass sees a decisions log, every entry counts as new, so the whole log is clustered and asked about once (at the shared cap). Choose: keep it, or mark the existing entries read at first sight and judge only entries added from then on.

- **Fetching code repos.** Ken sees a branch move when it moves locally (a person's pull, any fetch); every drift run fetches first, so at worst the weekly run catches up. Choose: keep it, or fetch each cited repo on a timer (say hourly) so an upstream merge starts a run within the hour.

- **The `base` a repo entry names.** `.ken/knowledge.json` says `base` is the branch drift measures against, but drift measures against the remote's default branch (or the checkout's upstream, said so), as before. Choose: read `base` when set, or drop it from the template.

- **Topic tags: a call of their own.** Tags are asked for in a separate background call after an ingest or drift writes (up to 12 pages a call, sharing the 20-an-hour cap). Choose: keep, or ask for them inside the ingest plan and the drift re-read so they cost no extra call.

- **Commits that cite something made later.** A commit is read once, and linked only to tickets and rulings the knowledge base has then; a ticket filed after the commit that names it is not linked. Choose: keep, or re-read a repo's history when a ticket or ruling is added.

- **The `entities` index state.** With the local reader gone, "read in full" (`entities`) indexes exactly like `search`; set-up and the Team screen still offer both. Slice 11 makes indexing two-state (decision 34); until then the choice does nothing.

- **The chat guide leans on the server's instructions.** Decision 1 leaves the chat guide only the app's rules, so how to weigh a hit's label, "a ruling is settled" and "the code wins" now reach Ken's chat through the knowledge server's `instructions`, which Claude Code adds to the system prompt. The chat's answers were measured with those rules in the guide. Choose: keep it, or repeat the three in the guide until a measurement (decision 38) shows the instructions carry them.

- **Signalling the app when an MCP tool files something.** A ruling `propose_decision` files, or a source `add_source` puts in Raw, reaches the open app at its next refresh (the Raw watcher starts an ingest for a source; nothing tells the Inbox about a ruling). Choose: keep, or have ken-mcp tell the app over its listener when one is running.

## Next: slice 11, setup and building (32–34)

Nothing has been edited yet.

- [ ] The three-step setup (32), in `src/onboarding/SetupFlow.svelte`, `setupFlow.ts` and `setup.rs`:
  - 1, the workspace name; 2, where the knowledge base is saved: a new repo (`setup::create_wiki`, `wikinew::create`) or an existing wiki to overlay; 3, the sources: repos and folders (`propose`, `propose_repos`), each with its kind. The row's team and index state go (35, 34); connectors are left for later (DECISIONS "Still open").
  - Confirm lays down the knowledge base's structure (the template, decision 4) and scans every source, so search and the code index work from day one.
  - Running setup again on a knowledge base reports missing or out-of-date template files as a diff and never overwrites (4); `rescan` keeps reporting what moved.
- [ ] The Files tab splits into Wiki and Sources (32): `FilesScreen.svelte` and `FileTree.svelte` show the knowledge base under Wiki and the other repos under Sources.
- [ ] The build queue (33), a new module in ken-core:
  - A queue of the workspace's sources, the heavy hitters first (a simple order to start, by indexed size, since the exact order is still open), kept in the knowledge base's index (`Db::check_state` or a table).
  - Two passes, both through ordinary ingest one source at a time: a high-level pass per source (one note from its README, layout and code index outline, written into Raw like any source), then file by file from broad to fine.
  - Throttled by the one throttle below and shown in the status panel; "a few at once once trusted" is a setting, default one.
  - Answers worth keeping are filed back as pages: a chat answer offered as a source (`add_source`'s path) or as a page proposal; pick one and say so.
- [ ] Map an existing wiki after a dry run (33): each page placed in a section (Rules, Decisions, Design, Current, Reference, Research), shown as a list of moves first; applied on Confirm with links rewritten where certain and the rest reported (7), a `log.md` line and one commit (36). Nothing is deleted.
- [ ] Index rules in one place (34):
  - Two states: a path is indexed or it isn't. `kenignore::Tier::SearchOnly` and the `~` syntax go; `registry::IndexState` {Off, Search, Entities} becomes indexed or not, which settles the `entities` item above.
  - The rules live in `.ken/knowledge.json` (per repo under `repos`, read through `knowledge_settings`) plus the fixed built-ins: VCS, dependencies, build output, secrets, tests (`kenignore::TEST_PATTERNS`), minified and generated files, binaries. `project.json`'s `excluded`, the repo `.kenignore` and the workspace `.kenignore` are read once into it (or kept read-only as legacy; say which), and the Team screen's ignore list follows.
  - A file is read again when its content hash changes; one that fails three times is counted as failed (check the scan's failure handling against this).
- [ ] One throttle and one status panel (34):
  - Gentle / Normal / Fast and Pause, one app setting over all background work: the summaries and checks (`EXTRACT_PACING`, `summary::MAX_CALLS_PER_HOUR`, `CHECKS_EVERY` in `background_worker`), the build queue, ingest passes, the semantic rebuild. It replaces the `backgroundExtraction` flag, and slice 4's summary cap.
  - One status panel: what is running, what is queued, progress, and what failed with its reason.
- [ ] Not carried over: `wikidraft.rs`, `wikiverify.rs`, `checkout.rs` (none is on this branch; keep it so).
- [ ] Tests to update or add: setup's three steps and the re-run diff, the Files split, the queue's order and two passes, the wiki mapping's dry run and apply, the two index states and the rules from `.ken/knowledge.json`, the throttle levels and Pause.

## Later slices

- [ ] **12, Wright leftovers (35–38):**
  - Remove escalations, families and the team bus, OpenSpec, the people, owner and roster code (`people.rs`), and the evals and `KEN_EVAL_*` hooks.
  - The Team screen becomes workspace settings.
  - Keep Your day, tasks and memory.
- [ ] **13, the method pages in WoW:** finish the remaining pages on `docs/knowledge-core` in the WoW repo.
- [ ] **14, verify:**
  - The full test suite (ask first, see above).
  - An end-to-end run: set up a new knowledge base, ingest a source, search, drift on a commit.
