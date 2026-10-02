# Session handoff — Ken at Level 2 (`knowledge-layer`)

Last updated: 2026-09-25. Branch: `knowledge-layer`, cut from
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

## Keeping Ken responsive

Indexing sends `index-updated` many times a second across a workspace's
repos. Home stuck on "Reading your list…" came from four things at once;
each now has a rule.

- **No slow work under the global lock.** A command clones what it needs
  (the project, `search_db`, the base dir), drops the guard, then reads.
  `get_tree`, `unread_files` and the mark-seen commands, `ingest_overview`,
  `ingest_card`, `team_overview`, `files_banner`, `sync_status` and the
  transcription re-index work this way. `activate` still runs its
  backfills under the lock (open is slow while it does); the save and
  file-move commands still refresh the index under it.
- **Runtimes drop with the lock released.** Dropping one joins its watcher
  and sync threads, which can be waiting for the lock: `activate`,
  `open_workspace` and `close_workspace` take the map out and drop it after.
- **Slow commands are `#[tauri::command(async)]`.** A plain command runs on
  the window's thread. The ones left plain are quick or must stay in order
  (`save_file`, terminal input, recording start and stop).
- **Stores read with `singleFlight` and `coalesce`** (`src/lib/refresh.ts`):
  one read at a time with one more after a burst, never a newer read
  throwing away an older one, and a burst of events never pushing the read
  back. Day, Inbox, the sidebar counts, conflicts, the tree, unread and
  Ingest use them.
- **Keyed lists have unique keys.** A repeated key throws and stops the
  screen updating. Inbox ids go through `uniqueIds`; lists that only show
  text (sources, takeaways, breadcrumbs) are not keyed; the conflict banner's
  ids carry the repo.

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
- **Step 11, aliases in the query.** New `vocab` module builds groups of
  equivalent phrases from the Vocabulary page word tables (our word <->
  the platform's word; Method Words skipped), each decisions-log entry's
  topic plus its `aliases:` line (the fenced format example skipped), and
  every page's frontmatter title plus aliases. Keyword search is AND, so a
  matched phrase yields up to 3 alternative queries (phrase swapped, whole
  words only); `search_member` runs them after the original and appends new
  chunks. The app's single-project `hybrid_search` now calls
  `search_member` too, so it gains lines, page facts, bands and aliases.
- **Step 10, page links.** New `links` module, the third index. At index
  time each Markdown page's links are stored raw in `page_links` (schema
  v14), code blocks and inline code stripped first: `[[Name]]` (label and
  heading dropped) and relative `[text](path)` resolved against the page's
  folder; URLs and anchors skipped. Resolution runs when asked: a name by
  page file stem or frontmatter alias (other files by full name, for
  embeds), a path by path (`.md` optional). `links::report`: both kinds
  counted apart, missing names most-asked first, broken paths by section,
  names two pages claim. After each scan of a team or wiki repo one open
  Review item of kind `links` is kept (replaced only when its text
  changes, resolved when clean). `page_links` command gives a page's
  outgoing and incoming links, shown as Linked from / Links to under a
  Markdown page in Files (`PageLinks.svelte`). The entity Map is unchanged:
  its nodes are entities, not pages.
- **Step 14, the graph per workspace (per team still to do).** The merged
  graph is built over the workspace manifest's members (every Ok member
  with an index on disk, read for entities: team, wiki, or no kind yet),
  not only the members open this session. Its file moved from app data to
  `<workspace>/.ken-workspace/kg.sqlite` (`workspace_kg_root`; app data
  only with no workspace open); the build, the three KG read commands,
  routed search and the MCP (`kg_root`, via the last-opened workspace) all
  use it. The old app-data `kg.sqlite` is left in place, unused. Not
  unit-tested (src-tauri has no tests for it); check in the app. A
  team's graph is a view of the merged one, not a second build (which
  would multiply model cost): `workspace_kg_search` takes `team` and keeps
  entities with a link into that group's repos; the graph screen passes
  the scope picker's group. The overview counts are still whole-workspace.
