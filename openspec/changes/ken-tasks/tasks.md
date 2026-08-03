# Tasks: ken-tasks

## 1. ken-core

- [ ] 1.1 `tasks.rs` (new): task frontmatter model (`id` ulid,
      `title`, `status`, `kind`, `assignee`, `project`, `tags`,
      `due?`, `board`, `created`, `updated`; `#[serde(default)]`,
      flattened extras), parse/serialize preserving unknown keys +
      key order + body bytes; filename `<ulid>-<slug>.md` with `id`
      authoritative; register in `lib.rs`
- [ ] 1.2 `tasks.rs`: patch rewrite core — `apply_patch(file,
      patch)` rewrites only named keys + `updated`; invalid `status`
      in a parsed file ⇒ `NeedsAttention`, never a crash or silent
      rewrite; optimistic-concurrency guard per S6 (mtime/hash
      precondition before write, retry on mismatch); raw
      line-splitter core, not serde_yaml (S6); handle multi-line
      values on patched keys (S6 prototype TODO)
- [ ] 1.3 `tasks.rs`: home scanning + aggregation (workspace home +
      any `<project>/.ken/tasks/`), `project` defaulting from
      per-repo home; filter matching (status/project/tag/assignee/
      kind) shared by UI and `task_list`
- [ ] 1.4 `tasks.rs`: archive pathing (`tasks/archive/YYYY-MM/`
      within the task's own home); `task_complete` log append
      (`## Log` + timestamp) and journal summary line composition
- [ ] 1.5 `tasks.rs`: daily rollover detection (board daily, status
      ≠ done, `updated` < today) and the three resolutions (roll /
      promote / archive) as pure transitions
- [ ] 1.6 `tasks.rs`: goal file model (`tasks/goals/` in the
      workspace home; `id`, `title`, `status active|done|dropped`,
      `created`, `updated`, body; same patch core), `goal` in
      filter matching, derived progress counts (done/total per
      goal), unknown goal id ⇒ `NeedsAttention`; `backlog` as the
      default status on create
- [ ] 1.7 Tests: round-trip with unknown keys and hand-edited
      bodies; patch touches only intended keys; filename rename
      doesn't change identity; filter table tests incl. goal;
      progress count cases; archive path cases; rollover cases
      incl. repeated rollover; invalid-status and unknown-goal
      trays

## 2. src-tauri

- [ ] 2.1 Board state: scan on workspace open, watchers on both
      home kinds, self-write dedupe by content hash; `board-state`
      events; flag-gated by `kenTasks`
- [ ] 2.2 Commands: `task_create`, `task_list`, `task_update`,
      `task_complete`, `task_archive`, `board_get`; drag-drop uses
      `task_update` (status only); goal commands `goal_create`,
      `goal_update`, `goal_list` (chat tools wrap the same); board
      state carries per-goal progress counts
- [ ] 2.3 Daily flow: "plan my day" chat path drafting candidates
      from `read_journal` + recent ingest activity with approval
      cards; new-day rollover prompt with per-task roll / promote /
      archive
- [ ] 2.4 ken-memory integration: `task_complete` writes the
      journal summary line when `kenMemory` is on; skipped cleanly
      when off
- [ ] 2.5 Indexing tier: per-repo `.ken/tasks/` added to built-in
      search-only rules (kenignore defaults); workspace home covered
      by pseudo-member rules

## 3. ken-mcp

- [ ] 3.1 `task_create`, `task_list(filter)`, `task_update(id,
      patch)`, `task_complete(id, report)` delegating to core; tool
      descriptions document the claim convention (check unclaimed →
      set assignee + doing) and the complete→journal flow;
      `task_list` filter includes `goal` (no separate goal tools);
      absent when `kenTasks` is off
- [ ] 3.2 Tests: schema round-trips; claim sets exactly
      assignee+status; complete appends log; flag off ⇒ not listed

## 4. Frontend

- [ ] 4.1 `api.ts`: task/board types, command wrappers,
      `board-state` listener
- [ ] 4.2 Tasks tab in left sidebar (workspace mode, flag on):
      Kanban columns by status with `backlog` leftmost as the
      intake column, cards show title/project/tags/assignee/kind
      badge + goal chip; filters for project/tag/assignee/kind/
      goal; group-by-goal board mode with derived n/m progress per
      goal; goal create/edit dialog; needs-attention tray
- [ ] 4.3 Drag-drop between columns → `task_update`; card click
      opens the task file in the normal document view
- [ ] 4.4 Daily board view (`board: daily`); daily proposal
      approval cards; rollover prompt UI
- [ ] 4.5 Archive action on done cards

## 5. Verification

- [ ] 5.1 cargo test --workspace, pnpm test, pnpm check, pnpm build
      green
- [ ] 5.2 Flag off: no tab, no tools, no watchers, no folders —
      byte-identical
- [ ] 5.3 Manual: create tasks in both homes, filter, drag through
      the lifecycle; edit a file by hand mid-session and see the
      board update; claim + complete a task from an MCP client and
      see the log entry, done column, and journal line; run a
      rollover morning with all three resolutions; create a goal,
      tag tasks from both homes, group by goal, and watch the n/m
      progress update as tasks complete
- [ ] 5.4 Round-trip abuse: hand-add unknown frontmatter + drag the
      card twice; confirm the file diff is only status/updated each
      time
