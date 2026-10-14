<script lang="ts">
  // The Timeline (knowledge-core decision 37): the project's raid log, one
  // chronological list of every ingest, ruling, ticket change, commit,
  // finding and release, newest first, each linked to its source. Read from
  // the workspace's knowledge base (its log, decisions log, tickets, git and
  // findings), no model.
  import { untrack } from "svelte";
  import { api, type RaidEntry, type RaidLog } from "../lib/api";
  import { app } from "../lib/app.svelte";
  import { filterLog, groupByDate, highlightMatches, kindLabel, kindsIn } from "../lib/timeline";
  import Search from "@lucide/svelte/icons/search";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";

  let log = $state<RaidLog | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(false);
  let query = $state("");
  let kind = $state<string | null>(null);

  async function load() {
    loading = true;
    try {
      log = await api.raidLog();
      error = null;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  // Read again each time the screen is shown, and when the focused repo
  // changes while it is.
  $effect(() => {
    if (app.screen !== "timeline") return;
    void app.focused;
    untrack(() => void load());
  });

  const entries = $derived(log?.entries ?? []);
  const kinds = $derived(kindsIn(entries));
  const filtered = $derived(filterLog(entries, kind, query));
  const groups = $derived(groupByDate(filtered));

  const MONTHS = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

  function dateLabel(date: string): string {
    const [y, m, d] = date.split("-").map(Number);
    const label = `${MONTHS[(m ?? 1) - 1] ?? "—"} ${d ?? "?"}`;
    return y === new Date().getFullYear() ? label : `${label}, ${y}`;
  }

  function timeLabel(at: number | null): string {
    if (at === null) return "";
    return new Date(at * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }

  function fileName(path: string): string {
    return path.split("/").pop() ?? path;
  }

  /** Open an entry's source in Files, in the repo the log was read from. */
  async function open(e: RaidEntry) {
    const projectId = log?.projectId;
    if (!e.path || !projectId) return;
    if (projectId !== app.focused && app.workspace?.members.some((m) => m.projectId === projectId)) {
      await app.focusMember(projectId);
    }
    app.openAt(e.path, { line: e.line ?? undefined });
  }

  function pillStyle(k: string): string {
    switch (k) {
      case "ruling":
        return "background:color-mix(in srgb, var(--healthy) 12%, transparent);color:var(--healthy-text)";
      case "finding":
        return "background:color-mix(in srgb, var(--needs-input) 12%, transparent);color:var(--needs-input-text)";
      case "ticket":
      case "release":
        return "background:color-mix(in srgb, var(--accent) 10%, transparent);color:var(--accent-deep)";
      default:
        return "background:var(--sunken);color:var(--ink-secondary)";
    }
  }
</script>

<div class="screen">
  <div class="inner">
    <div class="head">
      <h1>Timeline</h1>
      {#if log?.name}<span class="from">{log.name}</span>{/if}
      <button class="refresh" title="Read again" aria-label="Read again" disabled={loading} onclick={() => void load()}>
        <RefreshCw size={14} strokeWidth={1.75} aria-hidden="true" />
      </button>
    </div>

    {#if error}
      <div class="error">The timeline could not be read: {error}</div>
    {/if}

    {#if log && entries.length === 0}
      <div class="empty-card">
        <h2>Nothing on the timeline yet</h2>
        <p>
          Ingests, rulings, ticket changes, commits, findings and releases show here as they happen, newest
          first, each linked to where it came from.
        </p>
      </div>
    {:else if log}
      <div class="search">
        <Search class="lens" size={14} strokeWidth={1.75} aria-hidden="true" />
        <input bind:value={query} placeholder="Search the timeline…" spellcheck="false" />
        <span class="count">{filtered.length} of {entries.length}</span>
      </div>

      <div class="chips">
        <button class="cat-chip" class:on={kind === null} onclick={() => (kind = null)}>All</button>
        {#each kinds as k (k)}
          <button class="cat-chip" class:on={kind === k} onclick={() => (kind = kind === k ? null : k)}>
            {kindLabel(k)}
          </button>
        {/each}
      </div>

      {#if filtered.length === 0}
        <div class="no-match">Nothing matches.</div>
      {:else}
        <div class="timeline">
          <div class="line"></div>
          {#each groups as g (g.date)}
            <div class="day">
              <span class="dot"></span>
              <span class="date">{dateLabel(g.date)}</span>
            </div>
            {#each g.entries as e, i (`${g.date}-${i}`)}
              <div class="event">
                <div class="meta">
                  <span class="pill" style={pillStyle(e.kind)}>{e.kind}</span>
                  {#if e.at !== null}<span class="time">{timeLabel(e.at)}</span>{/if}
                  {#if e.repo}<span class="repo">{e.repo}</span>{/if}
                  {#if e.reference}<span class="ref mono">{e.reference}</span>{/if}
                </div>
                <div class="body">
                  {@html highlightMatches(e.text, query)}
                  {#if e.path}
                    <button class="src mono" title={e.path} onclick={() => void open(e)}>{fileName(e.path)}</button>
                  {/if}
                </div>
              </div>
            {/each}
          {/each}
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .screen {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: 24px clamp(24px, 4vw, 56px) 48px;
  }
  .inner {
    max-width: 760px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }
  h1 {
    margin: 0;
    font-family: var(--font-serif);
    font-size: 30px;
    font-weight: 500;
  }
  .from {
    font-size: 12.5px;
    color: var(--ink-tertiary);
  }
  .refresh {
    margin-left: auto;
    align-self: center;
    display: inline-flex;
    padding: 5px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--ink-secondary);
  }
  .refresh:hover:not(:disabled) {
    color: var(--ink);
    border-color: var(--border-strong);
  }
  .error {
    font-size: 12.5px;
    color: var(--danger);
  }

  .search {
    height: 38px;
    border: 1px solid var(--border-strong);
    border-radius: 9px;
    background: var(--surface);
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 0 14px;
    box-shadow: var(--shadow-control);
  }
  .search :global(.lens) {
    color: var(--ink-tertiary);
    flex: none;
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: transparent;
    font-family: inherit;
    font-size: 13px;
    color: var(--ink);
  }
  .count {
    font-size: 11.5px;
    color: var(--ink-tertiary);
    white-space: nowrap;
  }

  .chips {
    display: flex;
    gap: 7px;
    flex-wrap: wrap;
  }
  .cat-chip {
    font-size: 11.5px;
    font-weight: 600;
    padding: 4px 12px;
    border-radius: 12px;
    border: 1px solid var(--border-strong);
    background: var(--surface);
    color: var(--ink-secondary);
  }
  .cat-chip:hover {
    background: var(--paper);
  }
  .cat-chip.on {
    background: var(--ink);
    border-color: var(--ink);
    color: var(--paper);
  }

  .no-match {
    font-size: 13px;
    color: var(--ink-tertiary);
    padding: 12px 0;
  }

  .timeline {
    position: relative;
    padding-left: 26px;
    display: flex;
    flex-direction: column;
  }
  .line {
    position: absolute;
    left: 7px;
    top: 8px;
    bottom: 8px;
    width: 2px;
    background: var(--border);
    border-radius: 1px;
  }
  .day {
    position: relative;
    padding: 4px 0 10px;
  }
  .dot {
    position: absolute;
    left: -25px;
    top: 7px;
    width: 12px;
    height: 12px;
    border-radius: 6px;
    background: var(--surface);
    border: 2.5px solid var(--paper);
    box-shadow: 0 0 0 1.5px var(--border-strong);
  }
  .date {
    font-size: 11.5px;
    font-weight: 700;
    color: var(--ink-tertiary);
    letter-spacing: 0.05em;
  }
  .event {
    padding: 0 0 14px;
  }
  .meta {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: 11px;
    color: var(--ink-tertiary);
  }
  .pill {
    font-size: 10px;
    font-weight: 600;
    padding: 1px 7px;
    border-radius: 8px;
  }
  .repo {
    color: var(--ink-secondary);
  }
  .body {
    font-size: 13.5px;
    line-height: 1.6;
    margin-top: 3px;
  }
  .body :global(mark) {
    background: var(--match-highlight);
    border-radius: 3px;
    padding: 0 3px;
    color: inherit;
  }
  .src {
    font-size: 11px;
    color: var(--ink-secondary);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 1px 6px;
    background: var(--surface);
    margin-left: 4px;
  }
  .src:hover {
    border-color: var(--border-strong);
    color: var(--ink);
  }

  .empty-card {
    max-width: 420px;
    margin: 48px auto 0;
    text-align: center;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    padding: 28px 32px;
    box-shadow: var(--shadow-card);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }
  .empty-card h2 {
    margin: 0;
    font-family: var(--font-serif);
    font-size: 19px;
    font-weight: 500;
  }
  .empty-card p {
    margin: 0;
    font-size: 13.5px;
    color: var(--ink-secondary);
  }
</style>
