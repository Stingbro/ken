<script lang="ts">
  // Manage named project groups (Settings → Projects).
  //
  // Groups live in `.ken-workspace/workspace.json`, so they travel with the
  // folder — a teammate who clones the workspace gets the same grouping.
  // That is the right home because a group is a fact about the projects
  // ("these two repos are one product"), not a personal preference.
  import { onMount } from "svelte";
  import { api, type WorkspaceCandidate } from "../lib/api";
  import { app } from "../lib/app.svelte";
  import { scope } from "../lib/scope.svelte";
  import { workspaceHome } from "../lib/workspaceHome.svelte";

  // Sibling folders that aren't members yet. Before this existed the only
  // way to add a repo was the creation picker, which means closing the
  // workspace — so a repo cloned after setup could not be added at all.
  let candidates = $state<WorkspaceCandidate[]>([]);
  let ignored = $state<string[]>([]);
  let adding = $state<string | null>(null);
  let addError = $state<string | null>(null);

  onMount(() => void refreshCandidates());

  async function refreshCandidates() {
    [candidates, ignored] = await Promise.all([
      api.workspaceCandidates().catch(() => []),
      api.workspaceIgnored().catch(() => []),
    ]);
  }

  async function dismiss(folder: string) {
    ignored = await api.workspaceIgnoreCandidate(folder).catch(() => ignored);
    await refreshCandidates();
  }

  async function restore(folder: string) {
    // Fails when a hand-written glob is what hides the folder — surfacing
    // that beats silently doing nothing.
    try {
      ignored = await api.workspaceUnignoreCandidate(folder);
      addError = null;
    } catch (e) {
      addError = String(e);
    }
    await refreshCandidates();
  }

  async function addMember(folder: string) {
    adding = folder;
    addError = null;
    try {
      await api.workspaceAddMember(folder);
      await Promise.all([refreshCandidates(), workspaceHome.refresh(), app.refreshWorkspace()]);
    } catch (e) {
      addError = String(e);
    } finally {
      adding = null;
    }
  }

  let creating = $state(false);
  let name = $state("");
  let picked = $state<Record<string, boolean>>({});
  let error = $state<string | null>(null);

  // Groups are stored by parent-relative FOLDER name, which is what the
  // manifest speaks; the display name is the project's own name.
  const members = $derived(
    (app.workspace?.members ?? []).filter(
      (m) => m.status === "active" || m.status === "dormant",
    ),
  );

  function startNew() {
    creating = true;
    name = "";
    picked = {};
    error = null;
  }

  function edit(groupName: string) {
    const group = scope.groups.find((g) => g.name === groupName);
    if (!group) return;
    creating = true;
    name = group.name;
    picked = Object.fromEntries(group.members.map((m) => [m, true]));
    error = null;
  }

  async function save() {
    const chosen = Object.entries(picked)
      .filter(([, on]) => on)
      .map(([folder]) => folder);
    if (!name.trim()) {
      error = "Give the group a name.";
      return;
    }
    if (chosen.length === 0) {
      error = "Choose at least one project.";
      return;
    }
    try {
      await scope.saveGroup(name.trim(), chosen);
      creating = false;
      error = null;
    } catch (e) {
      error = String(e);
    }
  }
</script>

{#if app.workspace}
  <div class="groups">
    {#if candidates.length > 0}
      <div class="head">
        <span class="title">Add a project</span>
      </div>
      <p class="note">
        Folders next to your projects that Ken isn't tracking yet. Added
        projects start dormant and index when you first open them.
      </p>
      {#each candidates as c (c.name)}
        <div class="row">
          <span class="gname">{c.name}</span>
          <span class="gmembers">
            {c.existing ? "Known to Ken" : "New"} · {c.fileCount}
            {c.fileCount === 1 ? "file" : "files"}{#if c.markers.length > 0} · {c.markers.join(", ")}{/if}
          </span>
          <button class="link" disabled={adding === c.name} onclick={() => void addMember(c.name)}>
            {adding === c.name ? "Adding…" : "Add"}
          </button>
          <button
            class="link"
            title="Add {c.name}/ to the workspace .kenignore"
            onclick={() => void dismiss(c.name)}
          >
            Ignore
          </button>
        </div>
      {/each}
      {#if addError}<p class="err">{addError}</p>{/if}
    {/if}

    {#if ignored.length > 0}
      <div class="head">
        <span class="title">Ignored folders</span>
      </div>
      <p class="note">
        Excluded by <code>.kenignore</code> in your workspace folder — same
        syntax as <code>.gitignore</code>, so patterns like
        <code>sr-universe-*/</code> work. Edit that file directly for
        anything more involved.
      </p>
      {#each ignored as folder (folder)}
        <div class="row">
          <span class="gname dim">{folder}</span>
          <span class="gmembers"></span>
          <button class="link" onclick={() => void restore(folder)}>Undo</button>
        </div>
      {/each}
    {/if}

    <div class="head">
      <span class="title">Project groups</span>
      {#if !creating}
        <button class="link" onclick={startNew}>New group</button>
      {/if}
    </div>
    <p class="note">
      Group repos that belong together — a game and its tools, say. A group
      becomes a scope you can ask questions about on Home.
    </p>

    {#each scope.groups as g (g.name)}
      <div class="row">
        <span class="gname">{g.name}</span>
        <span class="gmembers">{g.members.join(", ") || "no resolvable projects"}</span>
        <button class="link" onclick={() => edit(g.name)}>Edit</button>
        <button class="link danger" onclick={() => void scope.deleteGroup(g.name)}>
          Delete
        </button>
      </div>
    {/each}

    {#if creating}
      <div class="editor">
        <input placeholder="Group name" bind:value={name} />
        <div class="picks">
          {#each members as m (m.name)}
            <label class="pick">
              <input type="checkbox" bind:checked={picked[m.name]} />
              <span>{m.name}</span>
            </label>
          {/each}
        </div>
        {#if error}<p class="err">{error}</p>{/if}
        <div class="actions">
          <button class="btn" onclick={() => void save()}>Save group</button>
          <button class="link" onclick={() => (creating = false)}>Cancel</button>
        </div>
      </div>
    {/if}
  </div>
{/if}

<style>
  .groups {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  .title {
    font-weight: 600;
    font-size: 13px;
  }
  .note {
    margin: 0 0 4px;
    font-size: 12px;
    color: var(--ink-tertiary);
    line-height: 1.5;
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 6px 0;
    border-bottom: 1px solid var(--rule-line);
    font-size: 13px;
  }
  .gname {
    font-weight: 600;
    flex: none;
  }
  .gname.dim {
    font-weight: 500;
    color: var(--ink-tertiary);
  }
  .gmembers {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    color: var(--ink-tertiary);
    overflow-wrap: anywhere;
  }
  .editor {
    margin-top: 8px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-card, 10px);
    background: var(--surface);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .picks {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }
  .pick {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    cursor: pointer;
  }
  .pick input {
    flex: none;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .link {
    background: transparent;
    border: none;
    padding: 0;
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-tertiary);
    cursor: pointer;
  }
  .link:hover {
    color: var(--ink-secondary);
  }
  .link.danger:hover {
    color: var(--danger);
  }
  .err {
    margin: 0;
    font-size: 12px;
    color: var(--danger);
  }
</style>
