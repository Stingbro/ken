<script lang="ts">
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api, type FeatureInfo, type RegistryEntryStatus } from "../lib/api";
  import { app } from "../lib/app.svelte";
  import KenMark from "../lib/ui/KenMark.svelte";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import ContextMenu, { openContextMenu } from "../lib/ui/ContextMenu.svelte";
  import ConfirmMenu, { openConfirm } from "../lib/ui/ConfirmMenu.svelte";

  let error = $state<string | null>(null);

  async function forgetId(id: string) {
    await api.forgetProject(id);
    await app.refreshRegistry();
  }

  function rowMenu(e: MouseEvent, entry: RegistryEntryStatus) {
    e.preventDefault();
    const x = e.clientX;
    const y = e.clientY;
    openContextMenu(x, y, [
      {
        label: "Open",
        icon: FolderOpen,
        disabled: !entry.available,
        onSelect: () => openExisting(entry.path, entry.available),
      },
      "separator",
      {
        label: "Remove from Ken…",
        icon: Trash2,
        danger: true,
        onSelect: () =>
          openConfirm(x, y, {
            title: `Remove “${entry.name}”?`,
            body: "Removes it from Ken's list — the folder and its files stay on disk.",
            confirmLabel: "Remove from Ken",
            onConfirm: () => void forgetId(entry.id),
          }),
      },
    ]);
  }
  let pendingPath = $state<string | null>(null);
  let pendingName = $state("");

  // Project-scoped feature flags offered during onboarding, plus the
  // toggles the user actually changed from their default (only those are
  // written once the project exists — see confirmCreate).
  let features = $state<FeatureInfo[]>([]);
  let featureChoices = $state<Record<string, boolean>>({});
  let featuresOpen = $state(false);

  async function chooseFolder() {
    error = null;
    const folder = await openDialog({
      directory: true,
      title: "Choose the folder that holds your knowledge",
    });
    if (typeof folder !== "string") return;
    pendingPath = folder;
    pendingName = folder.split("/").pop() ?? "My project";
    featuresOpen = false;
    try {
      const all = await api.listFeatures();
      features = all.filter((f) => f.scope === "project");
      featureChoices = Object.fromEntries(features.map((f) => [f.name, f.effective]));
    } catch {
      features = [];
      featureChoices = {};
    }
  }

  async function confirmCreate() {
    if (!pendingPath) return;
    try {
      const changed = features.filter((f) => featureChoices[f.name] !== f.effective);
      await app.createProject(pendingPath, pendingName.trim() || "My project");
      for (const f of changed) {
        const value = featureChoices[f.name];
        if (f.name === "semanticIndex") {
          await app.setSemanticIndex(value);
        } else {
          await api.setProjectFeature(f.name, value);
        }
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function openExisting(path: string, available: boolean) {
    if (!available) return;
    error = null;
    try {
      await app.openProject(path);
    } catch (e) {
      error = String(e);
    }
  }

  async function forget(id: string, e: MouseEvent) {
    e.stopPropagation();
    await api.forgetProject(id);
    await app.refreshRegistry();
  }
</script>

<div class="wrap" data-tauri-drag-region>
  <div class="panel">
    <div class="brand">
      <KenMark size={36} />
      <span class="wordmark">Ken</span>
    </div>
    <h1>Your team's knowledge, in one calm place.</h1>
    <p class="lede">
      Point Ken at a folder — notes, documents, spreadsheets, anything. Ken
      reads it, keeps watch, and makes every fact findable.
    </p>

    {#if pendingPath}
      <div class="confirm">
        <div class="mono path">{pendingPath}</div>
        <label>
          Project name
          <input
            bind:value={pendingName}
            onkeydown={(e) => {
              e.stopPropagation();
              if (e.key === "Enter") confirmCreate();
            }}
          />
        </label>

        {#if features.length > 0}
          <div class="features">
            <button
              type="button"
              class="features-toggle"
              onclick={() => (featuresOpen = !featuresOpen)}
              aria-expanded={featuresOpen}
            >
              <span class="chev" class:open={featuresOpen}>
                <ChevronRight size={13} strokeWidth={2} />
              </span>
              Features
            </button>
            {#if featuresOpen}
              <div class="features-body">
                {#each features as flag (flag.name)}
                  <label class="radio feature-row">
                    <input type="checkbox" bind:checked={featureChoices[flag.name]} />
                    <span class="feature-text">
                      <span class="feature-name">{flag.name}</span>
                      <span class="note">{flag.description}</span>
                    </span>
                  </label>
                {/each}
                <p class="note">You can change these later in Settings.</p>
              </div>
            {/if}
          </div>
        {/if}

        <div class="confirm-actions">
          <button class="btn btn-primary" onclick={confirmCreate}>Create project</button>
          <button class="btn btn-ghost" onclick={() => (pendingPath = null)}>Back</button>
        </div>
      </div>
    {:else}
      <button class="btn btn-primary big" onclick={chooseFolder}>Choose a folder…</button>
    {/if}

    {#if error}
      <div class="error">{error}</div>
    {/if}

    {#if app.registry.length > 0 && !pendingPath}
      <div class="recent-label">Recent projects</div>
      <div class="recents">
        {#each app.registry as entry (entry.id)}
          <button
            class="recent"
            class:unavailable={!entry.available}
            onclick={() => openExisting(entry.path, entry.available)}
            oncontextmenu={(e) => rowMenu(e, entry)}
          >
            <span class="badge">{entry.name.charAt(0).toUpperCase()}</span>
            <span class="info">
              <span class="name">{entry.name}</span>
              <span class="mono path-small">{entry.path}</span>
              {#if !entry.available}
                <span class="missing">Folder not found</span>
              {/if}
            </span>
            {#if !entry.available}
              <span class="forget" role="button" tabindex="0" onclick={(e) => forget(entry.id, e)} onkeydown={() => {}}>Remove</span>
            {/if}
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>

<ContextMenu />
<ConfirmMenu />

<style>
  .wrap {
    height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    /* Gentle lined paper: faint rules every 28px on the paper ground. */
    background:
      repeating-linear-gradient(
        to bottom,
        transparent,
        transparent 27px,
        var(--rule-line) 27px,
        var(--rule-line) 28px
      ),
      var(--paper);
  }
  .panel {
    width: 460px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 32px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .wordmark {
    font-family: var(--font-script);
    font-size: 26px;
    line-height: 1;
    color: var(--ink);
    /* Script baseline sits low; nudge up so it aligns with the mark. */
    transform: translateY(-3px);
  }
  h1 {
    margin: 6px 0 0;
    font-family: var(--font-serif);
    font-size: 30px;
    font-weight: 500;
    line-height: 1.2;
    letter-spacing: -0.01em;
  }
  .lede {
    margin: 0;
    font-size: 14px;
    line-height: 1.7;
    color: var(--ink-secondary);
  }
  .big {
    height: 40px;
    font-size: 14px;
    align-self: flex-start;
    margin-top: 6px;
  }
  .confirm {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .path {
    font-size: 12px;
    color: var(--ink-secondary);
    word-break: break-all;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-secondary);
  }
  input {
    height: 36px;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--border-strong);
    background: var(--surface);
    font-size: 14px;
    color: var(--ink);
    outline: none;
    font-family: inherit;
  }
  input:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .confirm-actions {
    display: flex;
    gap: 8px;
  }
  .features {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .features-toggle {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    align-self: flex-start;
    border: none;
    background: transparent;
    padding: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-tertiary);
  }
  .features-toggle:hover {
    color: var(--ink-secondary);
  }
  .chev {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    transition: transform 0.15s ease;
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .features-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 2px 2px 0;
  }
  .radio {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    cursor: pointer;
  }
  .radio input {
    accent-color: var(--accent);
  }
  .feature-row {
    align-items: flex-start;
  }
  .feature-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .feature-name {
    font-weight: 600;
    text-transform: capitalize;
  }
  .note {
    margin: 0;
    font-size: 12px;
    color: var(--ink-tertiary);
    line-height: 1.5;
  }
  .error {
    font-size: 13px;
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 7%, transparent);
    border: 1px solid color-mix(in srgb, var(--danger) 25%, transparent);
    border-radius: 10px;
    padding: 10px 14px;
  }
  .recent-label {
    font-size: 11px;
    font-weight: 700;
    color: var(--ink-tertiary);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    margin-top: 10px;
  }
  .recents {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .recent {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px;
    border-radius: 9px;
    border: 1px solid var(--border);
    background: var(--surface);
    text-align: left;
    font-size: 13px;
  }
  .recent:hover {
    background: var(--sunken);
  }
  .recent.unavailable {
    opacity: 0.7;
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
    flex: 1;
  }
  .name {
    font-weight: 600;
  }
  .path-small {
    font-size: 11px;
    color: var(--ink-tertiary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .missing {
    font-size: 11.5px;
    color: var(--danger);
  }
  .forget {
    font-size: 12px;
    font-weight: 600;
    color: var(--danger);
  }
</style>
