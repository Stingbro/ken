# Test plan — Ken at Level 2 (`knowledge-layer`)

One pass by hand, in the app, over a throwaway folder. Every check says what
to do and what you should see. Mark each one ✓, ✗ (with what you saw instead),
or skipped. Nothing here touches a real repo.

Steps refer to the report "Ken at Level Two" and to `SESSION-HANDOFF.md`.

## 0. Before you start

- [ ] **Build and start Ken.** From `C:\Code\ken`:
  `scripts\win-build.cmd dev`. Close any Ken window started before; Windows
  locks the old exe.
- [ ] **Make the test folder.**
  `node scripts\knowledge-layer-fixture.mjs C:\Users\<you>\ken-test-folder`
  It prints the three folders it made: `Realms-Docs` (a wiki from the method's
  templates), `Realms-Game` (code, with a worktree `Realms-Game-u7` beside it),
  `Personal` (documents, no git).
- [ ] **Optional pieces.** Some checks need more than Ken:
  - the Claude Code CLI, logged in (`claude` once): ingest (§9), the first-wiki
    draft (§3c);
  - an on-device model chosen in Settings: the graph and Map (§6, §10);
  - `ken-mcp` installed by `install.ps1`: the connector path (§11).
  Skip a check that needs what you have not set up, and say so.

## 1. Set-up offered on a fresh start (step 4)

- [ ] On the start screen, the first choice reads **Set up Ken for your code**,
  even if you have never turned on any feature.

## 2. Repos, Team (step 4)

There is no "where is your code?" step: you pick the repos themselves.

- [ ] **Pick repos…**: select `Realms-Docs` and `Realms-Game` together (the
  dialog takes several). Then **Add more repos…**: pick `Realms-Game-u7` and
  `Personal`.
- [ ] The **Repos** table:
  - `Realms-Docs`: kind **wiki**, **read for entities**, evidence mentions
    `_meta/DECISIONS.md` and an Obsidian vault; its description is prefilled
    from its README ("The wiki for Realms-Game.").
  - `Realms-Game`: kind **code**, **searchable only**, evidence mentions
    `Cargo.toml` and its remote; description from its README.
  - `Personal`: no kind, **read for entities**, "no git · documents".
  - `Realms-Game-u7`: unticked, **not indexed**, "a second checkout (git
    worktree) of another repo".
  - Each row shows the repo's full folder.
