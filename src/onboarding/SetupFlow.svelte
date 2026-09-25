<script lang="ts">
  // Set-up for Ken on its own: Repos, Team, Index (step 4 of the
  // knowledge-layer plan). A person picks the repos, one or many at a time,
  // wherever they are; the workspace lives in Ken's app data, so nothing is
  // written into a shared parent. Each row is proposed from what is in the
  // repo and every cell can change; nothing is written until Confirm.
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, type IndexState, type RepoKind, type SetupRepoRow } from "../lib/api";
  import { app } from "../lib/app.svelte";
  import { indexSummary, KINDS, mergeRows, noTeams, suggestedTeams, toggleKind } from "./setupFlow";

  let { onclose }: { onclose: () => void } = $props();

  type Step = "repos" | "team" | "index";
  let step = $state<Step>("repos");
  let picked = $state<string[]>([]);
  // Repos taken out, so a folder of repos picked again does not bring them back.
  let removed = $state<string[]>([]);
  let rows = $state<SetupRepoRow[]>([]);
  let name = $state("Code");
  let busy = $state(false);
  let error = $state<string | null>(null);

  const summary = $derived(indexSummary(rows));
  const teams = $derived(suggestedTeams(rows));

  // Item 4b: offered, never forced. Draft the first Current, architecture
  // and release pages into a wiki repo from these repos and their
  // descriptions, for a person to read.
  let draftWiki = $state(false);
  let draftInto = $state("");
  let draftExtra = $state<string | null>(null);
  const wikiRows = $derived(rows.filter((r) => r.include && r.kind.includes("wiki")));
  $effect(() => {
    if (!wikiRows.some((r) => r.member === draftInto)) draftInto = wikiRows[0]?.member ?? "";
  });

  async function chooseExtra() {
    const folder = await openDialog({ directory: true, title: "A folder of documents to read too (a Confluence export, say)" });
    if (typeof folder === "string") draftExtra = folder;
  }

  /** Pick one or more repos (a folder of repos stands for each inside it).
   *  The whole set is proposed again, so names stay unique; edits to rows
   *  already here are kept. */
  async function addRepos() {
    error = null;
    const chosen = await openDialog({ directory: true, multiple: true, title: "Pick the repos Ken should know" });
    const list = Array.isArray(chosen) ? chosen : typeof chosen === "string" ? [chosen] : [];
    if (list.length === 0) return;
    busy = true;
    try {
      const all = [...picked, ...list.filter((p) => !picked.includes(p))];
      const proposal = await api.setupProposeRepos(all);
      picked = all;
      // Picking a repo itself again undoes taking it out.
      removed = removed.filter((r) => !list.includes(r));
      rows = mergeRows(rows, proposal.rows.filter((r) => !r.path || !removed.includes(r.path)));
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function remove(row: SetupRepoRow) {
    rows = rows.filter((r) => r !== row);
    if (row.path) removed = [...removed, row.path];
  }

  function kenOnly() {
    rows = noTeams(rows);
    step = "index";
  }

  async function confirm() {
    if (busy) return;
    busy = true;
    error = null;
    try {
      await app.confirmSetupRepos(name.trim() || "Code", rows);
      if (draftWiki && draftInto) await api.draftWiki(draftInto, draftExtra);
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
    {#each ["repos", "team", "index"] as s, i}
      <li class:current={step === s}>{i + 1} · {s[0].toUpperCase() + s.slice(1)}</li>
    {/each}
  </ol>

  {#if step === "repos"}
    <h2>Which repos should Ken know?</h2>
    <p class="note">
      Pick each repo, wherever it lives; pick a folder of repos to add every repo in it. Ken reads
      what is in each one to propose what it is, and reads nothing until you confirm. Say in a line
      what each repo is for: it helps Ken use it well, and the first-wiki draft reads it.
    </p>
    <div class="actions">
      <button class="btn btn-primary" disabled={busy} onclick={addRepos}>
        {busy ? "Reading…" : rows.length === 0 ? "Pick repos…" : "Add more repos…"}
      </button>
    </div>

    {#if rows.length > 0}
      <div class="table" role="table">
        <div class="tr head" role="row">
          <span>Include</span><span>Repo</span><span>Kind</span><span>Index</span><span></span>
        </div>
        {#each rows as row (row.path ?? row.member)}
          <div class="tr" role="row" class:dim={!row.include}>
            <span><input type="checkbox" bind:checked={row.include} aria-label="Include {row.member}" /></span>
            <span class="repo">
              <span class="mono">{row.member}</span>
              <span class="evidence mono" title={row.path ?? ""}>{row.path}</span>
              {#if row.evidence.length > 0}<span class="evidence">{row.evidence.join(" · ")}</span>{/if}
              <textarea
                class="description"
                rows="2"
                placeholder="What is this repo for, and how is it used?"
                bind:value={row.description}
                aria-label="What {row.member} is for"
              ></textarea>
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
            <span>
              <button class="remove" title="Take it out" aria-label="Remove {row.member}" onclick={() => remove(row)}>✕</button>
            </span>
          </div>
        {/each}
      </div>
      <p class="note">
        Team and wiki repos are read for entities; code and reference repos are searchable only, and
        Ken never commits into them. Secrets (<span class="mono">.env</span>, keys,
        <span class="mono">credentials.json</span>) are never read.
      </p>
    {/if}
    <div class="actions">
      <button class="btn btn-primary" disabled={rows.every((r) => !r.include)} onclick={() => (step = "team")}>Next</button>
      <button class="btn btn-ghost" onclick={onclose}>Cancel</button>
    </div>
  {:else if step === "team"}
    <h2>Who do you work with?</h2>
    <p class="note">
      A team is a name over a set of repos. Picking a team picks its repos at once, and its graph
      is built over them.
    </p>
    <div class="table" role="table">
      <div class="tr head teams" role="row"><span>Repo</span><span>Team</span></div>
      {#each rows.filter((r) => r.include) as row (row.path ?? row.member)}
        <div class="tr teams" role="row">
          <span class="mono">{row.member}</span>
          <input
            class="team"
            placeholder="—"
            value={row.team ?? ""}
            oninput={(e) => (row.team = (e.currentTarget as HTMLInputElement).value.trim() || null)}
            aria-label="Team for {row.member}"
          />
        </div>
      {/each}
    </div>
    <p class="note">
      {#if teams.length > 0}
        Suggested: {teams.join(" · ")}, from folders of repos and docs READMEs that name their code.
      {:else}
        No team suggested; type a name to group repos, or leave them each on their own.
      {/if}
    </p>
    <div class="actions">
      <button class="btn btn-primary" onclick={() => (step = "index")}>Next</button>
      <button class="btn btn-ghost" onclick={kenOnly}>None, Ken only</button>
      <button class="btn btn-ghost" onclick={() => (step = "repos")}>Back</button>
    </div>
  {:else}
    <h2>Index</h2>
    <p class="note">
      Confirm writes the workspace in Ken's own data, and each repo's kind, team, index state and
      description. Then Ken starts reading, in the background; you can use it while it runs.
    </p>
    <p class="counts">
      <strong>{summary.entities}</strong> read for entities · <strong>{summary.search}</strong>
      searchable only · <strong>{summary.off}</strong> not indexed
    </p>
    <label class="name">
      Workspace name <input bind:value={name} />
    </label>
    {#if wikiRows.length > 0}
      <div class="draft">
        <label>
          <input type="checkbox" bind:checked={draftWiki} />
          Draft the first wiki pages from these repos
        </label>
        {#if draftWiki}
          <p class="note">
            Claude reads each repo's description, READMEs, docs, layout, release tags and who
            commits where, and drafts Current/Project, Team, Who-Does-What, the architecture page
            and the release notes into
            <select bind:value={draftInto} aria-label="Wiki to draft into">
              {#each wikiRows as w (w.member)}<option value={w.member}>{w.member}</option>{/each}
            </select>.
            Each page is marked draft and names its sources; a page someone already wrote is left
            alone. A card on Review lists what was drafted.
          </p>
          <button class="btn btn-ghost" onclick={chooseExtra}>
            {draftExtra ? `Also reading ${draftExtra.split(/[\\/]/).pop()}` : "Also read a folder of documents…"}
          </button>
        {/if}
      </div>
    {/if}
    <div class="actions">
      <button class="btn btn-primary" disabled={busy || summary.entities + summary.search === 0} onclick={confirm}>
        {busy ? "Setting up…" : "Confirm and start reading"}
      </button>
      <button class="btn btn-ghost" onclick={() => (step = "team")}>Back</button>
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
    grid-template-columns: 56px minmax(220px, 2.4fr) minmax(170px, 1.2fr) 140px 28px;
    gap: 8px;
    align-items: start;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    font-size: 12.5px;
  }
  .tr.teams {
    grid-template-columns: minmax(160px, 1fr) minmax(160px, 1fr);
    align-items: center;
  }
  .description {
    width: 100%;
    margin-top: 4px;
    font: inherit;
    font-size: 12.5px;
    resize: vertical;
  }
  .remove {
    border: none;
    background: none;
    color: var(--ink-tertiary);
    cursor: pointer;
  }
  .remove:hover {
    color: var(--needs-input);
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
  .name {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 13px;
  }
  .draft {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 8px;
    font-size: 13px;
  }
  .error {
    color: var(--needs-input);
    font-size: 13px;
  }
</style>
