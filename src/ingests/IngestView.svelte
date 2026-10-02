<script lang="ts">
  // Ingest, the inbox that empties (the Wright white box, frame 8b). Four
  // ways to add a source: drop or choose files, record, write a note, or take
  // a chat. Each lands in the team library's Research/Ingestion/Raw/ and is
  // read at once into a dated note in Ingested/. What follows from the note
  // is written then and there, each write citing the note: page edits and
  // new pages, ideas, escalations, my next steps. The card is read afterwards, not confirmed: each write has
  // Open and Undo. Only four things wait: a ruling for its decider, a change
  // to Ways-of-Working as a ticket, an edit staging held, an action as a
  // ticket. Seen files the source beside its note; Undo all takes it back.
  import { onMount, untrack } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, type IngestCard, type IngestOverview, type IngestProposal, type IngestWrite } from "../lib/api";
  import { app } from "../lib/app.svelte";
  import { scope } from "../lib/scope.svelte";
  import { record } from "../lib/record.svelte";
  import { rail } from "../lib/rail.svelte";
  import { renderMarkdown } from "../lib/markdown";
  import { timeAgo } from "../lib/format";
  import { errorText } from "../lib/toast.svelte";
  import { claudeInstallHelp } from "../lib/platform";
  import { openConfirm } from "../lib/ui/ConfirmMenu.svelte";
  import {
    canReadNow,
    ingestBusy,
    pageName,
    rawActions,
    rawStateLabel,
    stagingLine,
    writtenLine,
  } from "../lib/ingestCard";
  import ProposalDetail from "./ProposalDetail.svelte";
  import RecordPanel from "./RecordPanel.svelte";
  import ChatSource from "./ChatSource.svelte";

  let overview = $state<IngestOverview | null>(null);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let selected = $state<number | null>(null);
  let card = $state<IngestCard | null>(null);
  let busy = $state<string | null>(null);
  let dropping = $state(false);
  let adding = $state(0);
  let openDiff = $state<number | null>(null);

  // The four ways to add a source.
  type Source = "files" | "record" | "note" | "chat";
  const SOURCES: { v: Source; label: string }[] = [
    { v: "files", label: "Files" },
    { v: "record", label: "Record" },
    { v: "note", label: "Note" },
    { v: "chat", label: "Chat" },
  ];
  let source = $state<Source>("files");
  let noteTitle = $state("");
  let noteText = $state("");

  const team = $derived(scope.team);

  // Coming to Ingest while a take runs (the title-bar pill) shows the recorder.
  $effect(() => {
    if (app.screen === "ingests" && (record.recording || record.transcribing)) source = "record";
  });
  // A finished take is in Raw now.
  $effect(() => {
    if (record.savedPath) untrack(() => void refresh());
  });

  // Read sources still in Raw have their card; under them, the rest.
  const rawCards = $derived(new Set((overview?.raw ?? []).map((r) => r.cardId).filter((id) => id !== null)));
  const ingested = $derived((overview?.ingested ?? []).filter((s) => !rawCards.has(s.id)));

  async function refresh() {
    try {
      overview = await api.ingestOverview(team);
      rail.setIngest(overview.waiting ?? 0);
      error = null;
      // The newest read source is shown until the person picks another.
      if (selected === null) {
        selected = overview.raw.find((r) => r.cardId !== null)?.cardId ?? overview.ingested[0]?.id ?? null;
      }
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

  async function reload() {
    await refresh();
    await loadCard(selected);
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
      if (ingestBusy(overview)) void reload();
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

  async function addNote() {
    const text = noteText.trim();
    if (!text) return;
    busy = "note";
    try {
      await api.ingestAddText(team, noteTitle.trim() || null, text);
      noteTitle = "";
      noteText = "";
      notice = "The note is in Raw. Ken reads it now.";
      error = null;
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = null;
      await refresh();
    }
  }

  async function chatAdded() {
    notice = "The chat is in Raw. Ken reads it now.";
    error = null;
    await refresh();
  }

  async function readNow() {
    busy = "now";
    try {
      const started = await api.ingestNow(team);
      notice = started ? "Reading what is queued." : "A read is already running; the queue is next.";
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = null;
      await refresh();
    }
  }

  async function retry(r: IngestOverview["raw"][number]) {
    busy = `r${r.path}`;
    try {
      const started = await api.ingestRetry(team, r.path);
      notice = started ? `Reading ${r.name} again.` : `${r.name} is read on the next pass.`;
      error = null;
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = null;
      await refresh();
    }
  }

  function askRemove(e: MouseEvent, r: IngestOverview["raw"][number]) {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openConfirm(rect.left, rect.bottom + 4, {
      title: `Remove ${r.name} from Raw?`,
      body: "It goes to the system trash, and Ken does not read it.",
      confirmLabel: "Move to Trash",
      onConfirm: () => void remove(r),
    });
  }

  async function remove(r: IngestOverview["raw"][number]) {
    busy = `x${r.path}`;
    try {
      await api.ingestRemove(team, r.path);
      notice = `${r.name} is in the trash.`;
      error = null;
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = null;
      await refresh();
    }
  }

  async function openIn(projectId: string, path: string) {
    if (app.focused !== projectId) await app.focusMember(projectId);
    app.openInFiles(path);
  }

  async function inLibrary(path: string) {
    if (overview) await openIn(overview.projectId, path);
  }

  async function openWrite(w: IngestWrite) {
    if (w.kind === "task") {
      app.screen = "home";
      return;
    }
    if (!w.projectId) {
      error = `${w.path} is in a repo that is not open in this workspace.`;
      return;
    }
    await openIn(w.projectId, w.path);
  }

  async function undoWrite(w: IngestWrite) {
    if (!card) return;
    busy = `w${w.index}`;
    notice = null;
    try {
      await api.ingestUndoWrite(card.id, w.index, team);
      error = null;
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
      await reload();
    }
  }

  /** A ruling's words, from its waiting item. */
  function rulingOf(p: IngestProposal): string {
    try {
      return (JSON.parse(p.payload ?? "{}") as { append?: { ruling?: string } }).append?.ruling ?? p.title;
    } catch {
      return p.title;
    }
  }

  async function apply(p: IngestProposal) {
    if (!overview) return;
    busy = `p${p.id}`;
    try {
      await api.applyPageProposal(p.id, overview.projectId);
      error = null;
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
      await reload();
    }
  }

  async function discard(p: IngestProposal) {
    if (!overview) return;
    busy = `p${p.id}`;
    try {
      await api.resolveReviewItem(p.id, overview.projectId);
    } finally {
      busy = null;
      await reload();
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
      await reload();
    }
  }

  async function undoAll() {
    if (!card) return;
    busy = "undo";
    try {
      const report = await api.ingestUndo(card.id, team);
      const kept = report.kept.length > 0 ? ` Kept, changed since: ${report.kept.join(", ")}.` : "";
      const note = report.noteRemoved ? "" : " The note was edited, so it stays.";
      notice = `Undone.${kept}${note}`;
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
      await refresh();
    }
  }

  async function readAgain() {
    if (!card) return;
    busy = "again";
    try {
      const started = await api.ingestReadAgain(card.id, team);
      notice = started ? "Reading it again." : "It is read on the next pass.";
      selected = null;
    } catch (e) {
      error = String(e);
    } finally {
      busy = null;
      await refresh();
    }
  }

  const written = $derived(card?.writes ?? []);
  const live = $derived(written.filter((w) => !w.undone));
  const rulings = $derived((card?.proposals ?? []).filter((p) => p.kind === "ruling"));
  const tickets = $derived((card?.proposals ?? []).filter((p) => p.kind === "ticket"));
  const held = $derived((card?.proposals ?? []).filter((p) => p.kind === "page" || p.kind === "new page"));
  const pageWrites = $derived(written.filter((w) => w.kind === "edit" || w.kind === "page").length);

  const writeTag: Record<IngestWrite["kind"], string> = {
    edit: "page",
    page: "new page",
    idea: "idea",
    escalation: "escalation",
    task: "my day",
  };

  function rawSub(r: IngestOverview["raw"][number]): string {
    const bits = [r.kind, r.length, r.present.length > 0 ? `${r.present.length} present` : "", rawStateLabel(r.state)];
    if (r.state === "read") bits.push(writtenLine(r.written, r.waiting));
    return bits.filter((b) => b).join(" · ");
  }
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

    <div class="sources" role="tablist" aria-label="Add a source">
      {#each SOURCES as s (s.v)}
        <button
          class="seg"
          class:on={source === s.v}
          role="tab"
          aria-selected={source === s.v}
          onclick={() => (source = s.v)}
        >
          {s.label}{#if s.v === "record" && record.recording}<span class="rec-dot" title="Recording"></span>{/if}
        </button>
      {/each}
    </div>

    {#if source === "files"}
      <button class="drop" onclick={chooseFiles}>
        <strong>Drop files</strong> or choose them
        <span class="sub">A transcript, a recording, a document or notes. Ken files each in Raw/ and reads it.</span>
      </button>
    {:else if source === "record"}
      <div class="panel"><RecordPanel {team} /></div>
    {:else if source === "note"}
      <div class="panel note-form">
        <input bind:value={noteTitle} placeholder="Title (optional)" aria-label="Title" />
        <textarea bind:value={noteText} rows="7" placeholder="What was said, decided or found" aria-label="Note"></textarea>
        <div class="panel-actions">
          <button class="btn btn-small btn-primary" disabled={!noteText.trim() || busy !== null} onclick={addNote}>Add</button>
        </div>
      </div>
    {:else}
      <div class="panel"><ChatSource {team} onAdded={chatAdded} /></div>
    {/if}
    {#if adding > 0}<p class="note">Adding {adding} {adding === 1 ? "file" : "files"}…</p>{/if}
    {#if error}<p class="warn">{error}</p>{/if}
    {#if notice}<p class="note">{notice}</p>{/if}
    {#if overview && !overview.claudeFound}
      <p class="warn">Reading needs Claude Code. {claudeInstallHelp()}</p>
    {/if}

    {#if overview}
      <div class="divider with-action">
        <span>raw · {overview.raw.length}</span>
        {#if canReadNow(overview)}
          <button class="link" disabled={busy !== null} onclick={readNow}>Read now</button>
        {/if}
      </div>
      {#each overview.raw as r (r.path)}
        {@const actions = rawActions(r.state)}
        <div class="rawrow" class:current={r.cardId !== null && r.cardId === selected}>
          <button
            class="row"
            title={r.path}
            onclick={() => (r.cardId !== null ? (selected = r.cardId) : inLibrary(r.path))}
          >
            <span class="name">{r.name}</span>
            <span
              class="sub"
              class:bad={r.state === "failed"}
              class:wait={r.state === "waiting"}
              class:live={r.state === "reading" || r.state === "transcribing"}>{rawSub(r)}</span
            >
          </button>
          {#if (r.state === "failed" || r.state === "waiting") && r.detail}
            <p class="reason" class:bad={r.state === "failed"}>{r.detail}</p>
          {/if}
          {#if actions.length > 0}
            <div class="raw-actions">
              {#if actions.includes("retry")}
                <button class="btn btn-small btn-ghost" disabled={busy !== null} onclick={() => retry(r)}>Try again</button>
              {/if}
              {#if actions.includes("remove")}
                <button class="btn btn-small btn-ghost" disabled={busy !== null} onclick={(e) => askRemove(e, r)}>Remove</button>
              {/if}
            </div>
          {/if}
        </div>
      {:else}
        <p class="note">Nothing in Raw.</p>
      {/each}

      <div class="divider">ingested · {ingested.length}</div>
      {#each ingested as s (s.id)}
        <button class="row" class:current={s.id === selected} onclick={() => (selected = s.id)}>
          <span class="name">{s.title}</span>
          <span class="sub">
            {s.kind || "source"} · {timeAgo(s.at)} · {writtenLine(s.written, s.waiting)}{#if !s.open} · seen{/if}
          </span>
        </button>
      {:else}
        <p class="note">Nothing ingested yet.</p>
      {/each}
      <p class="note foot">Raw empties as each source is seen: it moves beside its note in Ingested. Neither folder is in a pack.</p>
    {/if}
  </div>

  <div class="right">
    {#if card}
      <div class="card-head">
        <h3>{card.takeaways.title || card.note.split("/").pop()}</h3>
        {#if card.takeaways.kind}<span class="chip">{card.takeaways.kind}</span>{/if}
        {#if card.takeaways.present.length > 0}<span class="chip">{card.takeaways.present.join(" · ")}</span>{/if}
        {#if card.takeaways.length}<span class="chip dim">{card.takeaways.length}</span>{/if}
        {#if !card.open}<span class="chip dim">seen</span>{/if}
      </div>

      {#if card.undone}
        <section class="undone">
          <p>Undone. What was written from this source is taken back, and the source is in Raw. It is not read again until you ask.</p>
          <div class="footer">
            <button class="btn btn-primary" disabled={busy !== null} onclick={readAgain}>Read it again</button>
          </div>
        </section>
      {:else}
      <section class="takeaways">
        <div class="label">
          key takeaways · the note is in Ingested ·
          <button class="link" onclick={() => card && inLibrary(card.note)}>open the note</button>
        </div>
        {#if card.takeaways.summary}
          <div class="md">{@html renderMarkdown(card.takeaways.summary)}</div>
        {/if}
        {#if card.takeaways.keyTakeaways.length > 0}
          <ul>
            {#each card.takeaways.keyTakeaways as k (k)}<li>{@html renderMarkdown(k)}</li>{/each}
          </ul>
        {/if}
        <div class="label">what it overturns</div>
        <div class="md">{@html renderMarkdown(card.takeaways.overturns || "Nothing it overturns.")}</div>
        <div class="label">contradictions</div>
        {#if card.takeaways.contradictions.length > 0}
          <ul>
            {#each card.takeaways.contradictions as c (c)}<li>{@html renderMarkdown(c)}</li>{/each}
          </ul>
        {:else}
          <p class="note">None with the wiki or within the source.</p>
        {/if}
      </section>

      <div class="divider">written · {live.length} · each cites the note · undo if wrong</div>
      {#each written as w (w.index)}
        <div class="wrow" class:undone={w.undone}>
          <span class="tag">{writeTag[w.kind]}</span>
          <span class="what">
            <span class="target">{w.kind === "task" ? w.label : pageName(w.path)}</span>
            {#if w.kind !== "task" && w.label}<span class="sub"> · {w.label}</span>{/if}
            {#if w.kind === "escalation" && w.to}<span class="sub"> · to {w.to}</span>{/if}
            {#if w.kind === "edit" || w.kind === "page"}<span class="sub"> · cites the note</span>{/if}
            {#if w.undone}<span class="sub"> · undone</span>{/if}
          </span>
          <span class="actions">
            {#if !w.undone}
              <button class="btn btn-ghost" onclick={() => openWrite(w)}>Open</button>
              <button class="btn btn-ghost" disabled={busy !== null} onclick={() => undoWrite(w)}>Undo</button>
            {/if}
          </span>
        </div>
      {:else}
        <p class="note">Nothing was written from this source.</p>
      {/each}
      {#if pageWrites > 0 || held.length > 0}
        <p class="note">{stagingLine(held.length)}</p>
      {/if}
      {#if card.listed.length > 0}
        <div class="divider">on this card only · nowhere to write it</div>
        <ul>
          {#each card.listed as l, i (i)}
            <li><span class="tag">{l.kind}</span> {l.text}{#if l.to} · {l.to}{/if}</li>
          {/each}
        </ul>
      {/if}

      <div class="divider">waits · {rulings.length + tickets.length + held.length}</div>
      {#each rulings as p (p.id)}
        <div class="wrow">
          <span class="tag">ruling</span>
          <span class="what">
            <span class="target">"{rulingOf(p)}"</span>
            {#if p.decider}<span class="sub"> · {p.decider}</span>{/if}
          </span>
          <span class="actions">
            <button
              class="btn btn-primary"
              disabled={busy !== null || !p.canAccept}
              title={p.canAccept ? "Write it to the decisions log" : `Only ${p.decider} can accept a ruling`}
              onclick={() => apply(p)}>{p.decider ? `Accept as ${p.decider}` : "Accept"}</button
            >
            <button class="btn btn-ghost" disabled={busy !== null} onclick={() => discard(p)}>Drop</button>
          </span>
        </div>
      {/each}
      {#each tickets as p (p.id)}
        <div class="wait">
          <div class="wrow">
            <span class="tag">ticket</span>
            <span class="what"><span class="target">{p.title}</span></span>
            <span class="actions">
              <button class="btn btn-ghost" onclick={() => (openDiff = openDiff === p.id ? null : p.id)}>
                {openDiff === p.id ? "Close" : "Open"}
              </button>
              <button class="btn btn-primary" disabled={busy !== null} onclick={() => apply(p)}>Accept</button>
              <button class="btn btn-ghost" disabled={busy !== null} onclick={() => discard(p)}>Discard</button>
            </span>
          </div>
          {#if openDiff === p.id}<ProposalDetail payload={p.payload} />{/if}
        </div>
      {/each}
      {#each held as p (p.id)}
        <div class="wait">
          <div class="wrow">
            <span class="tag">held</span>
            <span class="what">
              <span class="target">{pageName(p.page)}</span>
              <span class="sub"> · {p.body}</span>
            </span>
            <span class="actions">
              <button class="btn btn-primary" disabled={busy !== null} onclick={() => apply(p)}>Apply</button>
              <button class="btn btn-ghost" disabled={busy !== null} onclick={() => discard(p)}>Discard</button>
            </span>
          </div>
          <ProposalDetail payload={p.payload} />
        </div>
      {/each}
      {#if rulings.length + tickets.length + held.length === 0}
        <p class="note">Nothing waits.</p>
      {/if}

      <div class="footer">
        <button class="btn btn-ghost" disabled={busy !== null} onclick={undoAll}>Undo all</button>
        {#if card.open}
          <button class="btn btn-primary" disabled={busy !== null} onclick={seen}>Seen</button>
        {/if}
      </div>
      {/if}
    {:else if overview && overview.ingested.length === 0}
      <p class="note">When a source is read, its key takeaways, what was written from it and what waits show here.</p>
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
  .sources {
    display: flex;
    margin-top: 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    overflow: hidden;
  }
  .sources .seg {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 5px 0;
    border: none;
    background: var(--surface);
    color: var(--ink-secondary);
    font-size: 12px;
    font-weight: 500;
  }
  .sources .seg + .seg {
    border-left: 1px solid var(--border);
  }
  .sources .seg:hover {
    background: var(--sunken);
  }
  .sources .seg.on {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent-deep);
    font-weight: 600;
  }
  .rec-dot {
    width: 6px;
    height: 6px;
    border-radius: 3px;
    background: var(--danger);
  }
  .panel {
    margin: 8px 0 4px;
  }
  .note-form {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .note-form input,
  .note-form textarea {
    font: inherit;
    font-size: 12.5px;
    padding: 6px 8px;
    border-radius: 7px;
    border: 1px solid var(--border-strong);
    background: var(--surface);
    color: var(--ink);
  }
  .note-form textarea {
    resize: vertical;
    line-height: 1.5;
  }
  .panel-actions {
    display: flex;
    justify-content: flex-end;
  }
  .divider.with-action {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .divider .link {
    font-size: 11px;
    font-weight: 600;
    text-transform: none;
    letter-spacing: 0;
  }
  .divider .link:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .rawrow {
    display: flex;
    flex-direction: column;
    border-radius: 8px;
  }
  .rawrow.current {
    background: color-mix(in srgb, var(--accent) 9%, transparent);
  }
  .rawrow .row {
    width: 100%;
  }
  .reason {
    margin: -2px 8px 4px;
    font-size: 11.5px;
    line-height: 1.45;
    color: var(--needs-input-text);
    overflow-wrap: anywhere;
  }
  .reason.bad {
    color: var(--danger);
  }
  .raw-actions {
    display: flex;
    gap: 4px;
    padding: 0 4px 4px;
  }
  .sub.wait {
    color: var(--needs-input-text);
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
  .note.foot {
    margin-top: 12px;
    font-size: 11.5px;
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
  .link {
    border: none;
    background: none;
    padding: 0;
    font: inherit;
    letter-spacing: inherit;
    text-transform: inherit;
    color: var(--accent);
    cursor: pointer;
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
  .wait {
    display: flex;
    flex-direction: column;
  }
  .wrow {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 34px;
    padding: 0 4px;
    border-bottom: 1px solid var(--sunken);
  }
  .wrow.undone .target {
    color: var(--ink-tertiary);
    text-decoration: line-through;
  }
  .tag {
    flex: none;
    width: 74px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--ink-tertiary);
  }
  li .tag {
    display: inline-block;
    width: auto;
    margin-right: 4px;
  }
  .what {
    flex: 1;
    min-width: 0;
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .target {
    font-weight: 600;
  }
  .actions {
    flex: none;
    display: flex;
    gap: 6px;
  }
  .footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }
</style>