- [ ] Write a description on `Realms-Game` ("The game: Rust, saves by
  region."). Add more repos again: your text is still there.
- [ ] ✕ on `Personal` takes it out.
- [ ] (A folder of repos) Pick the whole test folder: its repos appear as rows,
  not the folder itself.
- [ ] No file has appeared in the test folder yet (no `.kenignore`, no
  `.ken-workspace`).
- [ ] **Team**: `Realms-Docs` and `Realms-Game` show team **Realms** (the docs
  README names the game), and "Suggested: Realms".

## 3. Index and Confirm (steps 4, 2, 4b)

- [ ] **Index** counts what you kept (with Personal taken out and the worktree
  off: **1 read for entities · 1 searchable only · 1 not indexed**).
- [ ] (3c, needs Claude) Tick **Draft the first wiki pages from these repos**;
  it offers `Realms-Docs`. Leave it ticked if Claude is set up.
- [ ] Confirm. The workspace opens.
- [ ] The test folder is still untouched: no `.kenignore`, no `.ken-workspace`.
  The workspace is in `%APPDATA%\ken\workspaces\<id>\.ken-workspace\workspace.json`:
  it lists the members, a `paths` entry per member with its folder, and a
  group **Realms**.
- [ ] Settings > Projects > **Repos** shows each repo's description, editable.
- [ ] (3c) The first-wiki draft's Review card lists
  `Realms-Game:(what this repo is for, in the team's words)` among its sources.
- [ ] (3c) Within a few minutes, Review has **First wiki drafted: N pages to
  read**. The drafted pages (for example `Current/Project.md`) say
  `status: draft`, have no `verified:` line, cite sources like
  `Realms-Game:README.md`, and a page you had edited beforehand is listed as
  left alone.

## 4. What Ken reads, and what it never reads (steps 2, 3, 5, 7)

- [ ] Search **token** or **API_KEY**: nothing from `credentials.json` or `.env`.
- [ ] Search **world**: no `backups/world.zip`.
- [ ] Nothing from `Realms-Game-u7` appears in any search.
- [ ] Focus `Realms-Game`: Home's footer shows its files as **searchable only**
  and nothing waiting to be read for the graph.
- [ ] Settings > Projects > **Repos**: change `Personal` to **searchable only**.
  After the next scan (switch to it, or wait), its Home footer counts the files
  as searchable only, and nothing of it waits for the graph.
- [ ] Touch a file without changing it (open `Realms-Docs\Platform\Save.md` in
  an editor and save it unchanged, or change its date). Ken does not re-read it:
  the search results are unchanged and nothing new waits for the graph.

## 5. Sync stays out of code (step 1)

- [ ] Focus `Realms-Game` (it has a remote). Settings shows sync **off**, and
  the title-bar dot reads **Local project**. `git log` in `Realms-Game` shows no
  "Ken: update knowledge" commit, ever.
- [ ] Settings > Repos: the kinds are as set-up proposed. Setting a repo's kind
  to **wiki** would turn sync on for it (don't do this on a real repo).

## 6. Home health (step 6)

- [ ] Focus `Realms-Docs`. Home's footer has a second row: **N of M read for
  the graph**, and, if anything waits, **waiting to be read**.
- [ ] With no on-device model chosen: one line says **No on-device model is
  chosen … Search still works.** The Timeline shows the same notice.
- [ ] No "index check missed" line (the control page is `START-HERE.md`).

## 7. What a search hit carries (steps 8, 9, 11)

Use the search box (Ctrl+K) with **This project** scope, on `Realms-Docs`.
None of this needs Search by meaning or a model: a plain scan now builds the
keyword index over sections.

- [ ] Search **region**: `Platform/Save.md` comes back.
- [ ] Search **shard** (on no page; the Vocabulary page pairs it with
  "region"): `Platform/Save.md` still comes back.
- [ ] Delete a page in Files, then search a word only it had: it no longer
  comes back.
- [ ] (2b) Search **region** again and press **For business readers**: only
  `Design/Why-regions.md` (a business page) is left; `Platform/Save.md` (dev)
  is gone. Press it again to see both.
- [ ] (MCP, optional) Through `ken-mcp`, search **region**: the line starts
  with a locator like `[[Save]] · Realms-Docs:Platform/Save.md:1`. The app's
  result list does not show the locator yet.
- [ ] A page under `Ways-of-Working/` ranks above a `Research/` page for the
  same word (search a word both have, for example **ingest**).
- [ ] (MCP, optional) Through `ken-mcp`, a search result line starts with the
  locator, and a Research hit says "evidence from <date>, not current state".

## 8. Links (step 10)

- [ ] Review has **Links: … missing, … broken, … ambiguous** for `Realms-Docs`.
  It lists **[[Ghost Page]]** as missing, **Platform/Gone.md** as broken under
  Platform, and no `{{…}}` placeholder anywhere.
- [ ] Open `Platform/Save.md` in Files: under it, **Links to: Combat** and
  **Linked from: Combat**. Clicking one opens it.

## 9. Ingest (step 12, needs Claude)

- [ ] The rail item reads **Ingest**; its first tab **Ingest** shows
  `2026-09-20 standup.txt` waiting. (Tabs: Ingest · Recipes · Automations.)
- [ ] **Read them now**. When it finishes: the Raw folder is empty;
  `Research/Ingestion/Ingested/<today>-2026-09-20-standup.md` exists, with
  `status: evidence` and `source:` pointing at the moved file beside it.
- [ ] Review has **Ingested: …** with what it overturns (the ship date) and the
  actions (Ben, the save bug). **Undo** puts the source back in Raw and removes
  the note.
- [ ] Recipes > new from template: **Current, from ingested notes** is offered,
  with its source set to `Research/Ingestion/Ingested` and output `Current/`.
  (Run it only if you want to see a held first run: over the template pages it
  should wait on Review, not apply.)
- [ ] (Optional) Record a short clip in Ken while `Realms-Docs` is focused: the
  transcript lands in `Research/Ingestion/Raw/`, the audio in `Recordings/`.

## 10. Drift (step 13)

- [ ] Review has **Drift: … gone, … to re-read, … not verified in 30 days** for
  `Realms-Docs` (it runs after the first scan). It says:
  - **Findings**: `Platform/Legacy.md` cites `src/old.rs` (gone) and
    `src/save.rs:99` (past the end of the file);
  - **Is the page still true?**: `Platform/Save.md` (save.rs changed after its
    verified date), and the decision `D-001`;
  - **Combat.md is not listed** (its code change was a comment): it is counted
    as cleared;
  - **Design/Why-regions.md is not listed**, though it cites the same changed
    code as Save.md: it is a business page, and the report says business
    pages are checked by age only (2b);
  - **Not verified in 30 days**: `Platform/Legacy.md`, and the template pages
    (never verified);
  - that no controls are declared.
- [ ] Nothing in the wiki was edited by the sweep.
- [ ] Home › **Check for drift now** with nothing committed in `Realms-Game`: the card
  reads the same. Then commit a real change to `src/combat.rs` in
  `Realms-Game` and check again: `Platform/Combat.md` now appears under
  **Is the page still true?**, and nothing else changes. (The sweep asks git
  once per repo what changed and re-measures only those files.)

## 10b. The watcher at scale

- [ ] With `Realms-Docs` focused, edit and save `Platform/Save.md` outside
  Ken. Search finds the new words within a couple of seconds.
- [ ] Create `Realms-Docs/build/` with a file in it and add `build/` to its
  `.gitignore` first: the file never shows in search.

## 11. Settings and the team (step 15)

- [ ] Settings > Features lists plain names: **Search by meaning**, **Background
  reading**, **Team knowledge graph**, …, each with where it applies and its
  code name small.
- [ ] Settings > Working with agents: with `ken-mcp` installed by
  `install.ps1`, the connector shows its path under
  `%LOCALAPPDATA%\Programs\ken-mcp`. (In a dev build without it installed,
  "not found" is expected.)
- [ ] (Optional, needs a Ken team/family) The family tray has **Send**: pick a
  member, a kind, a title; it lands in their inbox.

## 12. The graph (step 14, needs a model and the graph features)

- [ ] Settings > Features: turn on **Team knowledge graph** and **Search across
  the team**; let background reading run. The graph is built over
  `Realms-Docs` and `Personal` (entity repos), not `Realms-Game`.
- [ ] Its file is `<test folder>\.ken-workspace\kg.sqlite`.
- [ ] With **Realms** picked in the scope picker, the graph search shows only
  entities from Realms repos.

## 13. Adding repos later (step 4)

- [ ] Settings > Projects > Repos > **Add repos…**: pick `Personal`. It waits
  in a panel with its proposed kind, a team box and its README description;
  nothing joins until **Add**.
- [ ] Set its team to `Realms` (the team whose wiki is `Realms-Docs`). The
  **Update the team's wiki** box appears, ticked. Click **Add**. A note says
  `Realms-Docs` is being updated.
- [ ] Focus `Realms-Docs` (needs Claude). After a minute or two:
  - `Repo-Map/Personal.md` exists, `status: draft`, citing `Personal:` sources.
  - Review has one card "Personal added to the wiki: … drafted, … proposed".
  - Each page a person keeps that the repo changes (the architecture, who does
    what, the project, the vocabulary, the Repo Map index) has its own card,
    "Proposed: <page> with Personal", showing the change as a diff.
  - **Apply the change** writes it. Edit a proposed page first, then apply its
    card: it refuses and says the page changed. **Mark as done** discards.
- [ ] Rename a member's folder; **Scan again** names it as gone.

## 14. A new team wiki (the wiki at team level)

- [ ] Start set-up again (start screen › Set up Ken…). Pick `Realms-Game`
  and `Personal` only, not `Realms-Docs`. In **Team**, set both to `Realms`.
- [ ] **Each team's wiki** lists `Realms` with **No wiki for now**. Choose
  **Create a new wiki…**; the name reads `Realms-Wiki`. **Next** stays off
  until **Choose where…** picks a folder (the test folder).
- [ ] Confirm with **Draft the first wiki pages** ticked. On disk,
  `<test folder>\Realms-Wiki` has the template's pages and a `.git` with one
  commit. `START-HERE.md` lists `Realms-Wiki`, `Realms-Game` and `Personal`
  with what each is for. No page has a `verified:` date filled in.
- [ ] Settings > Projects > Repos lists `Realms-Wiki` as a wiki on `Realms`.
- [ ] With Claude, after a few minutes: `Repo-Map/Realms-Game.md`,
  `Repo-Map/Personal.md` and `Repo-Map/Index.md` exist, then Current/Project,
  Team, Who-Does-What, the architecture page and Work/Releases. Review on
  `Realms-Wiki` has one card listing them all.
- [ ] Nothing was pushed anywhere: `git -C "<test folder>\Realms-Wiki" remote`
  prints nothing.

## 15. Claude in chat: edits reviewed, sources clickable (needs Claude)

- [ ] Open `Realms-Docs/Platform/Save.md`. In chat, ask: "Rename the title to
  Saving and add a line that saves are versioned." A card shows the edit as a
  diff, each change with its old and new line numbers, and the same diff sits
  over the open page. Nothing is written yet.
- [ ] Deny the title change, Accept the other, **Apply choices**. Only the
  accepted line lands on disk; Claude's next message says which change was
  left out and does not redo it.
- [ ] Ask for another edit and **Deny all**: the file is unchanged. Ask again
  and **Accept all**: Claude makes the edit as proposed.
- [ ] Edit the page yourself while a proposal waits, then Apply choices: it
  refuses and says the file changed.
- [ ] Ask "Where do we decide how saves are written?" The answer cites files
  as links. Clicking one opens it in Files at the cited line or heading, which
  flashes. Claude never opens a tab or switches screen by itself.

## After

- [ ] Close Ken; delete the test folder and `Realms-Game-u7`/`u8` beside it.
  The test repos are registered in Ken's list: remove them from the start
  screen (right-click › Remove from Ken).

## Known gaps (not bugs to report)

- Set-up still writes `.ken/project.json` into each member, code repos too.
- Ingest does not yet write rulings, tickets or Current pages from a note by
  itself; the Current recipe does the last, through staging.
- Chat edits to wiki pages do not go through staging.
- 2b is built by section; the all-projects search and the MCP do not filter
  by audience yet (the project search box does).
