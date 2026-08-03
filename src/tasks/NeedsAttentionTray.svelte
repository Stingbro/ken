<script lang="ts">
  // ken-tasks task 4.2: the needs-attention tray — tasks a hand edit or
  // agent error left with an out-of-vocabulary status/kind/board, or a
  // `goal:` id matching no goal file (design D7). These are deliberately
  // never auto-rewritten (spec: "the file is not rewritten"), so the tray
  // is read-only except for the same "open the file to fix it by hand" path
  // every other card offers.
  import { tasksStore } from "../lib/tasks.svelte";
  import type { AttentionReason } from "../lib/api";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";

  function describe(r: AttentionReason): string {
    switch (r.reason) {
      case "invalidStatus":
        return `status: "${r.value}" isn't a recognized column`;
      case "invalidKind":
        return `kind: "${r.value}" isn't human/ai`;
      case "invalidBoard":
        return `board: "${r.value}" isn't main/daily`;
      case "unknownGoal":
        return `goal: "${r.value}" doesn't match any goal file`;
    }
  }

  function taskFor(id: string) {
    return tasksStore.board.tasks.find((t) => t.id === id) ?? null;
  }
</script>

<div class="tray">
  <div class="tray-head">
    <TriangleAlert size={14} strokeWidth={1.75} />
    <span>Needs attention</span>
    <span class="count">{tasksStore.board.needsAttention.length}</span>
  </div>
  <p class="note">
    Ken never rewrites a file it doesn't understand — fix these by hand, then
    they'll drop off this list on their own.
  </p>
  {#each tasksStore.board.needsAttention as item (item.id)}
    {@const task = taskFor(item.id)}
    {@const reason = task ? tasksStore.openReason(task) : "Task file no longer on the board."}
    <button
      class="item"
      class:disabled-link={!!reason}
      title={reason ?? "Open task file"}
      onclick={() => task && void tasksStore.openFile(task)}
    >
      <div class="item-title">{item.title || "Untitled task"}</div>
      <div class="reasons">
        {#each item.reasons as r, i (i)}
          <span class="reason">{describe(r)}</span>
        {/each}
      </div>
    </button>
  {:else}
    <p class="note empty">Nothing needs attention.</p>
  {/each}
</div>

<style>
  .tray {
    width: 280px;
    flex: none;
    border-left: 1px solid var(--border);
    padding: 14px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .tray-head {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 600;
    color: var(--needs-input-text);
  }
  .count {
    margin-left: auto;
    font-size: 11px;
    font-weight: 700;
    color: var(--surface);
    background: var(--needs-input);
    border-radius: 999px;
    padding: 1px 7px;
  }
  .note {
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--ink-tertiary);
    margin: 0;
  }
  .note.empty {
    padding: 8px 0;
  }
  .item {
    display: flex;
    flex-direction: column;
    gap: 4px;
    text-align: left;
    padding: 9px 10px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface);
  }
  .item:hover {
    background: var(--sunken);
  }
  .item.disabled-link {
    cursor: default;
  }
  .item-title {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--ink);
  }
  .reasons {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .reason {
    font-size: 11px;
    color: var(--needs-input-text);
  }
</style>
