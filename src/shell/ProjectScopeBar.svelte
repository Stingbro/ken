<script lang="ts">
  // Per-tab project selection (ken-home-workspace 3.6).
  //
  // Project selection used to live in the nav rail, which framed "which
  // project" as a global mode — the same framing that made Home look
  // project-specific. It belongs to the project-scoped SCREENS instead:
  // Home and Settings describe the whole Ken setup and never show this.
  //
  // Selecting here still moves the one workspace focus (design D7) rather
  // than giving each tab its own project. Independent per-tab projects
  // would leave Files on one project while Tasks showed another, with no
  // answer to "which project am I in" — the bar shows the focus every
  // project-scoped screen is reading, and lets you change it in place.
  import { app } from "../lib/app.svelte";
  import Boxes from "@lucide/svelte/icons/boxes";

  const members = $derived(
    (app.workspace?.members ?? []).filter(
      (m) => m.projectId && (m.status === "active" || m.status === "dormant"),
    ),
  );

  // Bound rather than a plain `value=` attribute — see `ScopePicker`: a
  // plain attribute is applied before the options mount, leaving the
  // control showing nothing.
  //
  // No "All" entry here on purpose: this is which project the SCREEN is
  // showing, and Files or Map can only show one. "All" is a question
  // scope, and lives on Home.
  let sel = $state("");

  $effect(() => {
    sel = app.workspace?.focused ?? "";
  });

  async function pick() {
    if (!sel || sel === app.workspace?.focused) return;
    await app.focusMember(sel);
  }
</script>

{#if app.workspace && members.length > 0}
  <div class="bar">
    <span class="icon"><Boxes size={14} strokeWidth={1.75} /></span>
    <select
      bind:value={sel}
      onchange={pick}
      title="Which project this screen is showing (Ctrl+P cycles)"
      aria-label="Project"
    >
      {#each members as m (m.projectId)}
        <option value={m.projectId}>{m.name}</option>
      {/each}
    </select>
    {#if members.length > 1}
      <span class="hint">{members.length} projects</span>
    {/if}
  </div>
{/if}

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 14px;
    border-bottom: 1px solid var(--rule-line);
    background: var(--paper);
    flex: none;
  }
  .icon {
    display: inline-flex;
    color: var(--ink-tertiary);
    flex: none;
  }
  select {
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-control, 6px);
    padding: 2px 4px;
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    color: var(--ink);
    cursor: pointer;
    max-width: 220px;
  }
  select:hover {
    border-color: var(--border);
    background: var(--surface);
  }
  .hint {
    font-size: 11px;
    color: var(--ink-tertiary);
  }
</style>
