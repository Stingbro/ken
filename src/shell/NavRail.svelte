<script lang="ts">
  import type { Component } from "svelte";
  import { app, type Screen } from "../lib/app.svelte";
  import { families } from "../lib/families.svelte";
  import { rail } from "../lib/rail.svelte";
  import LayoutGrid from "@lucide/svelte/icons/layout-grid";
  import Files from "@lucide/svelte/icons/files";
  import Layers from "@lucide/svelte/icons/layers";
  import UsersRound from "@lucide/svelte/icons/users-round";
  import Network from "@lucide/svelte/icons/network";
  import Clock from "@lucide/svelte/icons/clock";
  import Settings from "@lucide/svelte/icons/settings";

  // The team inbox lives on Team; its unread messages count on Team's item,
  // with what the wiki's checks found.
  void families.init();
  const teamCount = $derived(families.totalUnread + rail.findings);
  const teamTitle = $derived(
    [
      families.totalUnread > 0 ? `${families.totalUnread} unread in the team inbox` : "",
      rail.findings > 0 ? `${rail.findings} ${rail.findings === 1 ? "finding" : "findings"} from the wiki's checks` : "",
    ]
      .filter((s) => s)
      .join(" · "),
  );

  const items: { key: Screen; icon: Component; label: string }[] = [
    { key: "home", icon: LayoutGrid, label: "Home" },
    { key: "files", icon: Files, label: "Files" },
    { key: "ingests", icon: Layers, label: "Ingest" },
    { key: "team", icon: UsersRound, label: "Team" },
    { key: "map", icon: Network, label: "Map" },
    { key: "timeline", icon: Clock, label: "Timeline" },
  ];
</script>

<nav>
  <!-- No project switcher here. It lives in the title bar
       (`ProjectSwitcher.svelte`), which is where people reach for it and
       which also owns rename/forget, open-a-folder, and the "All
       projects" merged Files tree. A second selector in the rail framed
       the whole app as being IN one project, which is what made Home read
       as project-specific. Ctrl+P still cycles members. -->
  {#each items as item (item.key)}
    {@const Icon = item.icon}
    <button
      class:active={app.screen === item.key}
      onclick={() => (app.screen = item.key)}
      title={item.label}
    >
      <span class="icon"><Icon size={16} strokeWidth={1.75} /></span>{item.label}
      {#if item.key === "files" && app.failedFiles.length > 0}
        <span class="dot" title="{app.failedFiles.length} {app.failedFiles.length === 1 ? 'file' : 'files'} Ken could not read"></span>
      {:else if item.key === "files" && app.unread.length > 0}
        <span class="dot unread" title="{app.unread.length} files changed since you last looked"></span>
      {/if}
      {#if item.key === "ingests" && rail.ingestWaiting > 0}
        <span class="count" title="{rail.ingestWaiting} {rail.ingestWaiting === 1 ? 'source has' : 'sources have'} something waiting">{rail.ingestWaiting}</span>
      {/if}
      {#if item.key === "team" && teamCount > 0}
        <span class="count" title={teamTitle}>{teamCount}</span>
      {/if}
    </button>
  {/each}
  <button
    class="settings"
    class:active={app.screen === "settings"}
    onclick={() => (app.screen = "settings")}
    title="Settings"
  >
    <span class="icon"><Settings size={16} strokeWidth={1.75} /></span>Settings
  </button>
</nav>

<style>
  nav {
    width: 64px;
    flex: none;
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 14px 0;
    gap: 6px;
  }
  button {
    width: 44px;
    height: 42px;
    border-radius: 10px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    color: var(--ink-tertiary);
    font-size: 9px;
    position: relative;
    border: none;
    background: transparent;
  }
  button:hover {
    background: var(--sunken);
    color: var(--ink-secondary);
  }
  button.active {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent-deep);
    font-weight: 600;
  }
  .icon {
    font-size: 14px;
  }
  .dot {
    position: absolute;
    top: 3px;
    right: 5px;
    width: 7px;
    height: 7px;
    border-radius: 4px;
    background: var(--danger);
    border: 1.5px solid var(--paper);
  }
  /* Unread files are informational, not a problem — accent, not the red the
     failed-index dot uses. Failed takes precedence when both apply. */
  .dot.unread {
    background: var(--accent);
  }
  .count {
    position: absolute;
    top: 2px;
    right: 2px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 15px;
    height: 15px;
    padding: 0 3px;
    border-radius: 8px;
    background: var(--danger);
    color: var(--surface);
    font-size: 9px;
    font-weight: 700;
    border: 1.5px solid var(--paper);
  }
  .settings {
    margin-top: auto;
  }
</style>
