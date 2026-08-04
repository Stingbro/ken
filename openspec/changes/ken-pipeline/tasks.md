# Tasks: ken-pipeline

Read `proposal.md` → `design.md` → `specs/ken-pipeline/spec.md`
before starting. Two sequencing rules this file encodes:

- **D6**: do not implement auto-transitions (2.12) until 5.4 has
  passed once.
- **D5**: the blocked refusal in `admit()` (1.8) is the invariant
  most likely to be broken by a later change. It lives in exactly one
  function and every caller — UI, MCP, and any future
  auto-transition — must go through it. Never re-implement it.

## 1. ken-core

- [x] 1.1 `pipeline.rs` (new): pipeline-definition model —
      `Pipeline { id, name, auto, concurrency_cap, bounce_cap,
      lanes: Vec<Lane> }`, `Lane { id, name, maps_to: TaskStatus,
      agent, model, kickoff, on_pass, on_fail, writes_code, human,
      terminal, generative, blocked, runner }`; `#[serde(default)]`
      on every optional field plus a flattened extras map;
      parse/serialize via the `tasks.rs` patch core so unknown keys,
      key order, and the body survive byte-for-byte;
      `pipelines_dir(workspace_root)`; validation that at most one
      lane sets `blocked: true` and at most one sets `human: true`;
      register in `lib.rs`
      - Note: parsed from a tolerant `serde_yaml::Mapping` rather than a
        `#[serde(default)]`+`flatten` struct — flatten inside a lane
        sequence is exactly where serde_yaml is fragile, and the mapping
        read gives the same defaults plus the extras map. Writes go
        through `patch_pipeline_text` → `tasks::patch_text`, so byte
        fidelity is the shipped patch core's, not a second one.
      - Note: `validate_pipeline` *reports* issues rather than failing the
        parse (a bad definition must not take the board down); it also
        catches duplicate lane ids, bad `maps_to`/`kickoff`, and dangling
        `on_pass`/`on_fail` targets.
- [x] 1.2 `pipeline.rs`: lane resolution — `resolve_lane(&Pipeline,
      status_raw) -> Option<&Lane>`, lane index lookup, and the
      `maps_to` projection. This is the single home of the
      board-scoped vocabulary check (D2); nothing else parses a lane
      - Note: an *empty* `status` on a pipeline ticket resolves to the
        first lane, mirroring `parse_task`'s "absent status ⇒ the intake
        column" — for a board-scoped vocabulary the intake column is
        whichever lane the human put first (D1: lane order is column
        order).
- [x] 1.3 `tasks.rs`: lane-aware parse — `Task` gains
      `lane: Option<String>`; when `pipeline` is set, `status` is
      derived from the resolved lane's `maps_to` instead of
      `TaskStatus::parse`; `status_raw` unchanged. `TaskFilter` gains
      `lane`, `pipeline`, and `blocked: Option<BlockedFilter>`
      (`Any | Blocked | NotBlocked | By(String) | NewlyUnblocked`) —
      all `Option`, all `#[serde(default)]`, and `matches` honours
      them. **No change to the pipeline-less path.**
      - Note: `parse_task` still leaves `lane: None` — it has no pipeline
        handle by design. `pipeline::resolve_task_lane` / `resolve_board`
        fill `lane` and re-derive `status` from `maps_to`, keeping the
        vocabulary check single-homed (1.2). A pipeline-less `Task` is
        byte-identical before and after resolution (asserted).
      - Note: `BlockedFilter` is evaluated from the ticket's own
        frontmatter only, so `matches` stays pure over one `Task`.
        "Blocked" therefore means *carries block evidence*
        (`blocked_by`/`block_reason`), not "status equals the blocked
        lane id" — the blocked lane id is data and `matches` has no
        pipeline. `NewlyUnblocked` = a surviving `return_lane` with
        nothing left holding it.
