# Session handoff — Ken at Level 2 (`knowledge-layer`)

Last updated: 2026-09-24. Branch: `knowledge-layer`, cut from
`ken-workspace-home` at `f00218c`. Read this top to bottom before doing
anything. It replaces the 2026-09-01 handoff (SR group folders); what
still applies from that one is under "Carried over" at the end.

## Where the plan lives

- **The analysis:** "Ken at Level Two",
  https://claude.ai/artifact/9b8HKcjTBRaJoreywfFcEG (private, owned by
  the other account; rulings go on it as comments). It checks 45
  Knowledge rulings from Ways of Working against this code: 5 has,
  17 partial, 23 missing, each with file and line at `f00218c`.
- **The method:** https://github.com/Stingbro/Ways-of-Working, `main`
  at `b824813` on 2026-09-24. The white box is
  `design/wright/wright-whitebox.html`; the Ken/Wright split note is
  in `BUILD-ORDER.md`, Repo strategy. Both are private to the
  `Stingbro` GitHub account.
- The earlier Ken handoff with items 5 and 6 was never committed; it
  lived on the other PC. This file is now the only copy.

## Measure `f00218c` on the other PC

`f00218c` fixes workers that stopped for every workspace member but
one, and adds one 2s pace for background extraction. Only a workspace
with several members and a real queue shows either, so the measurement
belongs on the other PC, against the Shattered-Realms index:

```sql
SELECT status, COUNT(*) FROM extractions GROUP BY status;
```

on `%APPDATA%\ken\index\<id>.db`. The before figures (from `d8ed403`):
354 errors, 77 done, 1205 queued. Look for errors staying low, the queue
draining at the pace, and every member moving. Status: built on this
PC (dev), not measured anywhere.

## Done on this branch

- **Steps 1 and 2, kind and sync (first part of 2).** `RepoKind`
  (team, wiki, code, reference) and `team` on each registry entry, in
  local app data, never in the repo; kept across a re-add. `sync_auto`
  now honours an explicit `sync.auto` in `project.json`, and otherwise
  is on only for team and wiki kinds; a folder with no `project.json`
  never syncs. App command `set_project_kind`; `api.setProjectKind`.
  No set-up screen yet (step 4).
  - **On the other PC after pulling this:** every existing project has
    no kind, so sync goes off. For sr-docs and any other wiki or team
    repo, either set its kind or switch Sync on in Settings (which
    writes `sync.auto: true`).
- **Step 2 index state, step 5 extraction scope, and step 3's re-tier
  (kenignore task 1.6).** `kenignore::kind_rules`: a code or reference
  repo (and not also team or wiki) gets a built-in `~*` between the
  global built-ins and the user's `.kenignore`, so `!docs/` there still
  reads a code repo's docs. A repo with no kind reads as before. Schema
  v14 adds `files.tier`; every scan and file refresh stores it and drops
  pending extractions for non-full files, so a kind change reaches
  unchanged files. Coverage, the unqueued check and the open-time
  backfill count full-tier files only; the deep rebuild sends only
  full-tier files. Already-extracted entities from a re-tiered file
  stay in the graph (removing them is still open).
- **Step 3, built-in patterns.** Secrets (`.env*`, `*.pem`, `*.key`,
  `*.p12`, `*.pfx`, `id_rsa*`/`id_ed25519*`/`id_ecdsa*`, `secrets.*`,
  `credentials.json`) are hard-ignored: no `!` line brings them back.
  Archives (`*.zip *.7z *.rar *.tar *.gz *.tgz *.iso *.jar`) are the first
  global built-in rule; `!*.zip` brings them back. Media is untouched
  (Ken transcribes video). The scan skips linked checkouts (a folder
  whose `.git` is a file: worktrees, submodules) but still walks nested
  clones; workspace discovery never offers a linked worktree as a member.
- **Step 7, hash not time.** Schema v14 also adds `files.byte_hash`
  (xxh64 of the bytes, files up to 64 MB). A rescan or watcher event
  that sees the same size but a new modified time hashes the bytes; on a
  match it records the new time and skips the parse and the search
  write. Cloud placeholders are never hashed (reading one downloads it).
- **Step 8, locators.** Chunks record the 1-based line their own content
  starts on (`chunks.line`, schema v14; kept current when lines are added
  above an unchanged chunk). Routed hits carry `line` and `locator`:
  `repo:path:line`, `repo@sha:path:line` for a git repo (sha read from
  `.git/HEAD`/`packed-refs`, no process), led by `[[Note]]` for a page in a
  team or wiki repo. The MCP search output leads with the locator; the
  app DTO and `RoutedHit` TS type carry both. Old chunks have no line
  until the file is re-chunked.
