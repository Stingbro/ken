<script lang="ts">
  // The workspace's repos after set-up: each member's kind, team and index
  // state, changeable in place, and Scan again, which lists what moved
  // (a new folder, a new worktree, a repo gone) and changes nothing until a
  // person adds or skips it. Knowledge-layer step 4.
  import { onMount } from "svelte";
  import {
    api,
    memberLeaf,
    type IndexState,
    type RegistryEntryStatus,
    type RepoKind,
    type SetupMoved,
  } from "../lib/api";
  import { app } from "../lib/app.svelte";
  import { KINDS, toggleKind } from "../onboarding/setupFlow";

  let entries = $state<RegistryEntryStatus[]>([]);
  let moved = $state<SetupMoved[] | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);

  const members = $derived(
    (app.workspace?.members ?? [])
      .filter((m) => m.projectId)
      .map((m) => ({ member: m, entry: entries.find((e) => e.id === m.projectId) })),
  );

  function effectiveIndex(e: RegistryEntryStatus | undefined): IndexState {
    if (e?.index) return e.index;
    const kind = e?.kind ?? [];
    return kind.length === 0 || kind.some((k) => k === "team" || k === "wiki") ? "entities" : "search";
  }

  async function load() {
    try {
      entries = await api.listProjects();
    } catch (e) {
      error = String(e);
    }
  }

  async function setKind(e: RegistryEntryStatus, kind: RepoKind[]) {
    entries = await api.setProjectKind(e.id, kind, e.team ?? null);
  }
  async function setTeam(e: RegistryEntryStatus, team: string) {
    entries = await api.setProjectKind(e.id, e.kind ?? [], team.trim() || null);
  }
  async function setIndex(e: RegistryEntryStatus, index: IndexState) {
    entries = await api.setProjectIndex(e.id, index);
  }

  async function scanAgain() {
    if (!app.workspace) return;
    busy = true;
    error = null;
    try {
      moved = await api.setupRescan(app.workspace.root);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  /** Add a new folder with what the scan proposes for it; Settings can
   *  change its kind right after. */
  async function add(member: string) {
    if (!app.workspace) return;
    busy = true;
    try {
      const p = await api.setupPropose(app.workspace.root);
      const row = p.rows.find((r) => r.member === member);
      if (!row) throw new Error(`${member} is no longer there`);
      await app.confirmSetup(app.workspace.root, app.workspace.name, [{ ...row, include: true }], []);
      await load();
      moved = await api.setupRescan(app.workspace.root);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function skip(pattern: string, evidence: string) {
    if (!app.workspace) return;
    busy = true;
    try {
      await app.confirmSetup(app.workspace.root, app.workspace.name, [], [
        { pattern, state: "off", reason: "Git worktree", evidence, ticked: true, fixed: false },
      ]);
      moved = await api.setupRescan(app.workspace.root);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  onMount(() => void load());
</script>

<div class="repos">
  <p class="note">
    What each repo is decides what Ken does there: team and wiki repos sync and are read for
    entities; code and reference repos are searchable only, and Ken never commits into them.
  </p>
  {#each members as { member, entry } (member.name)}
    <div class="row">
      <span class="mono name" title={member.name}>{memberLeaf(member.name)}</span>
      {#if entry}
        <span class="kinds">
          {#each KINDS as k}
            <button
              class="chip"
              class:on={(entry.kind ?? []).includes(k)}
              aria-pressed={(entry.kind ?? []).includes(k)}
              onclick={() => void setKind(entry, toggleKind(entry.kind ?? [], k))}>{k}</button
            >
          {/each}
        </span>
        <input
          class="team"
          placeholder="team"
          value={entry.team ?? ""}
          onchange={(e) => void setTeam(entry, e.currentTarget.value)}
          aria-label="Team for {member.name}"
        />
        <select
          value={effectiveIndex(entry)}
          onchange={(e) => void setIndex(entry, e.currentTarget.value as IndexState)}
          aria-label="Index state for {member.name}"
        >
          <option value="entities">read for entities</option>
          <option value="search">searchable only</option>
          <option value="off">not indexed</option>
        </select>
      {:else}
        <span class="note">not in Ken's list yet</span>
      {/if}
    </div>
  {/each}

  <div class="actions">
    <button class="btn btn-ghost" disabled={busy} onclick={scanAgain}>{busy ? "Scanning…" : "Scan again"}</button>
  </div>

  {#if moved}
    {#if moved.length === 0}
      <p class="note">Nothing moved since set-up.</p>
    {:else}
      <ul class="moved">
        {#each moved as m, i (i)}
          <li>
            {#if m.change === "NewFolder"}
              New folder <span class="mono">{m.member}</span>
              {#if m.evidence.length > 0}<span class="evidence">({m.evidence.join(" · ")})</span>{/if}
              <button class="btn-mini" disabled={busy} onclick={() => void add(m.member)}>Add</button>
            {:else if m.change === "NewWorktree"}
              New worktree <span class="mono">{m.pattern}</span>
              <span class="evidence">({m.evidence})</span>
              <button class="btn-mini" disabled={busy} onclick={() => void skip(m.pattern, m.evidence)}>Skip it</button>
            {:else}
              <span class="mono">{m.member}</span> is gone from disk; it stays listed until you remove it.
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
  {#if error}<p class="warn">{error}</p>{/if}
</div>

<style>
  .repos {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(120px, 1fr) auto 110px 150px;
    gap: 8px;
    align-items: center;
    font-size: 12.5px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .kinds {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .chip {
    padding: 1px 8px;
    font-size: 11.5px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: transparent;
    color: var(--ink-secondary);
    cursor: pointer;
  }
  .chip.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .note {
    margin: 0;
    font-size: 12.5px;
    color: var(--ink-tertiary);
    line-height: 1.5;
  }
  .evidence {
    font-size: 11.5px;
    color: var(--ink-tertiary);
  }
  .moved {
    margin: 0;
    padding-left: 18px;
    font-size: 12.5px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .btn-mini {
    margin-left: 6px;
    padding: 1px 8px;
    font-size: 11.5px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: transparent;
    color: var(--ink);
    cursor: pointer;
  }
  .warn {
    margin: 0;
    font-size: 12.5px;
    color: var(--needs-input);
  }
</style>
