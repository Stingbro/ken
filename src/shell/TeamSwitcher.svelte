<script lang="ts">
  // The title bar's team switcher (the Wright white box's top bar): Ken
  // is about one team at a time. Each team is listed on its own; there is no
  // "all teams". Choosing one scopes questions, search and Home to it; its
  // repos are picked inside Files.
  import { app } from "../lib/app.svelte";
  import { scope } from "../lib/scope.svelte";

  let { close }: { close: () => void } = $props();

  async function pick(name: string) {
    close();
    await scope.selectTeam(name);
  }
</script>

<button class="scrim" onclick={close} aria-label="Close team switcher"></button>
<div class="menu" role="menu">
  {#if scope.groups.length === 0}
    <div class="empty">
      {app.workspace?.name ?? "This workspace"} has no team yet. Set-up puts each repo on a team.
    </div>
  {:else}
    {#each scope.groups as team (team.name)}
      <button class="row" class:current={team.name === scope.team} role="menuitem" onclick={() => pick(team.name)}>
        <span class="badge">{team.name.charAt(0).toUpperCase()}</span>
        <span class="info">
          <span class="name">{team.name}</span>
          <span class="sub">{team.projectIds.length} {team.projectIds.length === 1 ? "repo" : "repos"}</span>
        </span>
      </button>
    {/each}
  {/if}
  <div class="sep"></div>
  <button class="row foot" role="menuitem" onclick={() => { close(); app.openTeam(); }}>
    <span class="info"><span class="name">Team library</span><span class="sub">Folders, rules and the weekly check</span></span>
  </button>
</div>

<style>
  .sep {
    height: 1px;
    background: var(--line-soft);
    margin: 4px 2px;
  }
  .scrim {
    position: fixed;
    inset: 0;
    background: transparent;
    border: none;
    z-index: 39;
  }
  .menu {
    position: fixed;
    top: 64px;
    left: 10px;
    width: 280px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-overlay);
    box-shadow: var(--shadow-overlay);
    padding: 6px;
    z-index: 40;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: 9px;
    border: none;
    background: transparent;
    text-align: left;
    color: var(--ink);
  }
  .row:hover {
    background: var(--sunken);
  }
  .row.current {
    background: color-mix(in srgb, var(--accent) 8%, transparent);
  }
  .badge {
    width: 26px;
    height: 26px;
    border-radius: 7px;
    background: var(--ink);
    color: var(--paper);
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: var(--font-serif);
    font-size: 13px;
    flex: none;
  }
  .info {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    font-size: 13px;
    font-weight: 600;
  }
  .sub {
    font-size: 11.5px;
    color: var(--ink-tertiary);
  }
  .empty {
    padding: 10px;
    font-size: 12.5px;
    color: var(--ink-secondary, var(--ink));
  }
</style>