- **Step 9, frontmatter.** New `pagemeta` module: a tolerant reader (not
  YAML: template pages carry `{{date}}` and `#` comments) for `title`,
  `aliases`, `status`, `verified`, `updated`/`date`, `sources`,
  `replaced_by`/`superseded_by`, `generated`; and the library section from
  the path (Markdown only). Stored per page in `page_meta` (schema v14) at
  index time; `backfill_page_meta` on project open fills pages indexed
  before. Hits carry `page` (section, title, verified, Research date,
  retired, generated, replaced_by, band). Ranking bands: Ways-of-Working
  and Platform first, Research/retired/generated last, stable within a
  band, and across members in `merge_routed`. MCP output adds "retired,
  see ...", "generated", "evidence from <date>" or "verified <date>".
  Not yet: serving a retired page's replacement in its place (the hit
  names it), and the app UI showing these (the data is on the DTO).
- **Step 6, index health.** `Db::index_health`: read of extractable,
  pending, retrying, failed (3 attempts), skipped (search-only files), and
  the last control query. Every scan ends with `scan::control_query`: the
  first of START-HERE.md, Current/Index.md, README.md searched by its
  title must rank first; the result is kept in `meta.index_control`. A
  failed file that changes starts its three attempts over. App command
  `index_health`; Home's footer shows a graph row and one warning line
  (no on-device model, or the control check missed). The Timeline now
  shows the model notice too.
  ken-core on this PC: 786 pass, 53 fail (all pre-existing, Windows);
  frontend 479/479.

## The steps, in order

What protects the index first, then what a hit carries, then the
inbox, then drift. Paths are relative to the repo root.

1. **Sync off for code.** `sync_auto` in `crates/ken-core/src/sync.rs`
   defaults to `true`: any git repo with a remote gets `git add -A`,
   a commit "Ken: update knowledge", pull and push (see `0f89f8d`,
   `921f432`). Default off; on only for kind team or wiki.
2. **Kinds and index state per repo.** Registry entries gain `kind`
   (team, wiki, code, reference, or a list) and `team`
   (`registry.rs:17-21`). The index state (off / search / entities) is
   one line Ken writes at the top of the repo's `.kenignore`, so the
   existing tiers do the work. Kind sets the default: entities for
   team and wiki, search for code and reference.
   - **2b. Business docs vs dev docs (new, from the other PC).** The
     library holds two kinds of page for two audiences. Business docs
     are for people who never read code and go further than a
     Confluence page: what the product does, what was decided and
     why, what changed per release. Dev docs are conventions,
     architecture, how-tos and code references. To decide, in an
     analysis first: how they are told apart (repo kind, wiki section
     or tag), who writes and who reads each, and how Ingest and the
     drift check treat each.
3. **Built-in patterns.** Fill the empty built-in list
   (`kenignore.rs:210`): worktrees from `git worktree list`, secrets
   (hard-ignored, no `!` brings them back), archives, media. Re-tier
   an indexed file when the ignore file changes (kenignore spec task
   1.6). The Deep rebuild honours the tier
   (`knowledge_model.rs:496-501`).
4. **Set-up: Folder, Team, Repos, Index.** Confirm writes the
   registry, the workspace manifest and the ignore lines; Ken reads
   only after. Nothing is written into a code repo. Scan again
   reports what moved, never overwrites.
   - **4b. First-time wiki from an analysis (new, from the other PC).**
     Today "Start a new team" writes the wiki from empty templates.
     Add an option, offered and never forced: Claude reads the repos,
     READMEs, existing docs, any Confluence export or dropped-in
     documents and the code layout, then drafts the Current pages
     (project, team, who does what), the architecture and convention
     pages and the first business docs (2b) for a person to review.
     Each drafted page names the source it came from.
5. **Extraction scope.** Entities from team and wiki repos only; code
   is chunked and embedded, never sent to the extraction queue
   (`scan.rs:338`, `knowledge_model.rs:416-450`).
6. **Index health.** Home shows pending, failed and skipped; the
   no-model row on Home, Map and Timeline (frame 8c). Each rebuild
   ends with one control query that must rank a known page first.
   Attempts reset when a failed file changes.
7. **Hash, not time.** The rescan re-reads only when the content hash
   changes (`scan.rs:273-283`).
8. **Locators.** Chunks store their first line; hits and MCP citations
   come back as `repo:path:line`, `[[Note Name]]` for wiki pages, and
   `repo@sha` for git repos.
9. **Frontmatter.** Read `title`, `aliases`, `verified`, `sources`,
   retired and generated banners from wiki pages. Hits carry the
   verified date; Ways-of-Working and Platform rank first; Research
   hits are dated; a retired page is served under its replacement.
10. **Page links.** The third index: name links by stem or alias,
    path links by path, code stripped first, counted apart.
    Duplicate and missing names land on Review, grouped by section.
