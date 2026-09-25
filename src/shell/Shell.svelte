<script lang="ts">
  import { app } from "../lib/app.svelte";
  import TitleBar from "./TitleBar.svelte";
  import NavRail from "./NavRail.svelte";
  import SearchOverlay from "../search/SearchOverlay.svelte";
  import ChatDrawer from "../chat/ChatDrawer.svelte";
  import { chats } from "../lib/chats.svelte";
  import HomeScreen from "../screens/HomeScreen.svelte";
  import FilesScreen from "../screens/FilesScreen.svelte";
  import ReviewScreen from "../screens/ReviewScreen.svelte";
  import IngestsScreen from "../screens/IngestsScreen.svelte";
  import TasksScreen from "../screens/TasksScreen.svelte";
  import MapScreen from "../screens/MapScreen.svelte";
  import TimelineScreen from "../screens/TimelineScreen.svelte";
  import RecordScreen from "../screens/RecordScreen.svelte";
  import SettingsScreen from "../screens/SettingsScreen.svelte";
  import WhatsNewDialog from "../whats-new/WhatsNewDialog.svelte";
  import { whatsNew } from "../whats-new/whatsNew.svelte";
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { SvelteSet } from "svelte/reactivity";

  // Screens the user has actually opened this session (home is always live).
  const visited = new SvelteSet<string>();
  $effect(() => {
    if (app.screen !== "home") visited.add(app.screen);
  });

  // The shell only renders with a project open, so the release notes never
  // interrupt onboarding.
  onMount(() => whatsNew.init());

  // The person asked Claude in chat to open something: focus its workspace
  // member if it is another one, then open it at the line or heading.
  onMount(() => {
    const off = api.onKenOpen(async (req) => {
      if (req.projectId && req.projectId !== app.focused && app.workspace?.members.some((m) => m.projectId === req.projectId)) {
        await app.focusMember(req.projectId);
      }
      app.openAt(req.path, { line: req.line ?? undefined, anchor: req.anchor ?? undefined });
    });
    return () => void off.then((u) => u());
  });

  function onKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      app.searchOpen = !app.searchOpen;
    } else if (e.key === "Escape" && app.searchOpen) {
      app.searchOpen = false;
    } else if (e.ctrlKey && !e.metaKey && e.key.toLowerCase() === "p" && app.workspace) {
      // Cycle the focused workspace member (workspace task 4.3).
      e.preventDefault();
      void app.cycleFocusedMember();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="frame">
  <TitleBar />
  <div class="body">
    <NavRail />
    <main class="screen">
      <!-- Screens mount on first visit and then stay mounted, so open file /
           query survive switching — without paying for every screen (and the
           restored editor tab) at boot. -->
      <div class="pane" hidden={app.screen !== "home"}><HomeScreen /></div>
      {#if visited.has("files")}
        <div class="pane" hidden={app.screen !== "files"}><FilesScreen /></div>
      {/if}
      {#if visited.has("review")}
        <div class="pane" hidden={app.screen !== "review"}><ReviewScreen /></div>
      {/if}
      {#if visited.has("ingests")}
        <div class="pane" hidden={app.screen !== "ingests"}><IngestsScreen /></div>
      {/if}
      {#if visited.has("tasks")}
        <div class="pane" hidden={app.screen !== "tasks"}><TasksScreen /></div>
      {/if}
      {#if visited.has("map")}
        <div class="pane" hidden={app.screen !== "map"}><MapScreen /></div>
      {/if}
      {#if visited.has("timeline")}
        <div class="pane" hidden={app.screen !== "timeline"}><TimelineScreen /></div>
      {/if}
      {#if visited.has("record")}
        <div class="pane" hidden={app.screen !== "record"}><RecordScreen /></div>
      {/if}
      {#if visited.has("settings")}
        <div class="pane" hidden={app.screen !== "settings"}><SettingsScreen /></div>
      {/if}
    </main>
    {#if chats.open}
      <ChatDrawer />
    {/if}
  </div>
  {#if app.searchOpen}
    <SearchOverlay />
  {/if}
  <WhatsNewDialog />
</div>

<style>
  .frame {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--paper);
    position: relative;
    overflow: hidden;
  }
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .screen {
    flex: 1;
    min-width: 0;
    display: flex;
    min-height: 0;
  }
  .pane {
    flex: 1;
    min-width: 0;
    display: flex;
    min-height: 0;
  }
  .pane[hidden] {
    display: none;
  }
</style>
