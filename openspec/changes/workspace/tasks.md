# Tasks: workspace

## 1. ken-core

- [ ] 1.1 `workspace.rs` (new): `WorkspaceConfig { name, id,
      members, #[serde(flatten)] extra }`; atomic save / tolerant
      load mirroring `project.rs`; `Workspace::create(parent, name,
      member_names)` (mkdir `.ken-workspace`, write manifest,
      `Project::create` per member), `Workspace::open(parent)`
      (missing members → `MemberStatus::Missing`, never fatal);
      register in `lib.rs`
- [ ] 1.2 `workspace.rs`: `discover_candidates(parent) ->
      Vec<Candidate { name, existing, file_count, markers }>` — one
      level, skip hidden/`.ken-workspace`/`node_modules`/`target`
- [ ] 1.3 workspace tests: create-then-open round trip, unknown-key
      preservation, adopt existing manifest, missing member reported,
      relative-member resolution after parent rename (tempdir),
      discovery fixture folder (existing project, plain repo, junk
      dirs excluded)
- [ ] 1.4 `registry.rs`: recent-workspaces list (path, name, last
      focused member id, opened_at) beside recent projects; tests

## 2. src-tauri — extraction commit (no behavior change)

- [ ] 2.1 Extract `ActiveProject` fields into `ProjectHandle`;
      introduce `enum AppMode { Single(ProjectHandle), Workspace
      {...} }` with only `Single` constructed; replace every
      `state.active.as_mut()...` with `focused(&mut state)`;
      **all existing tests green before proceeding**

## 3. src-tauri — workspace mode

- [ ] 3.1 `open_workspace(parent)` / `create_workspace(parent, name,
      members)`: build `ProjectHandle` per member (lazy beyond 12,
      LRU), ingest semaphore (max 2 concurrent), start watchers;
      `workspace-state` and `member-status` events
- [ ] 3.2 `workspace_overview()` (members + status + counts),
      `focus_project(id)`, `discover_workspace_candidates(parent)`;
      close path tears down all handles; register commands
- [ ] 3.3 `search_all_projects(query, limit)`: per-member FTS,
      round-robin rank interleave, member-name labels
- [ ] 3.4 Global `workspace` flag read; flag off → workspace commands
      return a friendly "feature disabled" error

## 4. Frontend

- [ ] 4.1 `api.ts`: workspace types (`Candidate`, `MemberStatus`,
      `WorkspaceOverview`), command wrappers, event listeners
- [ ] 4.2 Launcher: "Open a workspace" (flag-gated) → folder pick →
      candidate checklist (existing pre-checked, marker captions,
      include toggles) → name → Features disclosure → create;
      recent workspaces section
- [ ] 4.3 Nav-rail project switcher: workspace name, member list with
      status dots, click/`Ctrl+P` cycle to `focus_project`; screens
      reload their stores on focus-change event
- [ ] 4.4 ⌘K "All projects" scope toggle (workspace mode only):
      labeled results, selection switches focus then opens

## 5. Verification

- [ ] 5.1 cargo test --workspace, pnpm test, pnpm check, pnpm build
      green; extraction commit verified independently
- [ ] 5.2 Manual: create a workspace over the real parent folder
      (7 members), watch staggered ingests, switch focus, all-projects
      search, close/reopen restores focus