11. **Aliases in the query**, from the vocabulary page and the
    decisions log, in the app and the MCP.
12. **Ingest and Recipes.** The rail item splits into two tabs. Ingest
    watches `Research/Ingestion/Raw/`, writes one dated note per source
    to `Ingested/`. Recordings land in Raw. Review gets a card per
    source with Open, Undo and Seen. Every agent write to a wiki page
    goes through staging, first run included.
13. **Drift.** Weekly sweep over wiki pages: Docs check against git,
    Decision check per decisions-log entry, the 30-day age rule, two
    controls and a minimum count, a run record. Findings on Review;
    Research in its own bucket.
14. **The graph per team**, built from the manifest's team repos, its
    file in the workspace folder (`lib.rs:7112-7164`,
    `federation.rs:865`, `workspace_kg_db.rs:40`).
15. **Teams and Settings.** Send from the app into a team's `shared/`;
    Settings uses the plain names and scopes from frame 10e; the
    connector finds `ken-mcp` where `install.ps1` puts it on Windows
    (`%LOCALAPPDATA%\Programs\ken-mcp`).

Out of this branch: the upstream merge from smo-key/ken (17 commits,
7 conflicting files including `src-tauri/src/lib.rs`, its own ticket
after step 1; no `upstream` remote is set in this clone), and the
multi-workspace spec (0 of 35 tasks).

## Method pages that change once Ken is at Level 2

In Ways-of-Working, listed in the report's "Method pages that change"
section: `docs/index.html`, `docs/manual/index.html`,
`docs/the-tool.html`, `docs/tool/index.html`, `docs/wright.html`,
`docs/wright-onboarding.html`, `docs/manual/docs-system.html`,
`docs/manual/drift.html`, `docs/ways-of-working.html`, `README.md`,
`BUILD-ORDER.md`, `templates/wiki/Research/Ingestion/Index.md`, and
white box frames 8, 9, 10e, 11 and sections 16 and 17. Items 2b and 4b
will need their own lines there once decided.

## Machines

- **Other PC** (`C:\Users\chris\Code\ken`, and earlier
  `C:\Users\Owner\Documents\Hytale Code\ken`): has the full build
  environment and an installed Ken with real indexes. The measurement
  above belongs there.
- **This PC** (`C:\Code\ken`, `C:\Code\Ways-of-Working`, set up
  2026-09-24): Rust 1.97.1, VS 2022 Build Tools, Vulkan SDK 1.4.357,
  LLVM, Node 24.19 in Program Files (nvm-windows still selects Node 20
  on PATH; `scripts\win-build.cmd` uses Node 24 for its own run). The
  full app `cargo check` passes. Build with `scripts\win-build.cmd`
  (`doctor`, `core`, `test`, `check`, `sidecar`, `dev`, `build`),
  target dir `C:\kt`. winget's msstore source fails here on a
  certificate check; use `--source winget`. GitHub: `Stingbro` owns
  both repos; `StingBros` is a second login with no access to
  Ways-of-Working.
- **ken-core tests on Windows** (`scripts\win-build.cmd core`): 759
  pass, 55 fail. About 45 spawn the fake Claude CLI, a bash script
  Windows cannot launch (os error 193); the rest (pdf, scan, import,
  recipe, cloud) are unexamined, likely CRLF or path separators. The
  old recipe ran only `workspace::`, so the full suite was never green
  on Windows.

## Carried over from the 2026-09-01 handoff

- **The SR move hold still stands on the other PC:** do not move the
  three SR repos into `Hytale Code\SR\` or rename `~/.claude/projects`
  dirs until the user says go. The go-sequence and scripts are in
  `openspec/changes/workspace-group-folders/migration/`; see git
  history of this file (`606fe3e`) for the full sequence. Task 5.3
  (live verification) waits on it.
- **Build recipe:** plain `cargo test` fails (Vulkan/Ninja env). Use
  `scripts\win-build.cmd`, which finds MSVC, Ninja, Vulkan and LLVM
  on any machine; it supersedes
  `openspec/changes/workspace-group-folders/migration/test-workspace.bat`,
  whose paths are the other PC's.
- **Never write JSON or config with PowerShell `Set-Content`:** PS 5.1
  writes a BOM and `AppSettings::load` silently rejects BOM'd JSON.
  Use the Write/Edit tools or Node.
- Open, not approved: the sr-docs optimization pass (frontmatter on
  `Work/` tickets, lint job, `log.md`, `verified:` dates; overlaps
  steps 9 and 13), configurable sync interval, and the onboarding
  flag chicken-and-egg (the workspace card is gated on a flag a fresh
  install's settings.json lacks; overlaps step 4).
