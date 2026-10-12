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
- **Tests:** don't run test binaries or anything that spawns shells without asking first. Endpoint security on the dev machine flags it.

## Done

- **Slice 1a: one repo, the new sections, a smaller template** (`89536c3`). Decisions 3, 4, 6, 8, 35.
  - The knowledge base holds `tickets/`, `Decisions/DECISIONS.md`, `.ken/knowledge.json` and `log.md`.
  - The team-repo setup step is gone, along with `teamnew.rs` and `setup_create_team_repo`.
  - `pagemeta::Section` is {Rules, Decisions, Current, Design, Reference, Research}. Legacy folders still map: ways-of-working and conventions → Rules, platform → Reference, work → Current.
  - `Roles::of_repo` reads `.ken/knowledge.json`, falling back to `.wright/team.json`.
  - Ingest writes rulings to `Decisions/DECISIONS.md`, and new rules go in `Rules/`.

## Next: slice 2a, five tiers and the nudges removed (23, 24)

Nothing has been edited yet.

- [ ] **`crates/ken-core/src/authority.rs`**
  - [ ] Add the `Tier` enum {Binding, Current, Reference, Record, Superseded}, with weights +0.75 / +0.4 / 0 / −0.5 / −1.25.
    - Binding: live rulings and `Rules/`.
    - Current: Current and Design pages with `status: current`.
    - Reference: other pages and code.
    - Record: tickets open and done, notes, Research, drafts, generated or mirror pages.
    - Superseded: superseded rulings, retired pages, cancelled tickets.
  - [ ] Remove the people role from `Roles`.
  - [ ] Parse the legacy tier names: rule → binding, closed → superseded.
  - [ ] Split `CLOSED_STATUSES`:
    - cancelled, wontfix, duplicate and rejected tickets → Superseded;
    - done and closed tickets → Record.
  - [ ] Teams override the tier by path in `.ken/knowledge.json`.
  - [ ] Add a `label_of` that returns `kind · tier or role · date` (17).
  - [ ] Update the tests.
- [ ] **`crates/ken-core/src/routing.rs` and `search.rs`**, remove:
  - [ ] band fallback and `band_bonus`;
  - [ ] stale and template hygiene;
  - [ ] `intent_bonus` and `query_intent`;
  - [ ] `find_symbols` (W_SYMBOL);
  - [ ] `follow_links` (W_LINK);
  - [ ] `W_REFERENCE_ASSET`;
  - [ ] the band tie-break in `merge_routed`.

  Keep the blob hygiene, unless the index-time exclusion below covers it.
- [ ] **Score = base + tier.**
  - Base: filename/title 3.0, path 1.0, snippet 0.5, all × coverage.
  - Plus 1.5 × meaning, plus 0.75 when keyword and meaning both find the chunk.
- [ ] **Index time:** exclude `Templates/` and minified text.
- [ ] Note: "where is `get_current_user`" loses its jump to the definition until slice 6 adds symbol matching on plain words.

## Later slices

- [ ] **2b, routing (20):**
  - Every question searches every source, merged once, and a named repo narrows it.
  - Reference repos join only for platform questions, when named, or when nothing else answers.
  - Drop the graph planner, the 5-repo cap and the library-share rule.
  - Report a slow or building repo instead of dropping it.
  - Retire the file-level `search` index and its ranker (15).
- [ ] **3, frontmatter (5):**
  - Six keys: `title`, `aliases`, `status`, `updated`, `sources` and `checked`, plus an optional `reviewed_by`.
  - Drop `audience` (and `Audience` in `pagemeta.rs`), `lens`, `changed_by`, `pin` and `verified`.
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
