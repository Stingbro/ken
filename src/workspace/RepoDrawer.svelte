<script lang="ts">
  // One repo's settings, opened from its row on the workspace settings
  // screen: what it is for, reindex, the folders Ken watches,
  // cloud files, video transcription and sync (the knowledge base only). The
  // repo is focused before this opens; every card acts on the focused repo.
  import { onMount, untrack } from "svelte";
  import { app } from "../lib/app.svelte";
  import { api, memberLeaf, type SourceRow, type SyncStatus } from "../lib/api";
  import { toast } from "../lib/toast.svelte";
  import {
    buildFolderTree,
    folderTriState,
    isExcluded,
    toggleFolder as toggleFolderPaths,
    type FolderNode,
  } from "../lib/folderTree";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import X from "@lucide/svelte/icons/x";

  let {
    repo,
    close,
    changed,
    knowledgeBase = false,
  }: { repo: SourceRow; close: () => void; changed?: () => void; knowledgeBase?: boolean } = $props();

  // What the repo is for, in a person's words: Ken reads it when it decides
  // where something belongs.
  let description = $state(untrack(() => repo.description));
  let savingDescription = $state(false);
  async function saveDescription() {
    if (description.trim() === repo.description.trim()) return;
    savingDescription = true;
    try {
      await api.setProjectDescription(repo.id, description.trim());
      changed?.();
    } catch (e) {
      toast.error("Could not save what the repo is for", e);
    } finally {
      savingDescription = false;
    }
  }

  let busy = $state(false);
  let toggling = $state(false);
  let sync = $state<SyncStatus | null>(null);
  let syncingNow = $state(false);

  const focusedHere = $derived(app.focused === repo.id);
  const excludedSet = $derived(new Set(app.project?.excluded ?? []));
  const folderTree = $derived(buildFolderTree(app.folders));
  let expanded = $state<Set<string>>(new Set());

  onMount(() => {
    void api.syncStatus().then((s) => (sync = s)).catch(() => (sync = null));
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") close();
  }

  async function reindex() {
    busy = true;
    try {
      await app.reindex();
    } catch (e) {
      toast.error("Could not reindex", e);
    } finally {
      busy = false;
    }
  }

  function toggleExpand(relPath: string) {
    const next = new Set(expanded);
    if (next.has(relPath)) next.delete(relPath);
    else next.add(relPath);
    expanded = next;
  }

  async function toggleFolder(relPath: string) {
    if (!app.project || toggling) return;
    toggling = true;
    try {
      const next = toggleFolderPaths(relPath, isExcluded(relPath, excludedSet), excludedSet);
      await app.setExcluded(next);
    } catch (e) {
      toast.error("Could not change the watched folders", e);
    } finally {
      toggling = false;
    }
  }

  async function toggleSyncAuto() {
    if (!sync) return;
    try {
      sync = await api.setSyncAuto(!sync.auto);
      changed?.();
    } catch (e) {
      toast.error("Could not change sync", e);
    }
  }

  async function syncNow() {
    syncingNow = true;
    try {
      await api.syncNow();
    } catch (e) {
      toast.error("Could not sync", e);
    } finally {
      // Live progress shows on the title-bar dot.
      setTimeout(() => (syncingNow = false), 1200);
    }
  }

  function openInFiles() {
    close();
    app.screen = "files";
  }
</script>

<svelte:window onkeydown={onKey} />

<button class="scrim" aria-label="Close" onclick={close}></button>
<aside class="drawer" aria-label="{memberLeaf(repo.name)} settings">
  <div class="head">
    <div class="title">
      <span class="name">{memberLeaf(repo.name)}</span>
      <span class="path mono" title={repo.path}>{repo.path}</span>
    </div>
    <button class="close" aria-label="Close" title="Close (Esc)" onclick={close}><X size={15} strokeWidth={1.75} /></button>
  </div>

  <div class="card">
    <div class="card-title">What it is for</div>
    <textarea
      rows="3"
      bind:value={description}
      placeholder="What the repo holds and who uses it"
      aria-label="What it is for"
      onblur={() => void saveDescription()}
    ></textarea>
    <div class="row">
      <span class="note">Ken reads this when it decides where something belongs.</span>
      <button
        class="btn btn-small push"
        disabled={savingDescription || description.trim() === repo.description.trim()}
        onclick={() => void saveDescription()}>{savingDescription ? "Saving…" : "Save"}</button
      >
    </div>
  </div>

  {#if !focusedHere}
    <p class="note">Opening {memberLeaf(repo.name)}…</p>
  {:else}
    <div class="card">
      <div class="row">
        <span class="label">Index</span>
        <span>{app.files.length} files{#if app.failedFiles.length} · {app.failedFiles.length} Ken could not read{/if}</span>
        <button class="btn btn-small push" onclick={reindex} disabled={busy}>{busy ? "Rebuilding…" : "Reindex"}</button>
      </div>
      <p class="note">Reindex rebuilds Ken's index of this repo. It never changes the files.</p>
      <div class="row">
        <button class="btn btn-small btn-ghost" onclick={openInFiles}>Open in Files</button>
      </div>
    </div>

    <div class="card">
      <div class="card-title">Watched folders</div>
      <p class="note">Ken watches every folder. Untick one to leave it, and everything in it, out of search.</p>
      {#if app.folders.length === 0}
        <p class="note">No subfolders.</p>
      {:else}
        <div class="folder-tree">
          {#each folderTree as node (node.relPath)}
            {@render folderRow(node, 0)}
          {/each}
        </div>
      {/if}
    </div>

    {#snippet folderRow(node: FolderNode, depth: number)}
      {@const tri = folderTriState(node.relPath, excludedSet)}
      <div class="frow" style:padding-left={`${depth * 18}px`}>
        {#if node.children.length > 0}
          <button
            class="chev"
            class:open={expanded.has(node.relPath)}
            aria-label={expanded.has(node.relPath) ? "Collapse" : "Expand"}
            onclick={() => toggleExpand(node.relPath)}
          >
            <ChevronRight size={14} strokeWidth={2} />
          </button>
        {:else}
          <span class="chev-spacer"></span>
        {/if}
        <label class="fcheck">
          <input
            type="checkbox"
            checked={tri === "checked"}
            indeterminate={tri === "indeterminate"}
            disabled={toggling}
            onchange={() => toggleFolder(node.relPath)}
          />
          <span class="mono">{node.name}</span>
        </label>
      </div>
      {#if expanded.has(node.relPath)}
        {#each node.children as child (child.relPath)}
          {@render folderRow(child, depth + 1)}
        {/each}
      {/if}
    {/snippet}

    <div class="card">
      <div class="card-title">Cloud files</div>
      <label class="check">
        <input type="checkbox" checked={app.backgroundIndex} onchange={(e) => void app.setBackgroundIndex(e.currentTarget.checked)} />
        Index cloud files in the background
      </label>
      <p class="note">Downloads online-only documents so they are searchable without opening them. Large media still download on open.</p>
    </div>

    <div class="card">
      <div class="card-title">Video transcription</div>
      <label class="check">
        <input
          type="checkbox"
          checked={app.transcribeVideosOnIndex}
          onchange={(e) => void app.setTranscribeVideosOnIndex(e.currentTarget.checked)}
        />
        Transcribe videos while indexing
      </label>
      <p class="note">Speech to text on this computer, so what is said in a video is searchable. Off by default: it is slow. One video can be transcribed from its player.</p>
    </div>

    {#if knowledgeBase && sync?.mode === "git" && sync.remote}
      <div class="card">
        <div class="card-title">Sync</div>
        <div class="row">
          <label class="check">
            <input type="checkbox" checked={sync.auto} onchange={() => void toggleSyncAuto()} />
            Keep this repo in sync
          </label>
          <button class="btn btn-small push" onclick={() => void syncNow()} disabled={!sync.active || syncingNow}>
            {syncingNow ? "Syncing…" : "Sync now"}
          </button>
        </div>
      </div>
    {/if}
  {/if}
</aside>

<style>
  .scrim {
    position: fixed;
    inset: 52px 0 0 0;
    background: var(--scrim);
    border: none;
    z-index: 34;
  }
  .drawer {
    position: fixed;
    top: 52px;
    right: 0;
    bottom: 0;
    width: min(440px, 100vw);
    z-index: 35;
    background: var(--paper);
    border-left: 1px solid var(--border);
    box-shadow: var(--shadow-drawer);
    overflow-y: auto;
    padding: 16px 18px 28px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .head {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }
  .title {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .name {
    font-family: var(--font-serif);
    font-size: 19px;
    font-weight: 500;
  }
  .path {
    font-size: 11px;
    color: var(--ink-tertiary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .close {
    display: inline-flex;
    border: none;
    background: transparent;
    padding: 4px;
    border-radius: 6px;
    color: var(--ink-tertiary);
  }
  .close:hover {
    background: var(--sunken);
    color: var(--ink);
  }
  .card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 9px;
  }
  .card-title {
    font-size: 13.5px;
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
  }
  .label {
    width: 48px;
    flex: none;
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-secondary);
  }
  .push {
    margin-left: auto;
  }
  .note {
    margin: 0;
    font-size: 12px;
    line-height: 1.55;
    color: var(--ink-tertiary);
  }
  textarea {
    font: inherit;
    font-size: 12.5px;
    line-height: 1.5;
    padding: 6px 8px;
    border-radius: 7px;
    border: 1px solid var(--border-strong);
    background: var(--surface);
    color: var(--ink);
    resize: vertical;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    cursor: pointer;
  }
  .check input,
  .fcheck input {
    accent-color: var(--accent);
  }
  .folder-tree {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-height: 320px;
    overflow-y: auto;
  }
  .frow {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .chev {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border: none;
    background: transparent;
    color: var(--ink-tertiary);
    border-radius: 4px;
    transition: transform 0.15s ease;
  }
  .chev:hover {
    background: var(--sunken);
    color: var(--ink);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .chev-spacer {
    width: 18px;
    flex: none;
  }
  .fcheck {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    cursor: pointer;
  }
</style>
