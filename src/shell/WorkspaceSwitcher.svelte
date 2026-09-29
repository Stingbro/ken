<script lang="ts">
  // Nav-rail workspace switcher (workspace task 4.3): workspace name + the
  // member roster with status dots. Click (or `Ctrl+P` — wired in
  // Shell.svelte, cycling through `app.cycleFocusedMember()`) calls
  // `app.focusMember`, which routes through `focus_project` and reloads the
  // focused member's per-project caches (`app.svelte.ts`'s
  // `loadFocusedMemberState`). Styled as a popover anchored under its
  // nav-rail trigger, the same pattern `ProjectSwitcher.svelte` uses for the
  // title-bar project menu.
  import { onMount } from "svelte";
  import { app, type MemberInfo } from "../lib/app.svelte";
  import { api, memberLeaf, type RecentWorkspace } from "../lib/api";
  import { timeAgo } from "../lib/format";

  let { close }: { close: () => void } = $props();

  // The other workspaces opened recently: switch to one without going back
  // to the start screen.
  let others = $state<RecentWorkspace[]>([]);
  onMount(() => {
    api.listRecentWorkspaces().then(
      (list) => (others = list.filter((w) => w.available && w.path !== app.workspace?.root).slice(0, 5)),
      () => (others = []),
    );
  });

  async function switchTo(ws: RecentWorkspace) {
    close();
    await app.closeWorkspaceSession();
    await app.openWorkspace(ws.path);
  }

  function statusLabel(status: MemberInfo["status"]): string {
    switch (status) {
      case "active":
        return "open";
      case "dormant":
        return "dormant";
      case "missing":
        return "missing";
      case "invalid":
        return "invalid";
    }
  }

  function statusTitle(m: MemberInfo): string {
    switch (m.status) {
      case "active":
        return "Open and indexed";
      case "dormant":
        return "Known, not currently loaded — click to open";
      case "missing":
        return "Folder not found — was it moved or deleted?";
      case "invalid":
        return "Couldn't be read as a Ken project";
    }
  }

  async function pick(m: MemberInfo) {
    if (!m.id || m.status === "missing" || m.status === "invalid") return;
    await app.focusMember(m.id);
    close();
  }

  async function closeWorkspace() {
    await app.closeWorkspaceSession();
    close();
  }
</script>

<button class="scrim" onclick={close} aria-label="Close workspace switcher"></button>
<div class="menu">
  <div class="head">
    <span class="ws-name" title={app.workspace?.root}>{app.workspace?.name}</span>
    <button class="close-ws" onclick={closeWorkspace} title="Close this workspace">
      Close
    </button>
  </div>
  {#each app.members as m (m.id ?? m.name)}
    <button
      class="row"
      class:current={m.id !== null && m.id === app.focused}
      class:unavailable={m.status === "missing" || m.status === "invalid"}
      onclick={() => pick(m)}
      title={statusTitle(m)}
    >
      <span class="dot {m.status}"></span>
      <span class="name">{memberLeaf(m.name)}</span>
      {#if m.status !== "active"}
        <span class="status-label">{statusLabel(m.status)}</span>
      {/if}
    </button>
  {/each}
  {#if others.length > 0}
    <div class="recent-head">Recent workspaces</div>
    {#each others as ws (ws.id)}
      <button class="row" onclick={() => switchTo(ws)} title={ws.path}>
        <span class="name">{ws.name}</span>
        <span class="status-label">{timeAgo(ws.openedAt)}</span>
      </button>
    {/each}
  {/if}
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: transparent;
    border: none;
    z-index: 39;
  }
  .menu {
    position: fixed;
    top: 60px;
    left: 70px;
    width: 250px;
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
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px 8px;
  }
  .ws-name {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    color: var(--ink-tertiary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .close-ws {
    flex: none;
    border: none;
    background: transparent;
    padding: 2px 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--ink-tertiary);
  }
  .close-ws:hover {
    color: var(--danger);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 8px 10px;
    border-radius: 9px;
    border: none;
    background: transparent;
    text-align: left;
    font-size: 13px;
    color: var(--ink);
  }
  .row:hover {
    background: var(--sunken);
  }
  .row.current {
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    font-weight: 600;
  }
  .row.unavailable {
    opacity: 0.6;
    cursor: default;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 4px;
    flex: none;
    background: var(--ink-tertiary);
  }
  .dot.active {
    background: var(--healthy);
  }
  .dot.missing,
  .dot.invalid {
    background: var(--danger);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .recent-head {
    margin-top: 6px;
    padding: 8px 10px 4px;
    border-top: 1px solid var(--border);
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.03em;
    text-transform: uppercase;
    color: var(--ink-tertiary);
  }
  .status-label {
    flex: none;
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--ink-tertiary);
  }
</style>
