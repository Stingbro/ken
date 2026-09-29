<script lang="ts">
  // Ingest, the inbox that empties (the Wright white box, frame 8b). Drop a
  // file or choose one: Ken puts it in the team library's
  // Research/Ingestion/Raw/ and reads it into a dated note in Ingested/.
  // Each source's card holds what a person reviews: what it overturns, where
  // it contradicts the wiki or itself, and the changes it proposes (pages to
  // update, new pages, rulings for their decider, tickets), each applied or
  // discarded here. "Seen" files the source beside its note; Undo takes it
  // all back.
  import { onMount } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, type IngestCard, type IngestOverview, type IngestProposal, type InboxItem } from "../lib/api";
  import { app } from "../lib/app.svelte";
  import { scope } from "../lib/scope.svelte";
  import { renderMarkdown } from "../lib/markdown";
  import { timeAgo } from "../lib/format";
  import ProposalDetail from "../review/ProposalDetail.svelte";

  let overview = $state<IngestOverview | null>(null);
  let error = $state<string | null>(null);
  let selected = $state<number | null>(null);
  let card = $state<IngestCard | null>(null);
  let busy = $state<string | null>(null);
  let dropping = $state(false);
  let adding = $state(0);
  let openDiff = $state<number | null>(null);

  const team = $derived(scope.team);

  async function refresh() {
    try {
      overview = await api.ingestOverview(team);
      error = null;
      // The newest note is shown until the person picks another.
      if (selected === null && overview.ingested.length > 0) selected = overview.ingested[0].id;
    } catch (e) {
      error = String(e);
    }
  }

  async function loadCard(id: number | null) {
    if (id === null) {
      card = null;
      return;
    }
    try {
      card = await api.ingestCard(team, id);
    } catch (e) {
      card = null;
      error = String(e);
    }
  }

  $effect(() => {
    void team;
    void refresh();
  });
  $effect(() => {
    void loadCard(selected);
  });

  onMount(() => {
    // While anything is being read, the list follows it.
    const t = setInterval(() => {
      if (overview && (overview.running || overview.raw.some((r) => r.state === "queued" || r.state === "reading"))) {
        void refresh().then(() => loadCard(selected));
      }
    }, 4000);
    return () => clearInterval(t);
  });

  async function chooseFiles() {
    const picked = await openDialog({ multiple: true, title: "Choose files to ingest" });
    const paths = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
    if (paths.length === 0) return;
    adding += paths.length;
    try {
      await api.ingestAdd(team, paths);
    } catch (e) {
      error = String(e);
    } finally {
      adding -= paths.length;
      await refresh();
    }
  }

  async function onDrop(e: DragEvent) {
    e.preventDefault();
    dropping = false;
    const files = [...(e.dataTransfer?.files ?? [])];
    if (files.length === 0) return;
    adding += files.length;
    for (const f of files) {
      try {
        await api.ingestAddBytes(team, f);
      } catch (err) {
        error = String(err);
      } finally {
        adding -= 1;
      }
    }
    await refresh();
  }

  async function inLibrary(path: string) {
    if (!overview) return;
    if (app.focused !== overview.projectId) await app.focusMember(overview.projectId);
    app.openInFiles(path);
  }

  // A proposal as the Review card type, so the diff reads it the same way.
  const asItem = (p: IngestProposal): InboxItem => ({
    id: `stored-${p.id}`,
    kind: "page-proposal",
    title: p.title,
    body: p.body,
    when: 0,
    sourceRef: p.page,
    payload: p.payload,
  });

  async function apply(p: IngestProposal) {
    if (!overview) return;
    busy = `p${p.id}`;
    try {
      await api.applyPageProposal(p.id, overview.projectId);
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
      await loadCard(selected);
      await refresh();
    }
  }

  async function discard(p: IngestProposal) {
    if (!overview) return;
    busy = `p${p.id}`;
    try {
      await api.resolveReviewItem(p.id, overview.projectId);
    } finally {
      busy = null;
      await loadCard(selected);
      await refresh();
    }
  }

  async function seen() {
    if (!card) return;
    busy = "seen";
    try {
      await api.ingestFile(card.id, team);
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
      await refresh();
      await loadCard(selected);
    }
  }

  async function undo() {
    if (!card) return;
    busy = "undo";
    try {
      await api.ingestUndo(card.id, team);
      selected = null;
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
      await refresh();
    }
  }

  const groups = $derived.by(() => {
    const ps = card?.proposals ?? [];
    return [
      { label: "Pages it would update", rows: ps.filter((p) => p.kind === "page"), apply: "Apply" },
      { label: "New pages it calls for", rows: ps.filter((p) => p.kind === "new page"), apply: "Create" },
      { label: "Rulings, for their decider", rows: ps.filter((p) => p.kind === "ruling"), apply: "Accept" },
      { label: "Tickets", rows: ps.filter((p) => p.kind === "ticket"), apply: "Create" },
    ].filter((g) => g.rows.length > 0);
  });

  const stateLabel: Record<string, string> = {
    queued: "in the queue",
    reading: "reading",
    "in review": "read · on its card",
    failed: "could not read",
  };