- **Step 13, drift.** New `drift` module: the Docs check (a page's
  `sources:` measured with git from the commit its `verified:` date pins,
  falling back to `updated:`, to the default branch read from
  `origin/HEAD`, the fallback named in the run), the Decision check (per
  decisions-log entry from its own date, superseded skipped), and the
  30-day age rule. Only `repo:path[:line]` is measured (repo = this repo,
  a registered project, or a sibling folder); `[[Note]]` and `D-nnn` are
  counted as cross-references; placeholders and `...` skipped. Severity:
  Finding (path gone, line past end, checked before any pin), Judgment,
  AutoRecleared (whitespace/comment-only diff). Research in its own
  bucket; retired and generated pages skipped. Controls and minimum from
  `project.json` `drift: {controlMoved, controlClean, minPages,
  intervalDays}`; a wrong control or too few pages voids the run; no
  controls = `uncontrolled`, said in the report. Exit codes 0/1/2. Runs
  recorded in `drift_runs` (schema v14); one open Review item of kind
  `drift`. The app runs it after a background scan of a team or wiki repo
  when due (weekly); commands `drift_status`, `run_drift_now`.
- **Step 4, set-up (first cut).** New `setup` module: `propose(folder)`
  reads only (discovery, `.git/config` origin, repo markers, library signs
  such as DECISIONS.md/.obsidian/Research/Ingestion/Current/, team signs
  such as tickets/ decisions/ ideas/, a docs README naming a sibling,
  `git worktree list`) and proposes a row per repo (kind, team, index
  state, evidence) plus ignore rows (worktrees; secrets shown as a fixed
  built-in). `confirm` is the only write: workspace manifest (teams as
  groups unless already a group folder), registry kind/team/index,
  ticked ignore lines under a dated comment, declined rows in
  `.ken-workspace/setup.json` so they are not proposed twice. `rescan`
  reports new folders, gone members and new worktrees; never writes.
  Registry gains `index` (search/entities override; `kind_rules_for` and
  the graph members honour it). Commands `setup_propose`, `setup_confirm`
  (opens the workspace), `setup_rescan`; `SetupFlow.svelte` (4 steps) is
  the first choice on the start screen, beside the old wizard. Gaps:
  confirm still creates `.ken/project.json` in each member, code repos
  included (dropping it touches every Project::open); not tried in the
  app. Settings > Projects > Repos (`WorkspaceRepos.svelte`) now edits each
  member's kind, team and index state (`set_project_index`; `off` ignores
  the whole repo) and has Scan again: Add a new folder with its proposed
  row, Skip a new worktree (adds its ignore line), note a gone repo.
  ken-core on this PC: 803 pass, 53 fail (all pre-existing, Windows);
  ken-mcp 39/39; frontend 482/482.
- **Step 15, teams and settings.** Each feature flag has a plain `label`
  and `applies` (this repo, this machine, the team, mine) from white box
  frame 10e, with Ken in place of Wright; Settings and onboarding show
  them with the registry description, code name small. `find_ken_mcp`
  looks in `%LOCALAPPDATA%Programsken-mcp` (install.ps1) and uses
  `where` on Windows. New `family_send` command and a Send form in the
  family tray: a task, message or notification to a teammate's inbox
  from the app, the same lane-2 write as the MCP tool. Not yet: a
  separate Ken Settings page for sync and the connector (they stay where
  they were), and `shared/` as a scratch pad in the UI.
