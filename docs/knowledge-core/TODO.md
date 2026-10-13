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
  - Lost: a file with no text (a cloud placeholder, a failed, binary or minified file) is no longer found by its name alone.
  - The compile check is clean apart from the known warnings. `npm run check`: 0 errors, 8 warnings (the known ones).
  - Not run: any test, and the app. The v21 migration rewrites `contents` once (`DROP COLUMN`) on first open, which may take a moment on a large index.

## Next: slice 3, frontmatter (5)

Nothing has been edited yet.

- [ ] `pagemeta.rs` `PageMeta`: six keys, `title`, `aliases`, `status`, `updated`, `sources` and `checked`, plus an optional `reviewed_by`.
  - `status` is draft | current | superseded. Older values still read: `retired` and `deprecated` as superseded.
  - `checked` is a date and a sha, written by the check that re-read the page. Pick one written form (say `checked: 2026-10-13 f00218c`) and parse both parts.
  - Decide what happens to the keys the six leave out but code reads today: `generated` (`HitPage.generated`, the Record tier in `authority.rs`), `replaced_by`/`superseded_by` (decision 7: a retired page names its successor), and `date` (read as `updated`). Simplest: keep reading them as legacy and stop writing them; note the choice.
- [ ] Drop `audience` everywhere:
  - `pagemeta.rs`: `PageMeta.audience`, `Audience`, `audience_of`, `audience_at`, `suits`, `HitPage.audience`.
  - `src-tauri/src/lib.rs`: `route_search`'s `audience` argument, `RouteFanOut.audience` and the `fetch` widening.
  - `crates/ken-mcp/src/main.rs`: `audience_arg`, the `audience` property on `semantic_search` and `route_query`, and `fetch_for`'s audience case.
  - `src/lib/api.ts`: `Audience` and `routeSearch`'s `audience`; `src/search/SearchOverlay.svelte`: the READERS row.
  - `drift.rs`: the business-page split (decision 28 drops the audience split); every page with sources is measured alike.
  - `Templates/Ingested-note.md`: `audience: business`.
- [ ] `verified` becomes `checked`:
  - `HitPage.verified` (and `api.ts` `HitPage`), the label's date in `authority::hit_label`, the MCP `page_note` ("verified …").
  - `drift.rs` takes a page's pin from `checked`'s sha, else its date as now. The rest of drift is slice 8.
  - `proposal.rs` drops `checked:` from a proposed page, as it dropped `verified:`.
  - The ingest edit prompt (`ingest.rs`, "leave any `verified:` line") names `checked:`.
- [ ] Remove `HitPage.band` and `pagemeta::band`: nothing ranks by them since 2a (23). Also the `authority_weights` doc comment that mentions band, and `band` in `api.ts` `HitPage`.
- [ ] `db.rs` `page_meta` table (schema v22): add `checked` and `reviewed_by`, drop `verified` and `audience` (and whatever the legacy choice above retires); `set_page_meta` and `page_meta` follow.
- [ ] Templates (`crates/ken-core/templates/wiki/Templates/`): only the six keys, plus `reviewed_by` where it fits.
  - `Finding.md` and `Ingested-note.md` say `status: evidence`, not one of the three. Use `current`: the Research section already makes them record.
  - `Ingested-note.md` also has `source:` and `present:`: fold the first into `sources:`, and move who was there into the body.
  - `Ticket.md` keeps its own frontmatter (id, status, type, assignee; decision 8).
- [ ] `lens`, `changed_by` and `pin` are not read anywhere; only a test fixture (`pagemeta.rs`, the `changed_by:` page) names one. Drop them from fixtures.
- [ ] Tests to update: `pagemeta.rs`, `scan.rs` (the `verified:` Rules page), `routing.rs` (`a_scan_alone_makes_chunks_searchable…` asserts `verified`), `authority.rs` (the `checked` label test), `proposal.rs`, `drift.rs`, and the MCP label tests.

## Later slices

- [ ] **4, chunking (16):**
  - Code chunks by definition from the code index parse.
  - A heading breadcrumb title field weighted about 2–3×.
  - Code chunk fields: path, symbol, doc comment and role.
  - One-line summaries for pages, notes, decisions and tickets, cached by chunk hash.
  - Report the chunk cap in index health.
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
