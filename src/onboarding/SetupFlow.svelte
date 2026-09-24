<script lang="ts">
  // Set-up for Ken on its own: Folder, Team, Repos, Index (step 4 of the
  // knowledge-layer plan). The scan proposes; every cell can change; nothing
  // is written until Confirm, and Ken reads nothing until then.
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import {
    api,
    type IndexState,
    type RepoKind,
    type SetupIgnoreRow,
    type SetupProposal,
    type SetupRepoRow,
  } from "../lib/api";
  import { app } from "../lib/app.svelte";
  import { indexSummary, KINDS, noTeams, toggleKind } from "./setupFlow";

  let { onclose }: { onclose: () => void } = $props();

  type Step = "folder" | "team" | "repos" | "index";
  let step = $state<Step>("folder");
  let proposal = $state<SetupProposal | null>(null);
  let rows = $state<SetupRepoRow[]>([]);
  let ignores = $state<SetupIgnoreRow[]>([]);
  let name = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  const summary = $derived(indexSummary(rows));

  async function chooseFolder() {
    error = null;
    const folder = await openDialog({ directory: true, title: "Where is your code?" });
    if (typeof folder !== "string") return;
    busy = true;
    try {
      proposal = await api.setupPropose(folder);
      rows = proposal.rows.map((r) => ({ ...r, kind: [...r.kind], evidence: [...r.evidence] }));
      ignores = proposal.ignores.map((i) => ({ ...i }));
      name = folder.split(/[\\/]/).pop() || "Code";
    } catch (e) {
      error = String(e);
      proposal = null;
    } finally {
      busy = false;
    }
  }

  function kenOnly() {
    rows = noTeams(rows);
    step = "repos";
  }

  async function confirm() {
    if (!proposal || busy) return;
    busy = true;
    error = null;
    try {
      await app.confirmSetup(proposal.folder, name.trim() || "Code", rows, ignores);
      onclose();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  const STATES: { value: IndexState; label: string }[] = [
    { value: "entities", label: "read for entities" },
    { value: "search", label: "searchable only" },
    { value: "off", label: "not indexed" },
  ];
</script>

<div class="setup">
  <ol class="steps" aria-label="Set-up steps">
    {#each ["folder", "team", "repos", "index"] as s, i}
      <li class:current={step === s}>{i + 1} · {s[0].toUpperCase() + s.slice(1)}</li>
    {/each}
  </ol>

  {#if step === "folder"}
    <h2>Where is your code?</h2>
    <p class="note">Ken counts what is under the folder and reads nothing until you confirm.</p>
    <button class="btn btn-primary" disabled={busy} onclick={chooseFolder}>
      {busy ? "Scanning…" : proposal ? "Choose another folder…" : "Choose the folder…"}
    </button>
    {#if proposal}
      <p class="counts">
        <strong>{proposal.repos}</strong> repositories · <strong>{proposal.withoutGit}</strong>
        folders without git · in <span class="mono">{proposal.folder}</span>
        {#if proposal.existingWorkspace}· already a Ken workspace, Confirm updates it{/if}
      </p>
      <div class="actions">
        <button class="btn btn-primary" onclick={() => (step = "team")}>Next</button>
        <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
      </div>
    {:else}
      <div class="actions"><button class="btn btn-ghost" onclick={onclose}>Cancel</button></div>
    {/if}
  {:else if step === "team"}
    <h2>Who do you work with?</h2>
    <p class="note">
      A team is a name over a set of repos. Picking a team picks its repos at once, and its
      graph is built over them.
    </p>
    <div class="choices">
      <button class="choice" onclick={() => (step = "repos")}>
        <span class="choice-title">Use the teams this folder suggests</span>
        <span class="choice-note">
          {#if proposal && proposal.teams.length > 0}
            {proposal.teams.join(" · ")}, from group folders and docs READMEs that name their code.
          {:else}
            None found; every repo is a team of one. You can type team names on the next step.
          {/if}
        </span>
      </button>
      <button class="choice" onclick={kenOnly}>
        <span class="choice-title">None, Ken only</span>
        <span class="choice-note">Index the folder as one workspace, no teams.</span>
      </button>
    </div>
    <div class="actions"><button class="btn btn-ghost" onclick={() => (step = "folder")}>Back</button></div>
  {:else if step === "repos"}
    <h2>Repos</h2>
    <p class="note">
      The proposal, one row per repo, evidence under the name. Every cell can change. Team and
      wiki repos are read for entities; code and reference repos are searchable only; Ken never
      commits into a code repo.
    </p>
    <div class="table" role="table">
      <div class="tr head" role="row">
        <span>Include</span><span>Repo</span><span>Team</span><span>Kind</span><span>Index</span>
      </div>
      {#each rows as row (row.member)}
        <div class="tr" role="row" class:dim={!row.include}>
          <span><input type="checkbox" bind:checked={row.include} aria-label="Include {row.member}" /></span>
          <span class="repo">
            <span class="mono">{row.member}</span>
            {#if row.evidence.length > 0}<span class="evidence">{row.evidence.join(" · ")}</span>{/if}
          </span>
          <span>
            <input
              class="team"
              placeholder="—"
              value={row.team ?? ""}
              oninput={(e) => (row.team = (e.currentTarget as HTMLInputElement).value.trim() || null)}
              aria-label="Team for {row.member}"
            />
          </span>
          <span class="kinds">
            {#each KINDS as k}
              <button
                class="chip"
                class:on={row.kind.includes(k)}
                onclick={() => (row.kind = toggleKind(row.kind, k as RepoKind))}
                aria-pressed={row.kind.includes(k)}>{k}</button
              >
            {/each}
          </span>
          <span>
            <select bind:value={row.index} aria-label="Index state for {row.member}">
              {#each STATES as s}<option value={s.value}>{s.label}</option>{/each}
            </select>
          </span>
        </div>
      {/each}
    </div>

    <h3>Skip list</h3>
    <p class="note">Lines Confirm adds to the folder's <span class="mono">.kenignore</span>, under a dated comment.</p>
    <ul class="ignores">
      {#each ignores as ig (ig.pattern)}
        <li>
          <label>
            <input type="checkbox" bind:checked={ig.ticked} disabled={ig.fixed} />
            <span class="mono">{ig.pattern}</span>
            <span class="evidence">{ig.reason}: {ig.evidence}</span>
          </label>
        </li>
      {/each}
    </ul>
    <div class="actions">
      <button class="btn btn-primary" onclick={() => (step = "index")}>Next</button>
      <button class="btn btn-ghost" onclick={() => (step = "team")}>Back</button>
    </div>
  {:else}
    <h2>Index</h2>
    <p class="note">
      Confirm writes the workspace, each repo's kind, team and index state, and the ticked skip
      lines. Then Ken starts reading, in the background; you can use it while it runs.
    </p>
    <p class="counts">
      <strong>{summary.entities}</strong> read for entities · <strong>{summary.search}</strong>
      searchable only · <strong>{summary.off}</strong> not indexed
    </p>
    <label class="name">
      Workspace name <input bind:value={name} />
    </label>
    <div class="actions">
      <button class="btn btn-primary" disabled={busy || summary.entities + summary.search === 0} onclick={confirm}>
        {busy ? "Setting up…" : "Confirm and start reading"}
      </button>
      <button class="btn btn-ghost" onclick={() => (step = "repos")}>Back</button>
    </div>
  {/if}

  {#if error}<div class="error">{error}</div>{/if}
</div>

<style>
  .setup {
    display: flex;
    flex-direction: column;
    gap: 12px;
    text-align: left;
  }
  .steps {
    display: flex;
    gap: 14px;
    margin: 0 0 4px;
    padding: 0;
    list-style: none;
    font-size: 12px;
    color: var(--ink-tertiary);
  }
  .steps .current {
    color: var(--accent);
    font-weight: 600;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  h3 {
    margin: 8px 0 0;
    font-size: 14px;
  }
  .note {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
    color: var(--ink-secondary);
  }
  .counts {
    margin: 0;
    font-size: 13px;
    color: var(--ink-secondary);
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }
  .choices {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .choice {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 12px 14px;
    text-align: left;
    background: var(--sunken-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    cursor: pointer;
    color: var(--ink);
  }
  .choice:hover {
    border-color: var(--accent);
  }
  .choice-title {
    font-weight: 600;
    font-size: 14px;
  }
  .choice-note {
    font-size: 12.5px;
    color: var(--ink-secondary);
  }
  .table {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: auto;
    max-height: 46vh;
  }
  .tr {
    display: grid;
    grid-template-columns: 56px minmax(180px, 2fr) 110px minmax(200px, 1.4fr) 150px;
    gap: 8px;
    align-items: start;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    font-size: 12.5px;
  }
  .tr:last-child {
    border-bottom: none;
  }
  .tr.head {
    position: sticky;
    top: 0;
    background: var(--sunken-2);
    font-weight: 600;
    color: var(--ink-secondary);
  }
  .tr.dim {
    opacity: 0.55;
  }
  .repo {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .evidence {
    font-size: 11.5px;
    color: var(--ink-tertiary);
  }
  .team {
    width: 100%;
  }
  .kinds {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
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
  .ignores {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12.5px;
  }
  .ignores label {
    display: flex;
    gap: 8px;
    align-items: baseline;
    flex-wrap: wrap;
  }
  .name {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 13px;
  }
  .error {
    color: var(--needs-input);
    font-size: 13px;
  }
</style>
