# Tasks: kenignore

## 1. ken-core

- [x] 1.1 `kenignore.rs` (new): `Tier` enum, `Rule` type,
      `parse(text) -> Vec<Rule>` (gitignore semantics + `~`/`!`
      prefixes, comments, blanks, `\~`/`\!` escapes, malformed
      lines skipped with warnings); register in `lib.rs`
- [x] 1.2 `kenignore.rs`: `classify(path, is_dir, rule_sets) ->
      Tier` — hard-ignore short-circuit, then built-in rule sets +
      user rules folded last-match-wins, default `Full`
- [x] 1.3 Built-in rule sets as data: pseudo-member tiers
      (ken-memory D3), per-repo `~.ken/tasks/` (ken-tasks D6),
      exposed for src-tauri to compose per member
      — `built_in_rule_sets()` lands as an honest empty-`Vec`
      placeholder: neither `ken-memory` nor `ken-tasks` exists in
      this codebase yet, so there is no data to encode. The D2
      plug point (built-ins folded before user rules) is wired
      into `scan::scan` and `scan::refresh_path` now, so filling
      this in later is a one-function edit, not a call-site hunt.
- [x] 1.4 Schema: `chunks.tier` column in the v12 migration
      (default 0 = full); ingest walk classifies each path and
      threads tier into chunk rows; `Ignore` produces no rows
- [x] 1.5 Consumer filters: knowledge-model extraction input and
      profiler doc sampling select full-tier only; search paths
      unchanged (both tiers)
      — extraction gating done and tested in `scan::index_one`
      (enqueue only when `tier == Tier::Full`); FTS/OCR stay
      tier-blind per D3. Profiler doc sampling is not yet
      applicable: no profiler module exists in this codebase
      (that's task 2.3/D5, unimplemented) — nothing to gate yet.
- [ ] 1.6 Tier-transition diffing for `.kenignore` edits: old→new
      tier per path ⇒ scoped work items (reingest / flip tier +
      purge KM contributions / delete rows)
- [ ] 1.7 Tests: parse table (every syntax form, escapes,
      malformed); classify table (anchoring, `**`, dir-vs-file,
      ordering, negation chains, built-in override by user `!`,
      hard-ignore wins over `!`); extraction excludes search-only;
      transition diff cases

## 2. src-tauri

- [ ] 2.1 Load `.kenignore` (when present) on project open; compose
      rule sets per member; classification wired into the ingest
      engine's walk
- [ ] 2.2 Watcher: `.kenignore` change ⇒ re-parse, diff, enqueue
      scoped transitions through the existing queue/debounce/cancel
      machinery; malformed-line warning event
- [ ] 2.3 Profiler draft flow (gated by `profiler`): heuristic
      classification ⇒ proposed `.kenignore` with explanatory
      comments; no existing file ⇒ full draft for approval;
      existing file ⇒ additions-only diff appended under a
      `# proposed by ken profiler` marker on approval; never
      deletes or reorders user lines
- [ ] 2.4 Search results carry tier so the frontend can badge
      search-only hits

## 3. ken-mcp

- [ ] 3.1 No new tools; verify `semantic_search` / `kg_search`
      results include search-only chunks and KG answers never cite
      search-only-minted entities (there are none)

## 4. Frontend

- [ ] 4.1 Profiler review UI: proposed `.kenignore` (or additions
      diff) rendered for approve/dismiss before any write
- [ ] 4.2 Subtle "search-only" badge on search results from
      search-only-tier chunks

## 5. Verification

- [ ] 5.1 cargo test --workspace, pnpm test, pnpm check, pnpm build
      green
- [ ] 5.2 No `.kenignore` present ⇒ DB content and behavior
      byte-identical to today (fresh ingest diff)
- [ ] 5.3 Manual on a decompiled-heavy project: `~decompiled/`
      keeps the tree searchable while entity extraction and
      profiler sampling skip it; flip a rule and watch the scoped
      reindex; profiler proposes a sane draft and refuses to
      clobber a hand-written file
- [ ] 5.4 Golden queries: code-lookup queries that target
      search-only files still hit; manager-shaped queries never
      surface entities sourced from search-only content