- **Step 12, Ingest and Recipes (first cut).** New `ingest` module: the
  library inbox. `waiting` lists `Research/Ingestion/Raw/`; `ingest_one`
  reads a source's text, prompts with the library's own
  `Templates/Ingested-note.md` (or the method's, built in), writes
  `Ingested/<date>-<name>.md` with `source:` set, moves the source to
  `Ingested/<date>-<name>/`, and files one Review card (kind `ingest`,
  body = What It Overturns / Actions / Rulings) whose payload holds the
  placement and a hash of the note. `undo` moves the source back and
  removes the note unless it was edited. The model call is injected, so
  tests run on Windows. App: `start_ingest_pass` after a scan of a team or
  wiki repo (one at a time, Claude CLI, 10 min per source, a failure
  files one `ingest-failed` item and stops the pass); commands
  `ingest_status`, `ingest_now`, `ingest_undo`. Review: Open, Undo, Mark
  as done on the card. Ingests screen: tabs Ingest (new inbox view),
  Recipes (was Knowledge docs), Automations; rail says Ingest. Not yet:
  rulings and tickets from a note; staging for chat edits (they are
  Claude Code's own file tools and would need a PreToolUse hook; the MCP
  has no wiki-writing tool, memory writes to the workspace memory folder,
  automations already stage a proposal). Current pages rewritten in place
  from notes: the recipe template "Current, from ingested notes"
  (collection, output `Current/`, sources `Research/Ingestion/Ingested`),
  so the recipe engine's staging and holds apply. `refresh::evaluate` now
  exempts a first run only when its output has no files yet, so a first
  run over existing pages is weighed like any other (one test updated to
  approve its held first run). svelte-check shows 19 warnings: the new
  one is IngestForm's `preset.sources`, the same initial-value pattern as
  its eight other preset fields. A recording made in Ken in a library with `Raw/` now writes its
  transcript there (audio stays in Recordings/; audio-only or failed
  transcription stays whole in Recordings/).
- **Item 4b, the first wiki from an analysis.** New `wikidraft` module:
  `gather` reads each workspace repo's READMEs/CLAUDE.md/START-HERE/AGENTS,
  up to 10 top-level docs, its two-level layout, CODEOWNERS and `git
  shortlog` authors, plus any extra folder (a Confluence export), labelled
  `repo:path`, capped at 120k chars. `draft` fills Current/Project, Team,
  Who-Does-What and Conventions/ARCHITECTURE only when missing or still an
  untouched template (`{{` left), using that template; forces `status:
  draft`, drops any `verified:`, cites sources inline and in frontmatter;
  one Review card (kind `wiki-draft`) lists drafted, kept and failed
  pages. Command `draft_wiki(wiki, extra)`; offered as a checkbox on the
  set-up Index step (wiki repo select, optional documents folder).
  Business docs beyond Current/Project wait for 2b.
- **Chunks without a model.** Found while writing the test plan: chunks
  were written only by the semantic rebuild, so with Search by meaning off
  (or no embedding model) project search found nothing, and lines, page
  facts, bands and aliases had nothing to ride on. Since `d6650c3` the
  project search box has used chunk search only. Now every scan chunks
  each indexed file (default profile); the rebuild embeds every chunk
  with no vector (`Db::chunks_missing_vectors`), not only changed ones;
  `remove_file` deletes a file's chunks (a removed file kept answering).
- **Item 2b, business docs vs dev docs: by section (decided 2026-09-24).**
  `pagemeta::audience_of`: frontmatter `audience:` wins; else Current,
  Design, Work are business; Conventions, Platform, Reference are dev;
  Ways-of-Working is method; Research has none. Stored in
  `page_meta.audience`; hits carry `page.audience`. `hybrid_search` takes
  `audience` and the search box has a For business readers toggle
  (project scope). Drift checks business pages by the age rule only
  (`DriftRun.business_pages`, said in the report). The first-wiki draft
  adds `Work/Releases.md` from CHANGELOG/RELEASES and git tags. Not yet:
  the audience filter in all-projects search and the MCP.
- **Set-up picks repos one by one (2026-09-25).** No folder step: the
  person picks each repo (several at a time; a folder of repos stands for
  each inside it; a worktree comes back off). `setup::propose_repos`,
  `setup::confirm_repos`; the workspace lives in Ken's app data
  (`%APPDATA%\ken\workspaces\<id>`), nothing is written in a shared parent.
  `WorkspaceConfig.paths` maps a member's short name to its folder
  (`Workspace::member_root`, `create_at`); workspaces in a parent folder
  keep resolving members relative to it. Each repo has a `description`
  (proposed from its README's first paragraph, editable at set-up and in
  Settings > Repos; registry `description`, `set_project_description`),
  read first by the first-wiki draft. Settings > Repos > Add repos… adds
  picked repos to the open workspace. `plain_canonical` strips Windows'
  verbatim path prefix. The folder-based `propose`/`confirm` remain for
  Scan again in parent-folder workspaces.
- **The wiki at team level (2026-09-25, `55c2c9a`).** A wiki belongs to a
  team. Set-up's Team step picks each team's wiki: a wiki repo already
  picked, a new one (`wikinew`: the Ways-of-Working `templates/wiki`
  bundled at `b824813` under `crates/ken-core/templates/wiki`, filled with
  the team, the wiki's name, the repos and their descriptions, `updated:`
  dates only, never `verified:`; `git init` and one commit, nothing
  pushed), or none. The draft is two passes (`wikidraft::draft_team`):
  `Repo-Map/<repo>.md` per repo from that repo alone (own 60k budget),
  `Repo-Map/Index.md` written by Ken, then the team pages from the repo
  pages. `team_repos`/`wiki_for` scope a wiki to its team's repos. Adding
  repos (Settings > Repos): they wait for team and description, then
  `wiki_add_repos` drafts their pages and files one `page-proposal` card
  per kept page (architecture, who does what, project, vocabulary, Repo Map
  index) with a diff; `apply_page_proposal` refuses a page that changed
  since. Removing a repo does not yet flag the pages that cite it.
- **Scale (2026-09-25).** Benchmark: `examples/scale_bench.rs`;
  `examples/search_probe.rs` times searches on a built index.
  - Scans: one commit per 500 files, `synchronous=NORMAL`, metadata from
    the listing (`4dfdf97`).
  - Drift from git diff (`df2b2ff`): `PinCache` in meta `drift_cache`; one
    `git diff --name-only <last> <now>` per repo, only citations of changed
    files re-measured; pins once per repo and date; `DriftRun.measured`/
    `reused`.
  - Watcher rescans only the named paths (`1a8f2c7`, `scan::scan_paths`,
    reads `.gitignore`s like the walk; a folder, >2,000 paths or a failure
    falls back to the full scan).
  - Search (`4b3edcb`): section FTS ranks before the join (common words at
    50k files 0.9–1.3 s → ~50–200 ms, same hits); plain all-projects search
    runs repos in parallel.
  - Not done: embeddings stay on for code repos; text is stored twice
    (contents + chunks) — size accepted for now (~12 KB per file).
- **Upstream merged (2026-09-25, `d725957`).** 40 commits from
  `smo-key/ken` main (editor work, v0.2.x); remote `upstream`. Upstream's
  single-project handlers now go through `member`/`member_mut`. Background
  extraction is back on (`KNOWLEDGE_EXTRACTION_ENABLED`, Map/Timeline in
  the nav); it only reads files marked for entities.
- **Chat like Headway (2026-09-25).** Default permission mode: every
  Edit/MultiEdit/Write is an `EditProposal` shown as a git-style diff with
  per-change Accept/Deny (chat card and over the open page;
  `answer_edit_proposal`). `KEN_GUIDE` appended to the system prompt: edits
  are reviewed, cite sources as links, never navigate unasked. Citations
  (`path#L12`, `path:12`, `#slug`, `ken://…#L12`) open at the place
  (`app.openAt`, `revealAt`). Ken's MCP runs in every chat (`--mcp-config`
  in app data `chat-mcp/<id>.json`); read tools allowed, write tools
  declined; `open_in_ken` posts to the hook listener's `/ken-ui` (token)
  only when the person asks.
