<script lang="ts">
  // Your day (Y1): the team digest, the team's tickets assigned to me, my
  // other tasks, then recent files across the team and its index health.
  // Clicking a task opens its panel (Y1b) beside the list.
  import { onMount } from "svelte";
  import { app } from "../lib/app.svelte";
  import { scope } from "../lib/scope.svelte";
  import { day, memberNames, openInRepo, openMemberPath, repoName } from "../lib/day.svelte";
  import { isOverdue, shortTarget, taskDetail, ticketDetail } from "../lib/day";
  import { digestMarkdown } from "../lib/assist";
  import { renderMarkdown } from "../lib/markdown";
  import { parseCitation } from "../lib/citation";
  import { loadRecents, mergeTeamRecents, fallbackRecents } from "../lib/recent";
  import { kindForPath, timeAgo } from "../lib/format";
  import { openContextMenu } from "../lib/ui/ContextMenu.svelte";
  import type { DayTask } from "../lib/api";
  import TaskPanel from "../day/TaskPanel.svelte";
  import FileGlyph from "../files/FileGlyph.svelte";
  import Check from "@lucide/svelte/icons/check";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import X from "@lucide/svelte/icons/x";
  import Reply from "@lucide/svelte/icons/reply";

  onMount(() => {
    void scope.init();
    void day.init();
  });

  const teamName = $derived(scope.team ?? app.workspace?.name ?? app.project?.name ?? "");

  const todayLong = $derived(
    new Date(`${day.today}T12:00:00`).toLocaleDateString(undefined, {
      weekday: "long",
      month: "long",
      day: "numeric",
    }),
  );

  // ── Digest ──────────────────────────────────────────────────────────

  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | undefined;

  async function share() {
    if (!day.digest) return;
    await navigator.clipboard.writeText(
      digestMarkdown({ date: todayLong, body: day.digest.body, sources: day.digest.sources }),
    );
    copied = true;
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => (copied = false), 1500);
  }

  function digestTime(epoch: number): string {
    return new Date(epoch * 1000).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  }

  function chipLabel(path: string): string {
    return path.split("/").pop() || path;
  }

  // A cited file opens in Files at its line or heading. A web link is left
  // alone: the opener plugin opens it in the browser.
  function onDigestClick(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    const href = a.getAttribute("href") ?? "";
    if (/^(https?|mailto):/i.test(href)) return;
    e.preventDefault();
    const c = parseCitation(href);
    if (!c) return;
    const where = { line: c.line, anchor: c.anchor };
    if (c.projectId) void openInRepo(c.projectId, c.path, where);
    else void openMemberPath(c.path, where);
  }

  const indexing = $derived((day.health?.queued ?? 0) > 0);

  // ── Tasks ───────────────────────────────────────────────────────────

  let replyFor = $state<string | null>(null);
  let replyNote = $state("");
  let actionError = $state<string | null>(null);

  async function run(p: Promise<unknown>) {
    actionError = null;
    try {
      await p;
    } catch (e) {
      actionError = String(e);
    }
  }

  function acceptMenu(e: MouseEvent, task: DayTask) {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openContextMenu(r.left, r.bottom + 4, [
      { label: "Accept", icon: Check, onSelect: () => void run(day.accept(task)) },
      { label: "Not mine", icon: X, onSelect: () => void run(day.notMine(task)) },
      {
        label: "Reply",
        icon: Reply,
        onSelect: () => {
          replyFor = task.id;
          replyNote = "";
        },
      },
    ]);
  }

  async function sendReply(task: DayTask) {
    const note = replyNote.trim();
    if (!note) return;
    await run(day.reply(task, note));
    replyFor = null;
    replyNote = "";
  }

  const repos = $derived(memberNames());

  const selectedId = $derived(day.panel?.mode === "edit" ? day.panel.id : null);

  // ── Recent files across the team ────────────────────────────────────

  const recents = $derived.by(() => {
    const ids = scope.teamProjectIds;
    const focused = app.focused;
    const lists = ids.map((id) => ({
      projectId: id,
      // The focused repo's history is live; the others' are what was saved.
      entries: id === focused ? app.recents : loadRecents(id),
      // Only the focused repo's index is loaded here, to drop files it lost.
      known: id === focused ? new Set(app.files.map((f) => f.relPath)) : null,
    }));
    const merged = mergeTeamRecents(lists);
    if (merged.length > 0) return merged;
    return focused
      ? fallbackRecents(app.files).map((f) => ({ projectId: focused, path: f.relPath, at: f.at }))
      : [];
  });

  const panelOpen = $derived(day.panel !== null && (day.panel.mode === "new" || day.panelTask !== null));
