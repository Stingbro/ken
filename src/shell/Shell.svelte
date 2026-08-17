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
  import ProjectScopeBar from "./ProjectScopeBar.svelte";
  import { SvelteSet } from "svelte/reactivity";

  // Screens that describe ONE project, and therefore carry the project
  // selector. Home is the whole Ken setup's front page and Settings spans
  // app-wide preferences — neither is project-scoped, so neither shows it.
  //
  // `tasks` is deliberately ABSENT: the board is workspace-global by
  // construction (`task_homes_scan` walks every member's `.ken/tasks/`)
  // and already carries its own "All projects" filter. A focus selector
  // above it would imply the board shows one project, and changing it
  // would move the focus without changing a single card — exactly the
  // false framing this bar exists to remove.
  const PROJECT_SCOPED = new Set([
    "files",
    "review",
    "ingests",
    "map",
    "timeline",
    "record",
  ]);

  // Screens the user has actually opened this session (home is always live).
  const visited = new SvelteSet<string>();
  $effect(() => {
    if (app.screen !== "home") visited.add(app.screen);
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
    <div class="stack">
      {#if PROJECT_SCOPED.has(app.screen)}
        <ProjectScopeBar />
      {/if}
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
    </div>
    {#if chats.open}
      <ChatDrawer />
    {/if}
  </div>
  {#if app.searchOpen}
    <SearchOverlay />
  {/if}
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
  /* Column so the scope bar sits above the screen without either of them
     losing their own scrolling. */
  .stack {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
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
