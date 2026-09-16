<script lang="ts">
  import { whatsNew } from "./whatsNew.svelte";
  import { renderMarkdown } from "../lib/markdown";

  let gotIt: HTMLButtonElement | undefined = $state();

  // Item bodies are one line of prose: render them through the shared
  // (sanitizing) markdown pipeline so code spans and links work, then drop the
  // block wrapper so the text flows after the bold key.
  function inline(md: string): string {
    return renderMarkdown(md)
      .trim()
      .replace(/^<p>/, "")
      .replace(/<\/p>$/, "");
  }

  function onKeydown(e: KeyboardEvent) {
    if (whatsNew.open && e.key === "Escape") {
      e.preventDefault();
      whatsNew.dismiss();
    }
  }

  // Focus the dismiss button as the dialog appears: Esc/Enter then both work
  // without hunting for the mouse.
  $effect(() => {
    if (whatsNew.open) gotIt?.focus();
  });
</script>

<svelte:window onkeydown={onKeydown} />

{#if whatsNew.open}
  <button class="scrim" onclick={() => whatsNew.dismiss()} aria-label="Close what's new"
  ></button>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="whats-new-title">
    <div class="head">
      <h2 id="whats-new-title">What's new in Ken {whatsNew.version}</h2>
      {#if whatsNew.showingAll}
        <div class="caption">
          No notes for this build yet — here is everything from earlier releases.
        </div>
      {:else if whatsNew.releases.length === 1 && whatsNew.releases[0]?.date}
        <div class="caption">{whatsNew.releases[0].date}</div>
      {/if}
    </div>

    <div class="body">
      {#each whatsNew.releases as release (release.version)}
        {#if whatsNew.releases.length > 1}
          <div class="release-head">
            <span class="release-version">{release.version}</span>
            {#if release.date}<span class="release-date">{release.date}</span>{/if}
          </div>
        {/if}
        {#each release.sections as section, i (section.title ?? i)}
          {#if section.title}
            <div class="section">{section.title}</div>
          {/if}
          <ul>
            {#each section.items as item, j (j)}
              <li>
                {#if item.key}<strong>{item.key}</strong>: {/if}<!--
                -->{@html inline(item.body)}
              </li>
            {/each}
          </ul>
        {/each}
      {/each}
    </div>

    <div class="foot">
      <button class="got-it" bind:this={gotIt} onclick={() => whatsNew.dismiss()}>
        Got it
      </button>
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: absolute;
    inset: 0;
    background: var(--scrim);
    border: none;
    z-index: 60;
  }
  .dialog {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(560px, calc(100vw - 80px));
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-overlay);
    box-shadow: var(--shadow-overlay);
    overflow: hidden;
    z-index: 61;
  }
  .head {
    flex: none;
    padding: 24px 28px 14px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  h2 {
    margin: 0;
    font-family: var(--font-serif);
    font-size: 24px;
    font-weight: 500;
    letter-spacing: -0.01em;
    color: var(--ink);
  }
  .caption {
    font-size: 12px;
    color: var(--ink-tertiary);
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 28px 8px;
  }
  .release-head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 18px 0 2px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }
  .release-head:first-child {
    margin-top: 0;
    padding-top: 0;
    border-top: none;
  }
  .release-version {
    font-family: var(--font-serif);
    font-size: 16px;
    font-weight: 600;
  }
  .release-date {
    font-size: 11.5px;
    color: var(--ink-tertiary);
  }
  .section {
    margin-top: 16px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--ink-tertiary);
  }
  ul {
    margin: 8px 0 0;
    padding-left: 18px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  li {
    font-size: 13.5px;
    line-height: 1.6;
    color: var(--ink-secondary);
  }
  li strong {
    color: var(--ink);
    font-weight: 600;
  }
  li :global(code) {
    font-family: var(--font-mono);
    font-size: 12px;
    background: var(--sunken);
    border-radius: 4px;
    padding: 1px 4px;
  }
  li :global(a) {
    color: var(--accent);
  }
  .foot {
    flex: none;
    display: flex;
    justify-content: flex-end;
    padding: 14px 28px 20px;
  }
  .got-it {
    height: 34px;
    padding: 0 18px;
    border: 1px solid var(--accent-deep);
    border-radius: var(--radius-control);
    background: var(--accent);
    color: #fdfcfa;
    font-family: inherit;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }
  .got-it:hover {
    background: var(--accent-hover);
  }
</style>