- **More (2026-09-25):** remove a repo from the workspace (its team wiki
  lists the pages citing it); code/reference repos get no `.ken/` (config
  in app data `project-configs/<hash>.json`); a file made search-only leaves
  the graph; a team's graph overview has its own counts; unit tests use a
  temp data folder, never real app data.
- **Ingest in two parts (`e41583d`).** Processing writes the note straight
  into `Ingested/<Meetings|Recordings|Documents>/<YYYY-MM>/` and proposes
  page changes, new pages and rulings on Review (one planning ask;
  Ways-of-Working/Research/Templates/_meta off-limits); the source waits in
  Raw (`in_review`, `waiting_new`). **Done, file it** (`ingest_file`) moves
  it beside the note.
- **Your day, memory built in, no pipeline (2026-10-01).** Design:
  `docs/design/home-whitebox.html` (Y1, Y1b, Y1c). Home is Your day: the team
  digest (`team_digest`, stored per team in app data `team-digests/`),
  Tickets (`tickets/<ID>.md` in the team's repos, assigned to the git user,
  read-only) and Other (task files in `.ken-workspace/tasks/`: `title,
  status open|done, target, repeat, links, from, done_on, updated_by`, body =
  description), then recent files and index health. Logic in
  `ken-core/src/day.rs`; commands `day_state`, `day_task_create|update|delete`,
  `ticket_tasks`, `index_health(team)`; event `day-changed`. MCP:
  `task_create`, `task_update`, `task_list`, `ticket_list`. The task board
  (columns, goals, daily board, rollover, Tasks screen), the pipeline board
  and the `kenTasks`, `kenFamilies`, `kenPipeline` flags are removed; they
  remain on `ken-workspace-home`. `kenMemory` is built in. The team inbox
  stays: Accept puts a task on my family board, which Your day lists.