</script>

<div class="ingest">
  <div
    class="left"
    class:dropping
    role="region"
    aria-label="Drop files to ingest"
    ondragover={(e) => {
      e.preventDefault();
      dropping = true;
    }}
    ondragleave={() => (dropping = false)}
    ondrop={onDrop}
  >
    <div class="head">
      <h2>Ingest</h2>
      {#if overview}<span class="chip" title="Its Research/Ingestion/ folders">{overview.library}</span>{/if}
    </div>

    <button class="drop" onclick={chooseFiles}>
      <strong>Drop files here</strong> or choose them
      <span class="sub">A transcript, a recording, a document or notes. Ken files each in Raw/ and reads it.</span>
    </button>
    {#if adding > 0}<p class="note">Adding {adding} {adding === 1 ? "file" : "files"}…</p>{/if}
    {#if error}<p class="warn">{error}</p>{/if}
    {#if overview && !overview.claudeFound}
      <p class="warn">Reading needs the Claude Code CLI. Install it and run <span class="mono">claude</span> once to log in.</p>
    {/if}

    {#if overview}
      <div class="divider">raw · {overview.raw.length}</div>
      {#each overview.raw as r (r.path)}
        <button class="row" title={r.detail ?? r.path} onclick={() => inLibrary(r.path)}>
          <span class="name">{r.name}</span>
          <span class="sub" class:bad={r.state === "failed"} class:live={r.state === "reading"}>{stateLabel[r.state] ?? r.state}</span>
        </button>
      {:else}
        <p class="note">Nothing waiting. Raw empties as each source is read.</p>
      {/each}

      <div class="divider">ingested · {overview.ingested.length}</div>
      {#each overview.ingested as s (s.id)}
        <button class="row" class:current={s.id === selected} onclick={() => (selected = s.id)}>
          <span class="name">{s.title}</span>
          <span class="sub">
            {s.kind || "source"} · {timeAgo(s.at)}
            {#if s.waiting > 0} · {s.waiting} waiting{/if}
            {#if !s.open} · seen{/if}
          </span>
        </button>
      {:else}
        <p class="note">Nothing ingested yet.</p>
      {/each}
    {/if}
  </div>

  <div class="right">
    {#if card}
      <div class="card-head">
        <h3>{card.takeaways.title || card.note.split("/").pop()}</h3>
        {#if card.takeaways.kind}<span class="chip">{card.takeaways.kind}</span>{/if}
        {#if card.takeaways.present.length > 0}<span class="chip">{card.takeaways.present.join(" · ")}</span>{/if}
        {#if !card.open}<span class="chip dim">seen</span>{/if}
      </div>

      <section class="takeaways">
        <div class="label">key takeaways · the note is in Ingested</div>
        <div class="md">
          {@html renderMarkdown(card.takeaways.overturns || "Nothing it overturns.")}
        </div>
        <div class="label">contradictions</div>
        {#if card.takeaways.contradictions.length > 0}
          <ul>
            {#each card.takeaways.contradictions as c (c)}<li>{@html renderMarkdown(c)}</li>{/each}
          </ul>
        {:else}
          <p class="note">None with the wiki or within the source.</p>
        {/if}
      </section>

      {#each groups as g (g.label)}
        <div class="divider">{g.label} · {g.rows.length}</div>
        {#each g.rows as p (p.id)}
          <div class="proposal">
            <div class="p-head">
              <span class="p-kind">{p.kind}</span>
              <span class="p-title">{p.page || p.title}</span>
              <span class="p-actions">
                {#if p.kind !== "ticket" && p.payload}
                  <button class="btn btn-ghost" onclick={() => (openDiff = openDiff === p.id ? null : p.id)}>
                    {openDiff === p.id ? "Hide" : "Show"} the change
                  </button>
                {/if}
                <button class="btn btn-primary" disabled={busy !== null} onclick={() => apply(p)}>{g.apply}</button>
                <button class="btn btn-ghost" disabled={busy !== null} onclick={() => discard(p)}>Discard</button>
              </span>
            </div>
            <div class="p-body">{p.body}</div>
            {#if openDiff === p.id}<ProposalDetail item={asItem(p)} />{/if}
          </div>
        {/each}
      {/each}
      {#if groups.length === 0}
        <div class="divider">proposed changes</div>
        <p class="note">Nothing waits: every change from this source is applied or discarded.</p>
      {/if}

      {#if card.takeaways.actions.length > 0 || card.takeaways.questions.length > 0}
        <div class="divider">actions and open questions</div>
        <ul>
          {#each card.takeaways.actions as a (a)}<li>{@html renderMarkdown(a)}</li>{/each}
          {#each card.takeaways.questions as q (q)}<li class="q">{@html renderMarkdown(q)}</li>{/each}
        </ul>
      {/if}

      <div class="footer">
        <button class="btn btn-ghost" onclick={() => card && inLibrary(card.note)}>Open the note</button>
        {#if card.open}
          <button class="btn btn-ghost" disabled={busy !== null} onclick={undo}>Undo all</button>
          <button class="btn btn-primary" disabled={busy !== null} onclick={seen}>Seen, file it</button>
        {/if}
      </div>
    {:else if overview && overview.ingested.length === 0}
      <p class="note">When a source is read, its key takeaways and the changes it proposes show here.</p>
    {/if}
  </div>
</div>

<style>
  .ingest {
    display: grid;
    grid-template-columns: 320px minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }
  .left {
    border-right: 1px solid var(--border);
    padding: 18px 16px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .left.dropping {
    background: color-mix(in srgb, var(--accent) 6%, transparent);
    outline: 2px dashed color-mix(in srgb, var(--accent) 45%, transparent);
    outline-offset: -6px;
  }
  .right {
    padding: 18px 24px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .head,
  .card-head {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  h3 {
    margin: 0;
    font-size: 16px;
  }
  .chip {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 99px;
    border: 1px solid var(--border);
    color: var(--ink-secondary, var(--ink));
  }
  .chip.dim {
    color: var(--ink-tertiary);
  }
  .drop {
    display: flex;
    flex-direction: column;
    gap: 4px;
    text-align: left;
    border: 1.5px dashed var(--border-strong);
    border-radius: 10px;
    background: var(--surface);
    padding: 14px;
    font-size: 13px;
    color: var(--ink);
    cursor: pointer;
    margin: 8px 0 4px;
  }
  .drop:hover {
    border-color: var(--accent);
  }
  .divider {
    margin-top: 12px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border);
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--ink-tertiary);
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: 2px;
    text-align: left;
    border: none;
    background: transparent;
    padding: 7px 8px;
    border-radius: 8px;
    color: var(--ink);
    cursor: pointer;
  }
  .row:hover {
    background: var(--sunken);
  }
  .row.current {
    background: color-mix(in srgb, var(--accent) 9%, transparent);
  }
  .name {
    font-size: 13px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    font-size: 11.5px;
    color: var(--ink-tertiary);
  }
  .sub.bad {
    color: var(--danger);
  }
  .sub.live {
    color: var(--accent);
  }
  .note {
    margin: 4px 0;
    font-size: 12.5px;
    color: var(--ink-tertiary);
  }
  .warn {
    margin: 4px 0;
    font-size: 12.5px;
    color: var(--needs-input);
  }
  .takeaways {
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .label {
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--ink-tertiary);
  }
  .md :global(p) {
    margin: 0 0 6px;
  }
  ul {
    margin: 0;
    padding-left: 18px;
    font-size: 13px;
    line-height: 1.5;
  }
  li :global(p) {
    margin: 0;
  }
  li.q {
    color: var(--ink-secondary, var(--ink));
  }
  .proposal {
    border: 1px solid var(--border);
    border-radius: 9px;
    padding: 9px 11px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .p-head {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .p-kind {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--ink-tertiary);
  }
  .p-title {
    font-size: 13px;
    font-weight: 600;
    flex: 1;
    min-width: 0;
  }
  .p-actions {
    display: flex;
    gap: 6px;
  }
  .p-body {
    font-size: 12.5px;
    color: var(--ink-secondary, var(--ink));
  }
  .footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }
</style>
