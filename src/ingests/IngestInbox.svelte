<script lang="ts">
  // The library inbox (knowledge-layer step 12), in two parts: each source in
  // Research/Ingestion/Raw/ is processed into a dated note with its proposed
  // wiki changes on Review; once the person is done, Done, file it moves the
  // source beside its note, by kind and month.
  import { onMount } from "svelte";
  import { api, type IngestStatus } from "../lib/api";
  import { app } from "../lib/app.svelte";

  let status = $state<IngestStatus | null>(null);
  let error = $state<string | null>(null);
  let starting = $state(false);

  async function refresh() {
    try {
      status = await api.ingestStatus();
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  async function readNow() {
    starting = true;
    try {
      await api.ingestNow();
      await refresh();
    } catch (e) {
      error = String(e);
    } finally {
      starting = false;
    }
  }

  onMount(() => {
    void refresh();
    const t = setInterval(() => void refresh(), 5000);
    return () => clearInterval(t);
  });
</script>

<div class="inbox">
  <h2>Ingest</h2>
  <p class="note">
    Drop a transcript, a document, an export or pasted notes in
    <span class="mono">Research/Ingestion/Raw/</span>. Ken processes each one: a dated note with the
    summary and what it overturns, and proposed changes to the wiki (pages it changes, new pages it
    calls for, rulings for their deciders), each on Review. When you are done, <strong>Done, file
    it</strong> moves the source beside its note in
    <span class="mono">Ingested/&lt;Meetings, Recordings or Documents&gt;/&lt;month&gt;/</span>.
  </p>

  {#if error}<p class="warn">{error}</p>{/if}

  {#if status && !status.hasInbox}
    <p class="note">
      This project has no <span class="mono">Research/Ingestion/Raw/</span> folder, so there is no
      inbox. A wiki made from the Ways of Working templates has one.
    </p>
  {:else if status}
    {#if !status.claudeFound}
      <p class="warn">Reading the inbox needs the Claude Code CLI. Install it and run <span class="mono">claude</span> once to log in.</p>
    {/if}
    {#if status.inReview.length > 0}
      <p class="counts"><strong>{status.inReview.length}</strong> processed, waiting on Review to be filed</p>
      <ul class="waiting">
        {#each status.inReview as w (w)}
          <li><button class="link mono" onclick={() => app.openInFiles(w)}>{w.split("/").pop()}</button></li>
        {/each}
      </ul>
    {/if}
    {#if status.waiting.length === 0}
      <p class="note">{status.inReview.length > 0 ? "Nothing new to process." : "Nothing waiting. The inbox is empty, as it should be."}</p>
    {:else}
      <p class="counts"><strong>{status.waiting.length}</strong> waiting{status.running ? " · reading now" : ""}</p>
      <ul class="waiting">
        {#each status.waiting as w (w)}
          <li><button class="link mono" onclick={() => app.openInFiles(w)}>{w.split("/").pop()}</button></li>
        {/each}
      </ul>
      <button class="btn btn-primary" disabled={starting || status.running || !status.claudeFound} onclick={readNow}>
        {status.running ? "Reading…" : "Read them now"}
      </button>
    {/if}
    <button class="btn btn-ghost" onclick={() => (app.screen = "review")}>Open Review</button>
  {/if}
</div>

<style>
  .inbox {
    padding: 22px 26px;
    max-width: 720px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    align-items: flex-start;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  .note,
  .counts {
    margin: 0;
    font-size: 13px;
    line-height: 1.55;
    color: var(--ink-secondary);
  }
  .warn {
    margin: 0;
    font-size: 13px;
    color: var(--needs-input);
  }
  .waiting {
    margin: 0;
    padding-left: 18px;
    font-size: 13px;
  }
  .link {
    background: none;
    border: none;
    padding: 0;
    color: var(--accent);
    cursor: pointer;
  }
</style>