- **Ingest runs on its own (2026-10-01).** Per the latest Ways of Working
  (white box 8b; `templates/wiki` CLAUDE.md, Current/Index,
  Research/Ingestion/Index and Ingested-note, copied byte for byte): an
  ingest runs as soon as a source is dropped in and nobody confirms it; a
  person reads the card afterwards and can undo. The note has Summary, Key
  Takeaways, What It Overturns, Contradictions, What Was Said, Rulings,
  Actions and Requests, Escalations, Next Steps (an old note's Open
  Questions read as escalations). `ingest::follow_ups` writes at once, each
  citing the note and recorded on the card as a `Written` (edit with its
  `base`, created file with its hash, task with its id): page edits and new
  pages, ideas `ideas/I-nnn.md` and escalations `escalations/E-nnn.md` in
  the team repo, my next steps on Your day (`updated_by: ingest`; me =
  `day::Me` or my first name). Four things wait, as page-proposal items:
  a ruling (Accept only as its decider; the team repo's
  `decisions/DECISIONS.md` when it has one), one ticket for all
  Ways-of-Working/Conventions changes (no longer dropped), an edit staging
  held (more than a fifth of the page's lines, or the page changed during
  the run), and each action as a ticket. Without a team repo, ideas,
  escalations and tickets stay listed on the card. `ingest_undo_write`
  undoes one write (refused when its file changed since); `ingest_undo` is
  Undo all, last write first. Ingest screen rebuilt to 8b (Apply all
  gone); the rail's Ingest count is sources with something waiting.
- **Cleanup: Ingest works, Windows parity, the right models (2026-10-01).**
  A failing source no longer stops the pass; recordings and audio are
  transcribed first (symphonia in-process, ffmpeg.exe found on Windows as a
  fallback); any repo with an inbox is the library and the watcher starts a
  pass. Record lives in Ingest (cpal mic, WASAPI loopback on Windows). Review
  is removed; recipes and automations are removed. Windows: no console
  windows (`proc::quiet` in `spawn_with_input`), single instance,
  Windows.Media.Ocr (`ocr/win.rs`), vulkan-1.dll delay-loaded with a CPU
  fallback (`src-tauri/src/vulkan.rs`), static CRT, OneDrive error codes,
  Claude Code's trust and session folder encoding. Models: `compute.rs`
  (graphics card setting, CPU fallback, threads); embedding profiles
  (`embedder::NOMIC`, `QWEN3`), a model switch drops the vectors and re-reads
  (`engine::needs_reembed`, `Db::reset_vectors`), one shared embedder locked
  per batch (`shared_embedder`, `SlotEmbedder`); Whisper Base, Small, Large v3
  Turbo q5_0 with language detection for the multilingual one. The local
  language models are not offered: Claude builds the map. Chat and MCP:
  every listed tool dispatches, CRLF-safe edit proposals, the guide names
  `mcp__ken__*` tools, the chat may write tasks and memories, MCP searches
  stay in the open workspace.