- [x] 1.4 `tasks.rs`: `needs_attention` takes the loaded pipelines as
      an extra input; new `AttentionReason::UnknownLane(String)`,
      `UnknownPipeline(String)`, `UnknownBlocker(String)`, and
      `UnknownReturnLane(String)`; `apply_patch` refuses a
      non-status patch on an unknown-lane ticket exactly as it
      already does for an invalid status
      - Judgment call: the signature was **added, not changed**.
        `needs_attention(tasks, goals)` and `apply_patch(path, patch,
        updated)` keep their exact shapes and delegate to
        `needs_attention_with_pipelines` / `apply_patch_with_pipelines`
        with an empty definition slice. Changing the signatures would
        break `src-tauri` and `ken-mcp`, which this session may not edit;
        the additive form also means a caller that doesn't know about
        pipelines conservatively refuses to edit around a lane it can't
        validate. Callers get rewired in 2.x/3.x.
      - Note: `UnknownLane`/`UnknownPipeline` *replace* `InvalidStatus`
        for a pipeline ticket rather than doubling up on it.
- [x] 1.5 `pipeline.rs`: ticket pipeline fields — read/patch helpers
      for `pipeline`, `model`, `agent`, `scope`, `verify`, `bounces`,
      `return_lane`, `blocked_by`, `block_reason`, `blocked_at`,
      `parent`, `spawned_by`, `origin`, `projects`, `target`. All
      ride the existing `extra` flatten on the task frontmatter;
      extend `TaskPatch` with the writable ones only (`blocked_by`
      renders as a sequence via the existing `seq_lines` path)
      - Note: `TaskPatch` gains `lane: Option<String>`, which writes the
        *same physical `status` key* as `TaskPatch::status` (D2: one
        status key, two vocabularies). `lane` wins when both are set.
      - Note: numeric/boolean values go through the shipped
        `scalar_lines`, which quotes every YAML-ambiguous scalar
        ("dates, numbers ... single-quoted, which is always safe"). So
        `bounces` lands as `bounces: '2'`, not `bounces: 2`, despite D4
        calling it a plain integer. Reusing the renderer beats forking a
        numeric one; `ticket_fields` reads both forms and it is asserted
        to round-trip to the integer 2.
- [x] 1.6 `pipeline.rs`: **the block model (D5)** — `Block {
      return_lane, blocked_by: Vec<Ulid>, reason: Option<String>,
      blocked_at }`; `block(ticket, &Pipeline, request) ->
      BlockResult` capturing `return_lane` from the ticket's *current*
      lane before the move (never supplied by the caller, so a
      blocked ticket without a return lane is unconstructable);
      `unblock(ticket, request)` clearing dependencies and/or reason
      independently. `blocked_by` holds ULIDs only — reject anything
      that parses as a path
      - `Block`'s fields are **private** with read-only accessors and
        `return_lane` is a plain `String`, so this module holds its only
        constructors and "blocked with no return lane" does not
        typecheck. `BlockRequest` has no `return_lane` field at all.
      - Judgment call: re-blocking a ticket already in the blocked lane
        **preserves** its recorded `return_lane` instead of capturing
        "blocked" as the return lane; if that field is missing/orphaned
        the block is refused (`MissingReturnLane`) rather than invented.
        The design didn't cover extending an existing block.
      - Judgment call: `blocked_by` entries are rejected as
        `PathBlocker` when path-shaped and `MalformedBlocker` when not a
        26-char Crockford ULID. The stricter half means a hand-created
        ticket whose `id` fell back to a non-ULID filename stem cannot be
        a blocker — deliberate, since the spec says "SHALL hold ticket
        ULIDs", but it is a real edge.
      - Judgment call: `unblock` resets `bounces` only when the reason
        matches D4's retry-cap prefix; a dependency-blocked ticket keeps
        its bounce history.
- [x] 1.7 `pipeline.rs`: **write-time cycle detection (D5)** —
      `check_cycle(&BlockGraph, from, to) -> Result<(), CyclePath>`,
      a depth-first walk over the existing blocked tickets; refuse
      the edge and report the full path. Called by `block()` before
      any write; pure and table-tested with direct, 3-hop, and
      long-chain cases
      - Note: each offered edge is checked against the graph **plus the
        siblings already accepted from the same request**, so a set of
        dependencies that only closes a cycle in combination is still
        refused.
      - Note: the walk carries a `seen` set, so it terminates on a graph
        that a hand edit has *already* made cyclic.
      - Note: a "shortcut" edge along an existing chain (A→…→F, then
        A→F directly) is correctly **allowed** — it adds no loop.