</script>

<div class="wrap">
  <div class="inner" class:with-panel={panelOpen}>
    <div class="main">
      <header class="hd">
        <h1>Your day</h1>
        {#if teamName}<span class="chip team">{teamName}</span>{/if}
        <span class="sp"></span>
        <button class="btn btn-small" onclick={() => day.openNew()}>+ Task</button>
        <button class="btn btn-small btn-ghost" onclick={share} disabled={!day.digest} title="Copy the digest as markdown">
          {#if copied}<Check size={12} strokeWidth={2} /> Copied{:else}Share{/if}
        </button>
      </header>

      <!-- The digest, written for the team -->
      <section class="digest">
        <div class="dh">
          <span class="date">{todayLong}</span>
          {#if day.digest}<span class="chip">{digestTime(day.digest.generatedAt)}</span>{/if}
        </div>
        {#if day.digest}
          <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
          <div class="digest-body" onclick={onDigestClick}>
            {@html renderMarkdown(day.digest.body)}
          </div>
          {#if day.digest.sources.length > 0}
            <div class="sources">
              {#each day.digest.sources as source (source)}
                <button class="chip mono src" title={source} onclick={() => void openMemberPath(source)}>
                  {chipLabel(source)}
                </button>
              {/each}
            </div>
          {/if}
        {:else if day.generating}
          <p class="quiet">Writing the digest.</p>
        {:else if !day.claudeFound}
          <p class="quiet">The digest needs Claude Code.</p>
        {:else}
          <p class="quiet">No digest yet. Ken writes one each morning.</p>
          <div class="digest-actions">
            <button
              class="btn btn-small"
              class:dim={indexing}
              title={indexing ? "The index is still running; the digest may miss what it has not read." : undefined}
              onclick={() => void day.writeDigest()}>Write it now</button
            >
          </div>
        {/if}
        {#if day.digestError}
          <p class="error">The last try did not finish: {day.digestError}</p>
        {/if}
      </section>

      {#if day.loadError}
        <p class="error">{day.loadError}</p>
      {/if}

      <!-- Tickets: open ticket files assigned to me -->
      {#if day.state?.hasTickets}
        <section>
          <div class="list-head">
            <span class="overline">Tickets · {day.tickets.length}</span>
            <span class="sp"></span>
            <span class="col-label st-col">state</span>
            <span class="col-label due-col">target</span>
          </div>
          {#if day.tickets.length > 0}
            <div class="group">
              {#each day.tickets as t (t.projectId + ":" + t.relPath)}
                <button class="row ticket" title={`${t.repo}/${t.relPath}`} onclick={() => void openInRepo(t.projectId, t.relPath)}>
                  <span class="tid mono">{t.id}</span>
                  <span class="tx">
                    <span class="tt">{t.title}</span>
                    <span class="td">{ticketDetail(t)}</span>
                  </span>
                  <span class="st-col"><span class="chip state">{t.state}</span></span>
                  <span class="due-col due" class:late={t.target !== null && t.target < day.today}>
                    {shortTarget(t.target, day.today)}
                  </span>
                </button>
              {/each}
            </div>
          {:else}
            <p class="quiet small">No open tickets assigned to you.</p>
          {/if}
        </section>
      {/if}

      <!-- Other: my tasks -->
      <section>
        <div class="list-head">
          <span class="overline">Other · {day.tasks.length}</span>
          <span class="sp"></span>
          <span class="col-label due-col">target</span>
          <span class="act-col"></span>
        </div>
        {#if actionError}<p class="error">{actionError}</p>{/if}
        {#if day.tasks.length > 0}
          <div class="group">
            {#each day.tasks as task (task.id)}
              <div class="row task" class:done={task.state === "done"} class:selected={selectedId === task.id}>
                {#if task.inbox}
                  <span class="cb ghost" title="Accept it first"></span>
                {:else}
                  <button
                    class="cb"
                    class:on={task.state === "done"}
                    aria-label={task.state === "done" ? "Mark open" : "Mark done"}
                    onclick={() => void run(day.toggle(task))}
                  >
                    {#if task.state === "done"}<Check size={11} strokeWidth={2.5} />{/if}
                  </button>
                {/if}
                {#snippet body()}
                  <span class="tx">
                    <span class="tt">{task.title}</span>
                    <span class="td">{taskDetail(task, repos)}</span>
                  </span>
                  <span class="due-col due" class:late={isOverdue(task, day.today)}>{shortTarget(task.target, day.today)}</span>
                {/snippet}
                {#if task.inbox}
                  <!-- A teammate's task is not a file of mine until accepted. -->
                  <span class="open">{@render body()}</span>
                {:else}
                  <button class="open" onclick={() => day.openTask(task.id)}>
                    {@render body()}
                  </button>
                {/if}
                <span class="act-col">
                  {#if task.inbox}
                    <button
                      class="btn btn-small"
                      disabled={day.busy === task.id}
                      onclick={(e) => acceptMenu(e, task)}
                    >
                      Accept <ChevronDown size={12} strokeWidth={1.75} />
                    </button>
                  {/if}
                </span>
              </div>
              {#if replyFor === task.id}
                <div class="reply">
                  <textarea bind:value={replyNote} rows="2" placeholder="Reply to {task.from ?? 'the sender'}" aria-label="Reply"></textarea>
                  <div class="reply-actions">
                    <button class="btn btn-small btn-primary" disabled={!replyNote.trim() || day.busy === task.id} onclick={() => void sendReply(task)}>Send</button>
                    <button class="btn btn-small btn-ghost" onclick={() => (replyFor = null)}>Cancel</button>
                  </div>
                </div>
              {/if}
            {/each}
          </div>
        {:else}
          <p class="quiet small">Nothing for today. + Task adds one.</p>
        {/if}
      </section>

      <!-- Recent files across the team | index health -->
      <div class="halves">
        <section>
          <div class="list-head"><span class="overline">Recent files</span></div>
          {#if recents.length > 0}
            <div class="group">
              {#each recents as r (r.projectId + ":" + r.path)}
                <button class="row file" title={r.path} onclick={() => void openInRepo(r.projectId, r.path)}>
                  <FileGlyph kind={kindForPath(r.path)} size="sm" />
                  <span class="fname">{chipLabel(r.path)}</span>
                  <span class="chip repo">{repoName(r.projectId)}</span>
                  <span class="ftime">{timeAgo(r.at)}</span>
                </button>
              {/each}
            </div>
          {:else}
            <p class="quiet small">No files opened yet.</p>
          {/if}
        </section>
        <section>
          <div class="list-head">
            <span class="overline">Index health</span>
            <span class="sp"></span>
            <button class="link" onclick={() => (app.screen = "team")}>Team</button>
          </div>
          {#if day.health}
            <div class="stats">
              <div><b>{day.health.indexed} / {day.health.total}</b><span>repos indexed</span></div>
              <div><b>{day.health.queued}</b><span>files queued</span></div>
              <div><b class:bad={day.health.failed > 0}>{day.health.failed}</b><span>failed</span></div>
            </div>
          {:else}
            <p class="quiet small">Not available.</p>
          {/if}
        </section>
      </div>
    </div>

    {#if day.panel && (day.panel.mode === "new" || day.panelTask)}
      <div class="side">
        {#key day.panel.mode === "edit" ? day.panel.id : "new"}
          {#if day.panel.mode === "edit"}
            {#if day.panelTask}
              <TaskPanel task={day.panelTask} />
            {/if}
          {:else}
            <TaskPanel task={null} newLinks={day.panel.links} />
          {/if}
        {/key}
      </div>
    {/if}
  </div>
</div>

<style>
  .wrap {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: 36px 44px 56px;
  }
  .inner {
    max-width: 860px;
    margin: 0 auto;
  }
  .inner.with-panel {
    max-width: 1280px;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 400px;
    gap: 20px;
    align-items: start;
  }
  .side {
    position: sticky;
    top: 0;
  }

  /* ── Header ───────────────────────────────────────────── */
  .hd {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--border);
  }
  h1 {
    margin: 0;
    font-family: var(--font-serif);
    font-size: 26px;
    font-weight: 500;
    letter-spacing: -0.01em;
  }
  .sp {
    flex: 1;
  }
  .hd .btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    font-size: 11px;
    color: var(--ink-secondary);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 1px 8px;
    background: var(--sunken);
    white-space: nowrap;
  }
  .chip.team {
    font-size: 11.5px;
  }

  /* ── Digest ───────────────────────────────────────────── */
  .digest {
    margin-top: 20px;
  }
  .dh {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
  }
  .date {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--ink-secondary);
  }
  .digest-body {
    font-family: var(--font-serif);
    font-size: 16px;
    line-height: 1.75;
    color: var(--ink);
    max-width: 70ch;
  }
  .digest-body :global(p) {
    margin: 0 0 10px;
  }
  .digest-body :global(p:last-child) {
    margin-bottom: 0;
  }
  .digest-body :global(a) {
    color: var(--accent-deep);
    text-decoration-color: color-mix(in srgb, var(--accent) 40%, transparent);
    text-underline-offset: 2px;
  }
  .sources {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    margin-top: 12px;
  }
  .src {
    cursor: pointer;
  }
  .src:hover {
    border-color: var(--accent);
  }
  .quiet {
    margin: 0;
    font-size: 14px;
    line-height: 1.6;
    color: var(--ink-secondary);
  }
  .quiet.small {
    font-size: 13px;
    color: var(--ink-tertiary);
  }
  .digest-actions {
    margin-top: 10px;
  }
  .btn.dim {
    opacity: 0.55;
  }
  .error {
    margin: 8px 0 0;
    font-size: 12px;
    color: var(--danger);
  }

  /* ── Lists ────────────────────────────────────────────── */
  section {
    margin-top: 28px;
  }
  .list-head {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 8px;
    padding: 0 16px 0 0;
  }
  .col-label {
    font-size: 10.5px;
    color: var(--ink-tertiary);
    text-transform: lowercase;
  }
  .st-col {
    width: 110px;
    flex: none;
    text-align: right;
  }
  .due-col {
    width: 64px;
    flex: none;
    text-align: right;
  }
  .act-col {
    width: 96px;
    flex: none;
    display: flex;
    justify-content: flex-end;
  }
  .group {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    box-shadow: var(--shadow-card);
    overflow: hidden;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 52px;
    padding: 8px 16px;
    border: none;
    background: transparent;
    font: inherit;
    text-align: left;
    color: var(--ink);
  }
  .row + .row,
  .reply + .row,
  .row + .reply {
    border-top: 1px solid var(--border);
  }
  button.row {
    cursor: pointer;
  }
  button.row:hover,
  .row.task:has(button.open:hover),
  .row:focus-visible,
  .row.task:has(button.open:focus-visible) {
    background: color-mix(in srgb, var(--accent) 6%, transparent);
  }
  .open {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    align-self: stretch;
    border: none;
    background: transparent;
    padding: 0;
    font: inherit;
    text-align: left;
    color: inherit;
  }
  button.open {
    cursor: pointer;
  }
  .row.selected {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .tid {
    width: 72px;
    flex: none;
    font-size: 12px;
    color: var(--accent-deep);
    text-decoration: underline;
    text-decoration-color: color-mix(in srgb, var(--accent) 40%, transparent);
    text-underline-offset: 2px;
  }
  .tx {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .tt {
    font-size: 13.5px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .td {
    font-size: 11.5px;
    color: var(--ink-tertiary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip.state {
    max-width: 110px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .due {
    font-size: 12px;
    color: var(--ink-secondary);
    font-variant-numeric: tabular-nums;
  }
  .due.late {
    color: var(--danger);
  }
  .row.done .tt {
    font-weight: 400;
    text-decoration: line-through;
    color: var(--ink-tertiary);
  }
  .row.done .due {
    color: var(--ink-tertiary);
  }
  .cb {
    flex: none;
    width: 16px;
    height: 16px;
    border-radius: 4px;
    border: 1.5px solid var(--border-strong);
    background: var(--surface);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    color: var(--surface);
    cursor: pointer;
  }
  .cb:hover {
    border-color: var(--accent);
  }
  .cb.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .cb.ghost {
    border-style: dashed;
    cursor: default;
  }
  .act-col .btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 10px;
  }
  .reply {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 16px 12px 44px;
  }
  .reply textarea {
    font: inherit;
    font-size: 12.5px;
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid var(--border-strong);
    background: var(--surface);
    color: var(--ink);
    resize: vertical;
  }
  .reply-actions {
    display: flex;
    gap: 6px;
  }

  /* ── Two halves ───────────────────────────────────────── */
  .halves {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 20px;
  }
  .row.file {
    min-height: 40px;
    gap: 10px;
  }
  .fname {
    flex: 1;
    min-width: 0;
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip.repo {
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ftime {
    flex: none;
    font-size: 11.5px;
    color: var(--ink-tertiary);
  }
  .link {
    border: none;
    background: transparent;
    padding: 0;
    font: inherit;
    font-size: 12px;
    color: var(--accent);
    cursor: pointer;
  }
  .link:hover {
    color: var(--accent-hover);
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    box-shadow: var(--shadow-card);
  }
  .stats > div {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 16px;
  }
  .stats > div + div {
    border-left: 1px solid var(--border);
  }
  .stats b {
    font-size: 18px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .stats b.bad {
    color: var(--danger);
  }
  .stats span {
    font-size: 11.5px;
    color: var(--ink-tertiary);
  }
</style>