- **The Briefing design system (2026-10-02).** `docs/design/briefing/` (from
  Claude Design). `src/app.css` holds the tokens (old names are aliases) and
  the component set (`.btn` sizes, `.count`, `.k-pill`, `.k-segmented`,
  `.k-tabs`, `.k-action`, `.k-check`, `.k-empty`, `.k-progress`). Shell:
  `Sidebar.svelte` (team and sync, Home, Inbox, Files, Ingest, Explore,
  Settings, Collapse) and `TitleBar.svelte` (search, update, Ask Ken). New
  screens: `InboxScreen` (store `lib/inbox.svelte.ts`: people, Ken, sync and
  files), `ExploreScreen` (Timeline and Map). Home is the briefing; Settings
  has sections (General, Team library = the old Team screen, Sync with the
  team inbox, AI, Agents, Ingest rules). Not yet to the design: the Files
  document view's contradiction pill, the search overlay's "Ken's answer"
  card, Ask Ken's chat tabs.
- **Testing:** `TEST-PLAN-knowledge-layer.md` (one pass by hand) over
  `node scripts/knowledge-layer-fixture.mjs <folder>`.
  ken-core on this PC: 856 pass, 55 fail (all Windows-only: fake bash CLI, CRLF);
  ken-mcp 40/40; frontend 691/691 (plus upstream's release-script test, CRLF).

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
- **SentinelOne on this PC.** On 2026-09-25 it uninstalled Claude and
  quarantined files after a test run that wrote `.cmd` launchers into
  temp dirs (cmd → bash → curl) and used `taskkill /T /F`. It took
  `install.ps1`, `scripts/win-build.cmd`, `test-workspace.bat`,
  `drawio-viewer.min.js`, `svelte.config.js` and parts of
  `node_modules` (every `.bin/*.cmd` shim, `vitest/dist/cli.js`).
  All restored the same day (`git checkout`, `npm ci`, and a fresh
  extract of `tauri-plugin-dialog-2.7.1`, whose `api-iife.js` it had
  taken from the cargo registry). Never use the in-app browser here;
  run things one at a time.
- **ken-core tests on Windows:** 859 pass, 0 fail, 57 ignored. The 53
  that run the bash fake Claude CLI are ignored on Windows (covered on
  macOS/Linux). The rest were real: fixture PDFs were CRLF-converted
  (now `-text` in `.gitattributes`); `/etc` is rooted but not absolute
  on Windows (import and recipe path checks now use components); cloud
  hydration measured nothing on Windows (now `GetCompressedFileSizeW`);
  one test hard-coded `/`. ken-mcp 42/42. Frontend 697 pass,
  svelte-check 0 errors, `scripts\win-build.cmd check` passes.
- **Windows spawn fixes (real bugs):** a `claude.cmd` launcher cannot
  take an argument with a line break, so `-p` prompts go on stdin, the
  chat guide is one line, and a multi-line terminal-session prompt
  (research) goes in a temp file with a one-line pointer. Cancelling a
  session now ends node too: each child is put in a job object
  (`proc::track`), not `taskkill`.
- **End-to-end run on a real client workspace (2026-09-28).** Four
  members (two code repos, a notes vault, a design folder), about 3,400
  files, run through `examples/eval_run.rs` (a phase per step, ken-core
  calls as the app makes them). Fixed on the way:
  - Setup: a monorepo with its packages a level or two down is Code.
  - Claude discovery: a stale npm `claude.cmd` (exe renamed mid-update) is
    passed over, whole folder; winget's Links folder is a fallback.
  - Meaning index: an over-long chunk is truncated, a NUL is dropped, and a
    chunk the model refuses no longer stops the whole index.
  - Search: any-word keyword fallback when all-words finds too little;
    hits carry a relevance score, so members merge by relevance, not by
    repo; the page band is a bonus, not a wall; cosine from vec0's L2
    distance; word matches weighed by question coverage; one copy of a
    file kept in several repos. Curated questions: top-1 3 → 10 of 20,
    top-5 14 → 15.
  - Graph routing: graph members first, then the rest (code is never in
    the graph); it had cut code answers out.
  - Graph clusters: same name merges when a kind is loose (topic/other);
    fuzzy candidates need the shorter name nested in the longer (one shared
    word chained wrong merges).
  - Drift: derived labels (`repo:(layout)`) skipped, the wiki's own pages
    are cross-references, path case resolved, no-git folders unmeasured,
    templates skipped, a never-verified page ages from `updated`, the cache
    is versioned, each sweep fetches the remote, and a draft pins its code
    citations (`repo@sha:path`) so a same-day change is seen.
  - `.kenignore`: the workspace file's member lines reach inside members.
  - Local extraction: trailing commas and prose around the JSON forgiven.
  - Open: the 4B local model still fails about half the documents (often
    writes Markdown, not JSON) at ~42 s each; a llama.cpp grammar fixed the
    format but hit a native assert (abort), so it is not in. Claude's
    whole-project model build is the working path meanwhile. Paraphrased
    code questions ("stop people seeing projects…") still miss.
- **The local model embeds only (2026-09-28, the user's call).** Every
  generation runs through Claude (in-agent Ken): `LOCAL_GENERATION = false`
  in `src-tauri/src/lib.rs` gates the local paths (quick answers, profiler
  refine, journal distill, daily plan, per-file extraction, graph
  summaries). The map is Claude's whole-project build, automatic again
  (`map_by_claude`: first map once the first scan settles, a rebuild once
  changes go quiet, at most two an hour, the `should_auto_build` policy
  Arthur kept for the manual rebuild when he retired the tick in `e32ee00`).
  A build settles the per-file queue it covered. Graph judgements (same
  thing? how related?) go to Claude via `FederationLlm::judge`; summaries
  take the longest member summary. The UI's model status now means "Claude
  Code installed" and its notices say how to install it. The local JSON
  path is fixed anyway: `sample()` already accepts the token, the second
  `accept` fed the grammar every token twice (the abort); with a JSON
  grammar (`common` feature) 9 of 9 hard documents parsed.
- **Benchmark:** `C:\ken-eval` (not in git): 30 questions on the ATT
  workspace (14 top-1, 21 top-5) and 20 on a Ken/Headway/Ways-of-Working
  workspace (7, 14). Run `eval_run ask` after any search or graph change.
  Open: the new wiki's Ways-of-Working template pages rank in the top band
  and surface in unrelated questions; code questions often get the spec
  or design doc first.
- **Code map, history, fallback, ranking hygiene (2026-09-28).**
  - `codemap.rs`: tree-sitter tags (Rust, Python, JS, TS/TSX, Go, Java,
    C#) into `code_symbols`/`code_imports` (schema 15) at scan, backfilled
    once from stored text (`code_map_backfilled` meta). Definitions, calls
    with the enclosing function (callers), whole-word mentions for uses the
    tags miss (`Depends(fn)`), outlines, imports resolved to files both ways.
  - MCP: `find_definition`, `find_usages`, `file_outline`, `related_files`,
    `history` (git log by path, or by message and `-S` change; read-only).
    Allowed in chat; the guide says to navigate code like an IDE.
  - Files: `CodeMap.svelte` under code files (outline, imports, imported
    by, usages across the workspace); `code_file`/`code_usages` commands.
  - Search: a query naming a symbol leads with its definition (W_SYMBOL).
  - Fallback: chat gets `--add-dir` for every workspace repo and a rule to
    look (Grep/Glob/Read) before saying something is absent; empty MCP
    results say so; ⌘K "Ask Ken to look" runs `assistant::look` (read-only
    tools only) and returns ken:// sources.
  - Ranking: minified/maps/lockfiles ignored by default; one-line machine
    text −1.5; duplicate passages collapse across repos; pages verified over
    180 days ago −0.25; question shape nudges code vs prose ±0.5; the top
    page hits' wiki links rise (never past them) and up to 3 linked pages
    join.
- **Second end-to-end run, fresh (2026-09-28/29).** Clean clones of the ATT
  repos under `C:\ken-eval\fresh` with their own scratch remotes
  (`fresh\remotes`, never the real ones). Phases added to `eval_run`:
  `imports`, `look`, `map` (Claude's build per member), `add-repo`, `sync`,
  `chat` (`assistant::chat_oneshot`: the chat's guide, Ken's MCP server,
  read-only). Fixed on the way:
  - Embedding: one model context per batch, not per chunk (279 s → 83 s on
    2,596 chunks). Packing several chunks per call was slower; reverted.
  - First draft: writes Vocabulary; names the Domain section; the Review
    card lists the placeholders left for a person; cites once per
    paragraph or row; release notes add an Unreleased section from the
    commits after the newest tag; a Repo Map page reads the imports between
    folders from the code (call order, folders importing each other, the
    most imported files). `resolve_import` 37 s → 2.4 s on 1,796 files.
  - Drift: a repo checked out on a branch the default does not contain is
    pinned to the commit read and measured against its own upstream.
  - Search: a page titled with a `{{placeholder}}` ranks under real pages.
  - Look: code wins over tickets on what the system does; Glob/Grep need
    each folder as the path; the look gets the files search ranked.
    16 questions search missed: 4 → 12 cited the right file.
  - Graph: a first name alone never merges with a full name; a merged
    kind is voted (a specific kind needs half the votes).
  - MCP history: an old commit's moved path is text, not a dead address.
  - Chat: links a sibling repo's file by its ken:// address (each turn
    lists them; `inMember` also opens `Repo Name/path` links there); checks
    every repo before saying the code does not do something. 30 questions
    graded by hand: 29 answered correctly with sources; the one wrong
    (answered from the older prototype only) is right after the rule.
  - Results: ATT 14/21 (hybrid top-1/top-5), 15 of 30 routed by the graph
    now, same score; Ken workspace 8/14; code filtered 4/7 and 4/9.
  - Not done: template updates reaching existing wikis (needs a record of
    the template version a wiki started from); platforms and tools land in
    the `other` entity kind.
- **Workspaces only, and the Wright screens for knowledge (2026-09-29).**
  - Built in, not flags: `workspace`, `federatedKg`, `kgRouting`
    (`features::BUILT_IN`; the app's and ken-mcp's checks return true).
    "Search by meaning" defaults on per repo.
  - Start screen: Open a workspace, Set up a new workspace, Recent
    workspaces (`list_recent_workspaces`, `forget_workspace`); no single
    project. Launch reopens the last workspace.
  - Title bar: the team switcher (the workspace's groups, one each, no
    "all"; `scope.team`, kept per workspace). Files lists the team's repos
    above the tree. Search, Home's scope and digest follow the team.
  - Ingest (white box 8b): drop or choose files into the team wiki's Raw
    (`ingest_add`, `ingest_add_bytes` raw body); `ingest_overview`,
    `ingest_card`. The note is written against the wiki (`wiki_context`)
    with a Contradictions section. The card: takeaways, then proposals
    (pages, new pages, rulings, tickets) with diff and Apply / Discard,
    Seen / Undo all. Every ingest command targets the team's library
    (`inbox_member`), not the focused repo. Recipes and Automations left
    the UI; the engine still runs any saved on disk (no new ones can be
    made): stop it running them when their tests can be run on macOS.
  - Team (white box 9): `team_overview` (repos with index, kind, branch,
    commit, behind upstream; gaps; ignores; the wiki's sweep; rules and
    templates), `team_save_ignores`, `team_add_rule`; Add a repo, Scan
    again, Take out. Not built, being Wright's: roster, epics, protected
    paths, Leave this team.
  - Review names what waits in the team's other repos
    (`team_review_counts`); the rail count is the team's.
  - Chat: `history` answers "what changed lately" (`days`), git or not.
  - The confirm-first ingest decided here on 2026-09-29 is replaced: see
    "Ingest runs on its own (2026-10-01)".
- **Chat streams, with tool cards (as in Headway).** The chat CLI runs
  with `--include-partial-messages`; `chat::parse_events` reads every
  block of a line. Text deltas go out as `chat-delta` (shown, not kept)
  until the whole reply lands. Each tool call is a `tool` message
  (`{toolUseId,name,summary,status}`), updated in place with its status
  and the start of its result. UI: `src/chat/ToolCard.svelte`,
  `toolCard.ts`, `chats.draft`. Test plan §15.

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