- [x] 1.8 `pipeline.rs`: admission — `admit(ticket, &Lane,
      &Pipeline, &[RunRecord], entry: EntryKind) -> Admission`
      returning `Start | Confirm{reason} | Queued | Refused{reason}`.
      Order is load-bearing: **(1) blocked ⇒ `Refused` first, before
      anything else**; (2) missing `scope`/`verify` ⇒ downgrade to
      `Confirm`; (3) `entry == Unblock` ⇒ downgrade to `Confirm` even
      for an `auto` lane; (4) gate mode; (5) concurrency cap over
      currently-`running` records ⇒ `Queued`. Pure; table-tested
      across the full cross-product
      - Blocked-first is implemented as the literal first statement and
        tested across ticket-shape × pipeline(auto on/off) × 6 lanes ×
        4 entry kinds × ledger(empty/full) — 240 combinations, every one
        `Refused{Blocked}`, plus a positive control in the same lane.
      - Step (3) `EntryKind::Unblock` is an **unconditional early
        return**, not a flag consulted later, and is backed by a
        property test that no `Unblock` input in any lane of any
        pipeline reaches `Start`. Comment states it becomes the
        load-bearing rule under a future `command` runner.
      - Judgment call: two refusals not in the numbered list sit between
        (1) and (2) — `human: true` lanes (D11: no agent, no kickoff)
        and `agent: none` holding columns. Both mean "nothing to run",
        so they are refusals rather than gates; they are placed *after*
        the blocked check so the ordering claim is unaffected.
      - Judgment call: entry-kind semantics the design left implicit —
        `Kickoff` on a `manual` lane yields `Confirm` (the spec bars a
        lane from starting *itself*, not a human from asking);
        `AutoTransition`/`Unblock` into a `manual` lane is
        `Refused{ManualLane}`; `Claim` treats the lane gate as already
        satisfied (the queued run record is the authorisation) but is
        still subject to the boundary downgrade and the cap.
- [x] 1.9 `pipeline.rs`: transition resolution — `advance(ticket,
      &Pipeline, outcome) -> Transition`, resolving `on_pass` /
      `on_fail`, classifying backward moves as bounces, incrementing
      `bounces`, and — when `bounce_cap` would be exceeded —
      returning a **block** result (D4: `block_reason: exceeded retry
      cap`, `return_lane` = the lane it was bouncing to) rather than
      a separate halted state. There is no `halted` field anywhere in
      this codebase
      - Signature deviation: `advance(ticket, &Pipeline, outcome, now)`.
        The cap-breach path has to write `blocked_at` and this module
        owns no clock (same caller-supplied-date convention as
        `tasks::create_task` and `memory.rs`).
      - Boundary asserted exactly: with `bounce_cap: 3`, bounces 0→1,
        1→2, 2→3 all move; the attempt that would make 4 returns
        `Transition::Blocked` with `block_reason: exceeded retry cap (4
        bounces)` and `return_lane` = the lane it was bouncing *to*
        (note this is the opposite capture rule from `block()`, per D4).
        `bounces: 4` is still written — the count is the evidence.
      - No `Halted` variant exists; grep confirms no `halted` anywhere.
- [x] 1.10 `pipeline.rs`: unblock evaluation —
      `evaluate_unblocks(&[Task], &Pipeline, terminal_ticket_id) ->
      Vec<Unblocked>`, resolving which dependents are now free (all
      `blocked_by` terminal **and** `block_reason` empty) and
      returning them with their `return_lane`. Callers re-enter them
      with `EntryKind::Unblock` (1.8), never with a start
      - `evaluate_all_unblocks(&[Task], &Pipeline)` added alongside for
        the workspace-open sweep the spec requires ("and again on
        workspace open").
      - Note: a `blocked_by` id that resolves to no ticket is **not**
        treated as terminal — an unresolvable dependency must not read
        as a satisfied one; it is an `UnknownBlocker` tray entry.
        Likewise a missing/orphaned `return_lane` is skipped, not
        guessed at.
      - The 1.10 → 1.8 handoff is tested end to end: what
        `evaluate_unblocks` returns, re-admitted with
        `EntryKind::Unblock` into an `auto` lane of an `auto: true`
        pipeline, is `Confirm{Unblocked}`.
- [ ] 1.11 `pipeline.rs`: run-record model + pathing —
      **(seam left by this session)** `RunRecord` + `RunOutcome` +
      `running_runs()` already exist in `pipeline.rs` because `admit`
      takes `&[RunRecord]`; the struct carries the full spec field list
      so 1.11 can adopt it as-is. Still to do here: `runs_dir` /
      `runs/YYYY-MM/<ulid>.md` pathing, parse/write, ledger scan, the
      derived queue view, and stale detection.
      `RunRecord { id, ticket, pipeline, lane, agent, model, scope,
      verify, started, ended, outcome, artifacts }` + body report;
      `runs/YYYY-MM/<ulid>.md`; ledger scan; derived queue view
      (`running` / `queued` / `blocked` / `waiting_human` / `stale`)
      — stale = `running` with no live run after restart, never
      auto-passed (D13)
- [ ] 1.12 `pipeline.rs`: sign-off child composition (D11) — from
      (parent ticket, comment) produce the child `NewTask` in the
      `todo` lane with `parent`, inherited `pipeline`/`project`/
      `projects`, **no** inherited `scope`/`verify`, plus the parent's
      `## Log` append and the parent's `on_pass` transition, as one
      pure result
- [ ] 1.13 `pipeline.rs`: idea proposal + dedupe scoring (D7) —
      candidate struct requiring `spawned_by` (refuse without it),
      normalized-title matching, and a `DedupeVerdict`
      (`Land | NearDuplicate{ticket_id, score}`) over a caller-
      supplied candidate list, so the same function serves the
      semantic path and the FTS fallback. Dedupe scope = project +
      linked projects
- [ ] 1.14 `pipeline.rs`: artifact manifest model (D9) —
      `artifacts/<ticket-id>/manifest.md` with `durable: false`,
      `ticket`, `created`, `expires`, artifact list; `expires`
      defaulting to created + 30 days; expiry check is a pure
      predicate (prune is a UI action, never automatic)
- [ ] 1.15 `pipeline.rs`: digest composition (spec) — group the
      board + ledger, **in this order**: `awaiting_review` (oldest
      first), `newly_unblocked` (with return lanes), `blocked`
      (oldest first by `blocked_at`, showing the *root* blocker of
      each chain, not the nearest), `moved_today`, `new_ideas`,
      `stale_runs`; with per-ticket run counts. Renders to markdown
      for chat, MCP, and `journal_append`
- [x] 1.16 `workspace.rs`: `links: Vec<ProjectLink>` read from the
      manifest's existing `extra` map (`{from, to, relation, note?}`)
      with a helper `linked_projects(name) -> Vec<&str>`; write path
      preserves unknown manifest keys (D12)
- [x] 1.17 `project.rs`: optional `symbol` (and `color`) read from
      `ProjectConfig.extra` (per OPEN-2 — confirm before
      implementing); no schema change
- [x] 1.18 `features.rs`: register `kenPipeline`, `FlagScope::
      Workspace`, `default: false`, plain-language description
      stating it requires `workspace` and `kenTasks`
- [ ] 1.19 Default pipeline scaffold: `default.md` content constant
      reproducing the twelve lanes of design D1 (eleven flow lanes
      plus `blocked`), written on first enable only if the file does
      not exist
- [ ] 1.20 Tests — **partially done (1.1–1.10's share is green: 88
      passing in `pipeline::` + `tasks::`)**. Done: definition
      round-trip with unknown keys + hand-edited body; two-blocked-lanes
      and two-human-lanes rejected; lane resolution and `maps_to`
      projection incl. the pipeline-less ticket asserted byte-identical;
      unknown lane / pipeline / blocker / return-lane ⇒ tray with the
      file byte-compared unchanged; the full forward transition table,
      every bounce edge, and the exact bounce_cap boundary producing a
      block; the admission cross-product asserting blocked ⇒ Refused
      ahead of everything incl. an `auto` lane with the master switch
      on; cycle detection (self, direct, 3-hop, 6-hop, legal diamond
      allowed, shortcut allowed, already-cyclic graph terminates);
      `return_lane` capture on block and restore on unblock; unblock
      evaluation requiring both halves. **Still owed (belongs with
      1.11–1.15/1.19):** child-ticket composition, dedupe verdicts,
      run ledger scan + stale detection, digest grouping, artifact
      expiry, `links` round-trip.
      Original list follows:
      definition round-trip with unknown keys + hand-
      edited body; two-blocked-lanes and two-human-lanes rejected;
      lane resolution and `maps_to` projection (incl. a pipeline-less
      ticket taking the classic path unchanged); unknown lane /
      pipeline / blocker / return-lane ⇒ tray, file untouched;
      transition table incl. every bounce edge and the exact
      bounce_cap boundary producing a *block*, not a halt;
      **admission table asserting blocked ⇒ Refused ahead of every
      other condition, including an `auto` lane with the master
      switch on**; cycle detection (direct, 3-hop, 6-hop, self-edge,
      and a legal diamond that must be *allowed*); `return_lane`
      capture on block and restore on unblock; unblock evaluation
      requiring both dependencies terminal and reason cleared;
      child-ticket composition; dedupe verdicts incl.
      missing-citation refusal; run ledger scan + stale detection;
      digest grouping and ordering; artifact expiry predicate;
      `links` round-trip through an unknown-key manifest

## 2. src-tauri

- [ ] 2.1 Pipeline state: load definitions on workspace open (flag
      on), watch `.ken-workspace/pipelines/` with the existing
      content-hash self-write dedupe, and fold pipelines into the
      board state so `board-state` carries lanes, per-lane counts,
      block summaries, and the extended tray; flag-gated by
      `kenPipeline` (requires `workspace` + `kenTasks`)
- [ ] 2.2 Ledger state: scan `runs/` on open, watch it, derive the
      queue view, mark stale `running` records at startup; emit
      `pipeline-runs` events alongside `board-state`
- [ ] 2.3 Commands — read: `pipeline_list_defs`, `pipeline_board`
      (lane-ordered board state), `pipeline_runs`, `pipeline_digest`,
      `pipeline_blockers(ticket_id)` (the resolved chain, root first)
- [ ] 2.4 Commands — write: `pipeline_kickoff(ticket_id)` →
      `admit()` → confirmation payload or a `queued`/`running` run
      record; `pipeline_advance(ticket_id, outcome, report)` →
      `advance()` → patch (`status`, `bounces`, and the block fields
      when the cap trips) + `## Log` append + run record close;
      `pipeline_cancel_run(run_id)`. **Manual kickoff only in this
      pass (D6)** — no transition fires without a user action
- [ ] 2.5 Commands — block/unblock: `pipeline_block(ticket_id,
      {blocked_by?, reason?})` and `pipeline_unblock(ticket_id,
      {clear_deps?, clear_reason?})`, both routed through 1.6/1.7 so
      cycle detection and `return_lane` capture cannot be bypassed;
      the cycle refusal surfaces the path in the error shown to the
      user
- [ ] 2.6 Unblock trigger (D5, OPEN-10): when any ticket reaches a
      terminal lane, run `evaluate_unblocks` over the (small) set of
      tickets naming it and move each freed ticket to its
      `return_lane` with `EntryKind::Unblock` — which means it waits
      at a confirmation, never starts. Run the same evaluation once
      on workspace open so nothing is missed across restarts. **Add
      an assertion/test that this path cannot reach `Start`.**
- [ ] 2.7 Sign-off flow: `pipeline_signoff(ticket_id, decision,
      comment?)` implementing accept / accept-with-comments (creates
      the child ticket *and* advances the parent) / reject (bounce)
- [ ] 2.8 Idea flow: documentation-lane proposals go through 1.13;
      wire the semantic path to the existing search surface when
      `semanticIndex`/`federatedKg` are on and the FTS path when they
      are not; near-duplicate ⇒ log append on the matched ticket, no
      new file
- [ ] 2.9 Artifacts: create `artifacts/<ticket-id>/` lazily with its
      manifest; expose `pipeline_artifacts(ticket_id)` and a
      `pipeline_prune_artifacts(ticket_id)` action; **verify no write
      path can place an artifact inside a member repo**
- [ ] 2.10 ken-memory integration: `pipeline_digest` writes through
      `journal_append` when `kenMemory` is on; skipped cleanly when
      off
- [ ] 2.11 Indexing: `.ken-workspace/pipelines/` and `runs/` ride the
      pseudo-member at **search-only** tier (same rule as `tasks/`);
      `artifacts/` is excluded from indexing (binaries + throwaway),
      except its `manifest.md` and any `walkthrough.md` at
      search-only
- [ ] 2.12 **Gated on 5.4 passing**: auto-transitions — on lane entry
      with `kickoff: auto` *and* pipeline `auto: true`, run `admit()`
      and start or queue. Ship `auto: false` in the scaffold; do not
      enable by default. This path calls the same `admit()` as
      everything else, so the blocked refusal and the unblock
      downgrade apply for free — do not add a second code path

## 3. ken-mcp

- [ ] 3.1 `pipeline_list(filter, limit?, cursor?)` — filter by lane,
      pipeline, project, model, assignee, and block state
      (`blocked`, `blocked_by: <id>`, `newly_unblocked`); compact
      rows only (id, title, lane, symbol, model, assignee, block
      summary, updated), **never bodies**; default limit 20, hard max
      100, opaque cursor. The tool description states plainly that
      the backlog is large, that this tool is the only way to read
      it, and that `blocked` answers "what is stuck and why"
- [ ] 3.2 `pipeline_get(id)` — the one tool returning a full ticket
      (body, scope, verify, bounces, blockers, return lane, log tail)
- [ ] 3.3 `pipeline_claim(id, agent, model?)` — re-runs `admit()`
      server-side; **refuses blocked tickets** and over-cap claims;
      creates/updates the run record to `running`
- [ ] 3.4 `pipeline_advance(id, outcome, report)` — `pass|fail`;
      resolves the target lane from the definition, applies bounce
      accounting (a cap breach blocks the ticket), appends the
      report, closes the run record
- [ ] 3.5 `pipeline_block(id, {blocked_by?, reason?, clear?})` — set
      or clear a block; enforces cycle detection and records
      `return_lane`; the tool description states that `blocked_by`
      takes ticket ids, never paths, and that blocking removes the
      ticket from every claimable queue
- [ ] 3.6 `pipeline_runs(filter)` — the watch surface: running,
      queued, blocked, waiting-on-human, stale
- [ ] 3.7 `pipeline_digest(day?)` — the daily update:
      awaiting-review, then newly-unblocked, then blocked oldest-first
- [ ] 3.8 All seven absent when `kenPipeline` is off (mirror the
      existing `task_*` flag-off test)
- [ ] 3.9 Tests: schema round-trips; `pipeline_list` never returns a
      body and never exceeds the hard max; cursor paging covers a
      1000-ticket lane exactly once; **claim refused when blocked**,
      including when the lane is `auto`; claim refused when over cap;
      `pipeline_block` refuses a cycle and leaves both files
      unchanged; advance applies exactly the intended frontmatter keys

## 4. Frontend

- [ ] 4.1 `api.ts`: pipeline/lane/run/block/digest types, command
      wrappers, `pipeline-runs` listener alongside `board-state`
- [ ] 4.2 Pipeline board view (workspace mode, flag on): horizontally
      scrolling lanes in definition order; card shows **project
      symbol top-left**, lane colour, model badge, agent badge,
      bounce badge when > 0, `+n` for multi-project tickets
- [ ] 4.3 Blocked rendering (D5): a real **Blocked column**, plus a
      card badge reading "blocked · returns to `<return_lane>`" and
      a blocker chip per `blocked_by` entry (click ⇒ open the
      blocking ticket, cross-project included) and the block reason.
      Ghost placeholders in the return lane are OPEN-9 — **ship
      column + badge first, add the ghost only if the board stops
      reading correctly without it**
- [ ] 4.4 Block/unblock dialog: set dependencies (ticket picker
      searching across projects, returns ids) and/or a free-text
      reason, both optional but at least one required; cycle refusal
      renders the offending path readably; unblock offers clearing
      dependencies and reason independently
- [ ] 4.5 Filters: per-project chips, "include linked projects"
      toggle, lane, model, assignee, and a block filter
      (blocked / not blocked / blocked-by-ticket / newly unblocked);
      plus a classic-board toggle to hide pipeline tickets (OPEN-7)
- [ ] 4.6 Kickoff confirmation dialog showing lane, agent, model,
      **scope globs and verify command** — the intent diff, not a
      generic "are you sure"; disabled with an explanation when scope
      or verify is missing, or when the ticket is blocked
- [ ] 4.7 Run tray: running / queued / waiting-on-human / stale, with
      cancel; cap indicator when runs are queued behind the cap
- [ ] 4.8 Sign-off dialog: Accept / Accept with comments (comment box
      → child ticket, shown in the confirmation) / Reject
- [ ] 4.9 Needs-attention tray additions: unknown lane, unknown
      pipeline, unknown blocker, missing return lane, blocked-and-
      ageing, missing boundary, expired artifacts (with prune), stale
      runs
- [ ] 4.10 Artifact viewer for `artifacts/<ticket-id>/` with a visible
      `throwaway — not part of the test suite` marker and the expiry
      date
- [ ] 4.11 Digest surface: a daily panel rendering
      `pipeline_digest`, awaiting-review first, then a prominent
      "unblocked overnight" group with a per-ticket start action
      (which still opens the confirmation dialog, never starts
      directly)

## 5. Verification

- [ ] 5.1 cargo test --workspace, pnpm test, pnpm check, pnpm build
      green
- [ ] 5.2 **Flag off is byte-identical**: with `kenPipeline` off,
      open a workspace with existing task files — no pipeline view,
      no pipeline tools on either surface, no `pipelines/`, `runs/`
      or `artifacts/` folders created, no extra watchers; the
      ken-tasks board behaves exactly as before. Diff the task-home
      tree before/after a session: zero changes. Existing ken-tasks
      and ken-mcp test suites pass untouched
- [ ] 5.3 Migration check: a board of pre-existing tickets with no
      `pipeline` key is opened with the flag **on** — every ticket
      still parses, still lands in its classic column, and **no task
      file is rewritten** (byte-compare the tree)
- [ ] 5.4 **Manual end-to-end scenario (gates 2.12)**: create one
      real ticket with `scope` and `verify`; hand-click it through
      every lane — Ideas → Backlog → To Do → Investigation →
      Refinement → Programmer (block it here on a second ticket,
      confirm `return_lane: programmer` is recorded and the card
      leaves the claimable queue; complete the blocker and confirm
      the ticket returns to Programmer **waiting at a confirmation,
      not running**) → Programmer → Tester (fail once, confirm the
      bounce back to Programmer and the counter increment) → Tester
      (pass) → Architect review (fail once, confirm the bounce to
      Refinement) → Architect (pass) → QA (produce one throwaway
      artifact and confirm it lands only under
      `.ken-workspace/artifacts/`) → Sign-off (accept **with**
      comments; confirm the child ticket appears in To Do with
      `parent` set and the parent still advanced) → Documentation
      (confirm docs updated and one idea filed into Ideas with a
      `spawned_by` citation). Record what broke; only then implement
      2.12
- [ ] 5.5 Bounce cap: drive a ticket past `bounce_cap` and confirm it
      is **blocked** with a retry-cap reason and the correct
      `return_lane`, refuses `pipeline_claim`, and appears in the
      *same* digest group as dependency-blocked tickets; unblock it
      and confirm `bounces` resets
- [ ] 5.6 **Blocked invariant sweep**: for each of UI kickoff, MCP
      `pipeline_claim`, and (once 2.12 exists) auto-transition,
      attempt to start a blocked ticket and confirm refusal with no
      run record created — including a blocked ticket sitting in an
      `auto` lane with the pipeline master switch on
- [ ] 5.7 Cycle detection: attempt A→B→A and a 4-ticket chain closing
      on itself; confirm both are refused at write time with the path
      reported and both files unchanged; confirm a legal diamond
      (A blocked by B and C, both blocked by D) is accepted
- [ ] 5.8 Concurrency cap: with `concurrency_cap: 1`, kick off two
      runs and confirm the second is `queued`, the tray says why, and
      it starts only when the first closes
- [ ] 5.9 Backlog paging: seed ~1000 tickets in one lane; confirm
      `pipeline_list` with no arguments returns one default page of
      compact rows with a cursor, returns no bodies, and that paging
      through covers every ticket exactly once. Repeat with
      `{blocked: true}` over ~100 blocked tickets
- [ ] 5.10 Round-trip abuse: hand-add unknown frontmatter to a
      pipeline definition and a ticket, then advance the ticket
      twice; confirm each file diff is only the intended keys
      (`status`, `updated`, `bounces` on a bounce, the block fields
      on a block). Rename a lane in the definition and confirm
      orphaned tickets — including ones whose `return_lane` pointed
      at it — appear in the tray with their files unchanged
- [ ] 5.11 Restart safety: kill Ken mid-run; on restart confirm the
      run is reported stale, the ticket has not advanced, the queue
      is rebuilt from the folder alone, and any blocker that
      completed while Ken was down is picked up by the open-time
      unblock evaluation (still landing at a confirmation)
- [ ] 5.12 Dedupe: with `semanticIndex` on, generate an idea that
      duplicates an existing ticket and confirm no new file plus a
      log note on the match; repeat with the index off and confirm
      the FTS fallback still refuses an uncited proposal
