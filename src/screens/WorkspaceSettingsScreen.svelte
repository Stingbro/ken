<script lang="ts">
  // Workspace settings (knowledge-core decision 37; it was the Team screen):
  // every source as set-up wrote it, one row each (indexed or off, kind,
  // branch, commit, how far behind its upstream, its health line), what the
  // knowledge base's `.ken/knowledge.json` holds and nothing else shows (the
  // index rules, the tiers, each source's roles, the controls with their last
  // results), "Analyse again" for each source the repo analysis can propose
  // for, what a scan found and what a source lacks for its kind, the
  // template files the knowledge base lacks, the pages outside its sections,
  // its drift runs and what its checks found, and its rules and templates.
  // Each row opens that repo's own settings in a drawer. Add a repo and Scan
  // again sit at the top.
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import {
    api,
    memberLeaf,
    type Finding,
    type IndexState,
    type RepoKind,
    type SetupMoved,
    type SourceRow,
    type WikiMapPlan,
    type WorkspaceSettings,
  } from "../lib/api";
  import { app } from "../lib/app.svelte";
  import { scope } from "../lib/scope.svelte";
  import { timeAgo } from "../lib/format";
  import { renderTitle } from "../lib/markdown";
  import { openConfirm } from "../lib/ui/ConfirmMenu.svelte";
  import { toast } from "../lib/toast.svelte";
  import { rail } from "../lib/rail.svelte";
  import {
    analysisNote,
    controlLine,
    controlResult,
    editorLines,
    findingLabel,
    kindlessRepos,
    orderFindings,
    parseControls,
    proposedKinds,
  } from "../lib/workspaceSettings";
  import RepoDrawer from "../workspace/RepoDrawer.svelte";
  import Loading from "../lib/ui/Loading.svelte";
  import X from "@lucide/svelte/icons/x";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";

  let overview = $state<WorkspaceSettings | null>(null);
  let moved = $state<SetupMoved[]>([]);
  let error = $state<string | null>(null);
  let busy = $state<string | null>(null);
  let newRule = $state("");
  let scanned = $state<number | null>(null);

  async function refresh() {
    try {
      overview = await api.workspaceSettings();
      rail.setFindings(overview.findings?.length ?? 0);
      error = null;
    } catch (e) {
      error = String(e);
    }
  }
  $effect(() => {
    void app.workspace?.id;
    void refresh();
  });
  // An analysis asked for again ends with a change to the Inbox, filed or
  // not, so a busy row is read again then.
  $effect(() => {
    let off: (() => void) | undefined;
    void api.onReviewChanged(() => void refresh()).then((fn) => (off = fn));
    return () => off?.();
  });

  /** An action here: `what` names it in the notice when it fails. */
  async function run(label: string, what: string, f: () => Promise<unknown>) {
    busy = label;
    try {
      await f();
    } catch (e) {
      toast.error(what, e);
    } finally {
      busy = null;
      await refresh();
    }
  }

  async function scanAgain() {
    if (!overview) return;
    const root = overview.root;
    await run("scan", "Could not scan the folders", async () => {
      moved = await api.setupRescan(root);
      scanned = Math.floor(Date.now() / 1000);
    });
  }

  // One kind per repo here; a repo set up as two (a wiki that also carries
  // code) keeps both unless a person picks one.
  const KINDS: { v: RepoKind | ""; label: string }[] = [
    { v: "code", label: "code" },
    { v: "wiki", label: "wiki" },
    { v: "reference", label: "reference" },
    { v: "", label: "not said" },
  ];
  const kindValue = (r: SourceRow) => (r.kind.length > 1 ? r.kind.join("+") : (r.kind[0] ?? ""));

  function setKind(r: SourceRow, value: string) {
    const kind = value === "" ? [] : (value.split("+") as RepoKind[]);
    void run(`k${r.id}`, "Could not set the kind", () => api.setProjectKind(r.id, kind));
  }

  // Two states (knowledge-core decision 34): a repo is indexed or it is off.
  function setIndex(r: SourceRow, value: string) {
    void run(`i${r.id}`, "Could not change whether Ken indexes it", () =>
      api.setProjectIndex(r.id, value === "off" ? ("off" as IndexState) : null),
    );
  }

  // Template files the knowledge base lacks: added on request, never over a
  // file it has. An out-of-date one is shown as a diff and left alone.
  function addMissing() {
    const paths = (overview?.templateGaps ?? []).filter((g) => g.state === "missing").map((g) => g.path);
    if (paths.length === 0) return;
    void run("templates", "Could not add the template files", () => api.workspaceAddTemplateFiles(paths));
  }

  // The wiki's pages outside the sections, placed after a dry run.
  let mapPlan = $state<WikiMapPlan | null>(null);
  async function checkLayout() {
    const path = overview?.wiki?.path;
    if (!path) return;
    await run("map", "Could not read the wiki's layout", async () => {
      mapPlan = await api.wikiMapPlan(path);
    });
  }
  function applyMap() {
    const path = overview?.wiki?.path;
    const moves = mapPlan?.moves ?? [];
    if (!path || moves.length === 0) return;
    void run("map", "Could not move the pages", async () => {
      const done = await api.wikiMapApply(path, moves);
      mapPlan = null;
      toast.show(
        `${done.moved.length} pages moved${done.unresolved.length ? `; ${done.unresolved.length} links left as they were` : ""}${done.commit ? ` · commit ${done.commit}` : ""}`,
      );
    });
  }

  function takeOut(e: MouseEvent, r: SourceRow) {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    openConfirm(rect.left - 220, rect.bottom + 4, {
      title: `Remove ${memberLeaf(r.name)} from the workspace?`,
      body: "Ken stops indexing it. The folder and its files stay on disk.",
      confirmLabel: "Remove",
      onConfirm: () =>
        void run(`o${r.id}`, "Could not remove the repo", async () => {
          await api.workspaceRemoveMember(r.name);
          await app.refreshWorkspaceOverview();
        }),
    });
  }

  async function addRepos() {
    if (!overview) return;
    const picked = await openDialog({ directory: true, multiple: true, title: "Add a repo to the workspace" });
    const paths = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
    if (paths.length === 0) return;
    const name = overview.workspace;
    await run("add", "Could not add the repo", async () => {
      const proposal = await api.setupProposeRepos(paths);
      const rows = proposal.rows.map((r) => ({ ...r, include: true }));
      await app.confirmSetupRepos(name, rows, true);
      app.openWorkspaceSettings();
      await scope.refreshGroups();
    });
  }

  function addFolder(member: string) {
    void run(`m${member}`, "Could not add the folder", async () => {
      await api.workspaceAddMember(member);
      moved = moved.filter((m) => !("member" in m) || m.member !== member);
      await app.refreshWorkspaceOverview();
      await scope.refreshGroups();
    });
  }

  // ── The editors of `.ken/knowledge.json` ────────────────────────────
  // One open at a time: `editing` names it, `draft` holds its text.
  let editing = $state<string | null>(null);
  let draft = $state("");
  function edit(what: string, lines: string[]) {
    editing = what;
    draft = lines.join("\n");
  }

  function saveIgnores() {
    const lines = editorLines(draft);
    void run("ignores", "Could not save the index rules", async () => {
      await api.workspaceSaveIgnores(lines);
      editing = null;
    });
  }

  function saveWeights() {
    const lines = editorLines(draft);
    void run("weights", "Could not save the tiers", async () => {
      await api.workspaceSaveWeights(lines);
      editing = null;
    });
  }

  function saveRoles(r: SourceRow) {
    const lines = editorLines(draft);
    void run(`roles${r.id}`, `Could not save ${memberLeaf(r.name)}'s roles`, async () => {
      await api.workspaceSaveRoles(r.id, r.settingsName, lines);
      editing = null;
    });
  }

  function saveControls() {
    const { controls, bad } = parseControls(draft);
    if (bad.length > 0) {
      toast.error("Each control needs a question and a file", bad.join(" · "));
      return;
    }
    void run("controls", "Could not save the controls", async () => {
      await api.workspaceSaveControls(controls);
      editing = null;
    });
  }

  // "Analyse again" (Chris, 2026-10-14): the source's earlier no is
  // forgotten and one background call is queued; its proposal arrives in
  // the Inbox, replacing one still waiting.
  function analyseAgain(r: SourceRow) {
    void run(`analyse${r.id}`, `Could not analyse ${memberLeaf(r.name)} again`, () => api.analyzeSourceAgain(r.id));
  }

  function runSweep() {
    const id = overview?.wiki?.id;
    if (!id) return;
    void run("sweep", "Could not run drift", () => api.runDriftNow(id));
  }

  function addRule() {
    const id = overview?.wiki?.id;
    const rule = newRule.trim();
    if (!id || !rule) return;
    void run("rule", "Could not add the rule", async () => {
      const path = await api.workspaceAddRule(id, rule);
      newRule = "";
      await openInWiki(path);
    });
  }

  async function openInWiki(path: string) {
    const id = overview?.wiki?.id;
    if (!id) return;
    if (app.focused !== id) await app.focusMember(id);
    app.openInFiles(path);
  }

  async function openFinding(f: Finding) {
    if (!f.path) return;
    const id = f.projectId ?? overview?.wiki?.id;
    if (id && app.focused !== id) await app.focusMember(id);
    app.openInFiles(f.path);
  }

  // ── A repo's own settings, in a drawer from its row ─────────────────
  let drawer = $state<SourceRow | null>(null);
  async function openDrawer(r: SourceRow) {
    drawer = r;
    try {
      await app.focusMember(r.id, { stay: true });
    } catch (e) {
      drawer = null;
      toast.error(`Could not open ${memberLeaf(r.name)}`, e);
    }
  }

  // ── Repos no one has said the kind of ───────────────────────────────
  let kindsOpen = $state(false);
  let kindDraft = $state<Record<string, string>>({});
  async function openKinds() {
    const list = kindlessRepos(rows);
    kindsOpen = true;
    // Set-up's proposal, from what is in each repo, fills the choices in.
    const proposal = await api.setupProposeRepos(list.map((r) => r.path)).catch(() => null);
    const kinds = proposal ? proposedKinds(list, proposal.rows) : {};
    kindDraft = Object.fromEntries(list.map((r) => [r.id, (kinds[r.id] ?? []).join("+")]));
  }
  function saveKinds() {
    const chosen = Object.entries(kindDraft).filter(([, v]) => v !== "");
    if (chosen.length === 0) return;
    void run("kinds", "Could not set the kinds", async () => {
      for (const [id, v] of chosen) await api.setProjectKind(id, v.split("+") as RepoKind[]);
      kindsOpen = false;
    });
  }

  const rows = $derived(overview ? [...(overview.wiki ? [overview.wiki] : []), ...overview.repos] : []);
  const codeRows = $derived(rows.filter((r) => r.kind.includes("code") || r.kind.includes("reference")));
  const driftCount = $derived(moved.length + rows.filter((r) => (r.behind ?? 0) > 0).length);
  const findings = $derived(orderFindings(overview?.findings ?? []));
  const kindless = $derived(kindlessRepos(rows));
  const analysable = $derived(rows.filter((r) => r.canAnalyzeAgain));
  const controlsPassing = $derived((overview?.controls ?? []).filter((c) => c.passed).length);
</script>

<div class="team">
  <div class="head">
    <strong class="tname">{overview?.workspace ?? app.workspace?.name ?? ""}</strong>
    <span class="spacer"></span>
    {#if overview}
      <span class="chip" class:warn={driftCount > 0}>drifted · {driftCount}</span>
      <span class="chip" class:warn={overview.gaps.length > 0}>gaps · {overview.gaps.length}</span>
      <span class="chip" class:warn={findings.length > 0}>findings · {findings.length}</span>
    {/if}
    <button class="btn btn-ghost" disabled={busy !== null} onclick={addRepos}>+ Add a repo</button>
    <button class="btn btn-primary" disabled={busy !== null} onclick={scanAgain}>{busy === "scan" ? "Scanning…" : "Scan again"}</button>
  </div>
  {#if error}<p class="warn-text">{error}</p>{/if}
  {#if !overview && !error}
    <Loading label="Reading the workspace's folders…" lines={4} />
  {/if}

  {#if overview && kindless.length > 0}
    <div class="kinds">
      <div class="kinds-head">
        <span>
          <strong>Set what each repo is.</strong>
          {kindless.length} {kindless.length === 1 ? "repo has" : "repos have"} no kind, so Ken does not know whether
          {kindless.length === 1 ? "it is" : "they are"} the knowledge base, code or a reference.
        </span>
        {#if !kindsOpen}
          <button class="btn btn-small btn-primary" onclick={() => void openKinds()}>Set kinds</button>
        {/if}
      </div>
      {#if kindsOpen}
        {#each kindless as r (r.id)}
          <div class="kind-row">
            <span class="rname">{memberLeaf(r.name)}</span>
            <span class="sel">
              <select bind:value={kindDraft[r.id]} disabled={busy !== null} aria-label="Kind of {memberLeaf(r.name)}">
                <option value="">not said</option>
                {#if (kindDraft[r.id] ?? "").includes("+")}<option value={kindDraft[r.id]}>{kindDraft[r.id].split("+").join(" + ")}</option>{/if}
                {#each KINDS.filter((k) => k.v !== "") as k (k.v)}<option value={k.v}>{k.label}</option>{/each}
              </select>
              <ChevronDown size={12} strokeWidth={1.75} />
            </span>
          </div>
        {/each}
        <p class="note">Filled in from what is in each repo. The knowledge base (wiki) is the repo with the Research/Ingestion inbox.</p>
        <div class="actions">
          <button class="btn btn-ghost btn-small" onclick={() => (kindsOpen = false)}>Cancel</button>
          <button class="btn btn-primary btn-small" disabled={busy !== null} onclick={saveKinds}>Save</button>
        </div>
      {/if}
    </div>
  {/if}

  {#if overview}
    <div class="cols">
      <div class="left">
        <div class="divider">
          sources · {rows.length}{#if scanned} · last scan {timeAgo(scanned)}{/if}
        </div>
        <table>
          <thead>
            <tr><th>index</th><th>source</th><th>kind</th><th>branch</th><th>commit</th><th>drift</th><th></th></tr>
          </thead>
          <tbody>
            {#each rows as r (r.id)}
              <tr class:gone={!r.available}>
                <td>
                  <span class="sel">
                    <select
                      value={r.effectiveIndex}
                      disabled={busy !== null}
                      onchange={(e) => setIndex(r, e.currentTarget.value)}
                      title="Whether Ken indexes it"
                    >
                      <option value="indexed">indexed</option>
                      <option value="off">off</option>
                    </select>
                    <ChevronDown size={12} strokeWidth={1.75} />
                  </span>
                </td>
                <td>
                  <button class="repo" title="{memberLeaf(r.name)}: its settings" onclick={() => void openDrawer(r)}>
                    <span class="rname">{memberLeaf(r.name)}{r.id === overview.wiki?.id ? " · knowledge base" : ""}</span>
                    <span class="sub mono" title={r.path}>{r.description || r.path}</span>
                    <span class="sub">{r.health ?? "not indexed yet"}</span>
                  </button>
                </td>
                <td>
                  <span class="sel">
                    <select value={kindValue(r)} disabled={busy !== null} onchange={(e) => setKind(r, e.currentTarget.value)}>
                      {#if r.kind.length > 1}<option value={kindValue(r)}>{r.kind.join(" + ")}</option>{/if}
                      {#each KINDS as k (k.v)}<option value={k.v}>{k.label}</option>{/each}
                    </select>
                    <ChevronDown size={12} strokeWidth={1.75} />
                  </span>
                </td>
                <td class="mono">{r.branch ?? "—"}</td>
                <td class="mono">{r.head ?? "—"}</td>
                <td>
                  {#if !r.available}<span class="bad">folder gone</span>
                  {:else if (r.behind ?? 0) > 0}<span class="bad">{r.behind} newer upstream</span>
                  {:else}—{/if}
                </td>
                <td class="row-actions">
                  <button
                    class="remove"
                    disabled={!r.available}
                    onclick={() => void openDrawer(r)}
                    title="Its settings"
                    aria-label="{memberLeaf(r.name)} settings"
                  >
                    <SlidersHorizontal size={14} strokeWidth={1.75} />
                  </button>
                  {#if r.id !== overview.wiki?.id}
                    <button
                      class="remove"
                      disabled={busy !== null}
                      onclick={(e) => takeOut(e, r)}
                      title="Remove from the workspace"
                      aria-label="Remove {memberLeaf(r.name)} from the workspace"
                    >
                      <X size={14} strokeWidth={1.75} />
                    </button>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>

        <div class="divider">index rules · .ken/knowledge.json · {overview.ignores.length} {overview.ignores.length === 1 ? "line" : "lines"}</div>
        {#if editing === "ignores"}
          <textarea class="mono" rows="8" bind:value={draft}></textarea>
          <p class="note">
            One path or pattern a line: a path is indexed or it isn't. A bare line leaves it out, a
            <span class="mono">!path/</span> line brings it back. Saved in the knowledge base; every repo is scanned again.
          </p>
          <div class="actions">
            <button class="btn btn-ghost" onclick={() => (editing = null)}>Cancel</button>
            <button class="btn btn-primary" disabled={busy !== null} onclick={saveIgnores}>Save</button>
          </div>
        {:else}
          <div class="ignores">
            <span class="mono">{overview.ignores.length > 0 ? overview.ignores.join(" · ") : "VCS, dependencies, build output, secrets, tests, minified and generated files and binaries are left out already"}</span>
            <button class="btn btn-ghost" onclick={() => edit("ignores", overview?.ignores ?? [])} disabled={!overview.wiki}>Edit</button>
          </div>
          {#if overview.legacyIgnores.length > 0}
            <p class="note">
              Still read from the older <span class="mono">.kenignore</span> beside the repos:
              <span class="mono">{overview.legacyIgnores.join(" · ")}</span>
            </p>
          {/if}
        {/if}

        <div class="divider">tiers · search.weights · {overview.weights.length}</div>
        {#if editing === "weights"}
          <textarea class="mono" rows="6" bind:value={draft}></textarea>
          <p class="note">
            One <span class="mono">pattern = tier</span> a line: binding, current, reference, record or superseded, or a
            number added to a hit's score. The longest pattern wins. The knowledge base is scanned again.
          </p>
          <div class="actions">
            <button class="btn btn-ghost" onclick={() => (editing = null)}>Cancel</button>
            <button class="btn btn-primary" disabled={busy !== null} onclick={saveWeights}>Save</button>
          </div>
        {:else}
          <div class="ignores">
            <span class="mono">{overview.weights.length > 0 ? overview.weights.join(" · ") : "The built-in tiers only: rulings and Rules bind, Current and Design pages are current, records and notes weigh less"}</span>
            <button class="btn btn-ghost" onclick={() => edit("weights", overview?.weights ?? [])} disabled={!overview.wiki}>Edit</button>
          </div>
        {/if}

        <div class="divider">roles · code and reference sources · {codeRows.length}</div>
        {#each codeRows as r (r.id)}
          {#if editing === `roles${r.id}`}
            <div class="entry"><span><strong>{memberLeaf(r.name)}</strong></span></div>
            <textarea class="mono" rows="5" bind:value={draft}></textarea>
            <p class="note">
              One <span class="mono">pattern = role</span> a line, such as <span class="mono">src/net/** = protocol</span>. The
              longest pattern wins; a file no rule names gets a built-in role. {memberLeaf(r.name)} is scanned again.
            </p>
            <div class="actions">
              <button class="btn btn-ghost" onclick={() => (editing = null)}>Cancel</button>
              <button class="btn btn-primary" disabled={busy !== null} onclick={() => saveRoles(r)}>Save</button>
            </div>
          {:else}
            <div class="entry">
              <span>
                <strong>{memberLeaf(r.name)}</strong>
                <span class="mono">{r.roles.length > 0 ? r.roles.join(" · ") : "built-in roles only"}</span>
              </span>
              <span class="e-actions"><button class="btn btn-ghost" disabled={!overview.wiki} onclick={() => edit(`roles${r.id}`, r.roles)}>Edit</button></span>
            </div>
          {/if}
        {:else}
          <p class="note">No code or reference source yet.</p>
        {/each}

        <div class="divider">controls · {controlsPassing} of {overview.controls.length} passing</div>
        {#if editing === "controls"}
          <textarea class="mono" rows="6" bind:value={draft}></textarea>
          <p class="note">
            One <span class="mono">question | file | source</span> a line: the file the question must find in the top 5. No
            source means the knowledge base. They run at each source's next rebuild.
          </p>
          <div class="actions">
            <button class="btn btn-ghost" onclick={() => (editing = null)}>Cancel</button>
            <button class="btn btn-primary" disabled={busy !== null} onclick={saveControls}>Save</button>
          </div>
        {:else}
          {#each overview.controls as c, i (i)}
            <div class="entry">
              <span>
                {c.question} → <span class="mono">{c.expect}</span>{#if c.source}<span class="sub"> · {c.source}</span>{/if}
              </span>
              <span class="e-actions"><span class:bad={c.ran && !c.passed}>{controlResult(c)}</span></span>
            </div>
          {:else}
            <p class="note">No controls yet. Set-up proposes a few for each source once Claude has read its file list.</p>
          {/each}
          <div class="actions">
            <button class="btn btn-ghost" disabled={!overview.wiki} onclick={() => edit("controls", (overview?.controls ?? []).map(controlLine))}>Edit</button>
          </div>
        {/if}

        <div class="divider">repo analysis · proposed in the Inbox · {analysable.length}</div>
        {#each analysable as r (r.id)}
          <div class="entry">
            <span class="finding">
              <span class="ftitle">{memberLeaf(r.name)}</span>
              <span class="sub">{analysisNote(r, overview.controls, overview.wiki?.settingsName ?? null)}</span>
            </span>
            <span class="e-actions">
              <button class="btn btn-ghost" disabled={r.analyzing || busy !== null} onclick={() => analyseAgain(r)}>
                {r.analyzing ? "Analysing…" : "Analyse again"}
              </button>
            </span>
          </div>
        {:else}
          <p class="note">No source to analyse: one needs its kind said and a knowledge base that lists it.</p>
        {/each}

        {#if overview.wiki}
          <div class="divider">knowledge base files · against Ken's template · {overview.templateGaps.length}</div>
          {#each overview.templateGaps as g (g.path)}
            <div class="entry">
              <span>
                <span class="mono">{g.path}</span>
                {g.state === "missing" ? "is missing." : "differs from Ken's template:"}
                {#if g.diff.length > 0}<pre class="diff mono">{g.diff.join("\n")}</pre>{/if}
              </span>
            </div>
          {:else}
            <p class="note">Every template file is there and current.</p>
          {/each}
          {#if overview.templateGaps.some((g) => g.state === "missing")}
            <div class="actions">
              <button class="btn btn-ghost" disabled={busy !== null} onclick={addMissing}>Add the missing files</button>
            </div>
          {/if}

          <div class="divider">pages outside the sections</div>
          {#if mapPlan}
            {#each mapPlan.moves as m (m.from)}
              <div class="entry"><span><span class="mono">{m.from}</span> → <span class="mono">{m.to}</span> <span class="sub">({m.why})</span></span></div>
            {:else}
              <p class="note">Every page is in a section.</p>
            {/each}
            {#each mapPlan.left as [page, why] (page)}
              <div class="entry"><span><span class="mono">{page}</span> stays: {why}</span></div>
            {/each}
            <div class="actions">
              <button class="btn btn-ghost" onclick={() => (mapPlan = null)}>Cancel</button>
              {#if mapPlan.moves.length > 0}
                <button class="btn btn-primary" disabled={busy !== null} onclick={applyMap}>Move {mapPlan.moves.length} pages</button>
              {/if}
            </div>
            <p class="note">Nothing is deleted. Links are rewritten where Ken is sure of the target; the rest are listed. One commit.</p>
          {:else}
            <div class="actions">
              <button class="btn btn-ghost" disabled={busy !== null} onclick={() => void checkLayout()}>Check the layout (dry run)</button>
            </div>
          {/if}
        {/if}
      </div>

      <div class="right">
        <div class="divider">drift and gaps · {moved.length + overview.gaps.length}</div>
        {#each moved as m, i (i)}
          <div class="entry">
            {#if m.change === "NewFolder"}
              <span><strong>{memberLeaf(m.member)}</strong> is a new folder beside the repos.</span>
              <span class="e-actions"><button class="btn btn-primary" disabled={busy !== null} onclick={() => addFolder(m.member)}>Add to the workspace</button></span>
            {:else if m.change === "Gone"}
              <span><strong>{memberLeaf(m.member)}</strong>'s folder is gone.</span>
            {:else}
              <span>A new worktree, <span class="mono">{m.pattern}</span>, is left out.</span>
            {/if}
          </div>
        {/each}
        {#each overview.gaps as g, i (i)}
          <div class="entry">
            <span>{g.text}</span>
            {#if g.open}
              <span class="e-actions"><button class="btn btn-ghost" onclick={() => g.open && openInWiki(g.open)}>Open</button></span>
            {/if}
          </div>
        {/each}
        {#if moved.length + overview.gaps.length === 0}
          <p class="note">Nothing drifted and nothing missing{scanned ? "" : " (Scan again checks for new folders)"}.</p>
        {/if}

        <div class="divider">drift · runs when the code moves, and weekly</div>
        {#if !overview.wiki}
          <p class="note">No knowledge base, so nothing to check.</p>
        {:else if overview.sweep}
          <div class="entry">
            <span>
              Last run {timeAgo(overview.sweep.at)} · {overview.sweep.pagesExamined} pages ·
              {overview.sweep.voidReason ? `void: ${overview.sweep.voidReason}` : overview.sweep.uncontrolled ? "no controls set" : "both controls held"}
            </span>
          </div>
          {#if !overview.sweep.voidReason}
            <div class="entry">
              <span>
                {overview.sweep.stamped?.length ?? 0} checked · {overview.sweep.fixed?.length ?? 0} fixed ·
                {overview.sweep.held?.length ?? 0} held for you{overview.sweep.waiting?.length ? ` · ${overview.sweep.waiting.length} waiting to be read` : ""}{overview.sweep.commit ? ` · commit ${overview.sweep.commit}` : ""}
              </span>
            </div>
          {/if}
        {:else}
          <p class="note">Drift has not run yet.</p>
        {/if}
        {#if overview.wiki}
          <div class="actions">
            <button class="btn btn-ghost" disabled={busy !== null} onclick={runSweep}>{busy === "sweep" ? "Checking…" : "Run drift now"}</button>
          </div>
        {/if}

        <div class="divider">findings · drift, lint, ingest · {findings.length}</div>
        {#each findings as f, i (i)}
          <div class="entry">
            <span class="tag">{findingLabel(f.kind)}</span>
            <span class="finding">
              <span class="ftitle">{@html renderTitle(f.title)}</span>
              {#if f.detail}<span class="sub">{f.detail}</span>{/if}
            </span>
            {#if f.path}
              <span class="e-actions"><button class="btn btn-ghost" onclick={() => void openFinding(f)}>Open</button></span>
            {/if}
          </div>
        {:else}
          <p class="note">{overview.wiki ? "Nothing found." : "No knowledge base, so nothing to check."}</p>
        {/each}

        <div class="divider">rules · Rules · {overview.rules.length}</div>
        {#each overview.rules as r (r.path)}
          <button class="page" onclick={() => openInWiki(r.path)}>{r.title}</button>
        {:else}
          <p class="note">No rules yet.</p>
        {/each}
        {#if overview.wiki}
          <div class="add-rule">
            <input class="input" bind:value={newRule} placeholder="Add a rule, as a sentence" onkeydown={(e) => e.key === "Enter" && addRule()} />
            <button class="btn btn-ghost" disabled={!newRule.trim() || busy !== null} onclick={addRule}>Add</button>
          </div>
        {/if}

        <div class="divider">templates · {overview.templates.length}</div>
        {#each overview.templates as t (t.path)}
          <button class="page" onclick={() => openInWiki(t.path)}>{t.title}</button>
        {:else}
          <p class="note">No templates.</p>
        {/each}
      </div>
    </div>
  {/if}
</div>

{#if drawer}
  {#key drawer.id}
    <RepoDrawer repo={drawer} close={() => (drawer = null)} changed={() => void refresh()} />
  {/key}
{/if}

<style>
  .team {
    display: flex;
    flex-direction: column;
    gap: 10px;
    container-type: inline-size;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .tname {
    font-size: 15px;
  }
  .spacer {
    flex: 1;
  }
  .chip {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 99px;
    border: 1px solid var(--border);
    color: var(--ink-tertiary);
  }
  .chip.warn {
    color: var(--needs-input);
    border-color: color-mix(in srgb, var(--needs-input) 45%, transparent);
  }
  .cols {
    display: grid;
    grid-template-columns: minmax(0, 1.7fr) minmax(280px, 1fr);
    gap: 24px;
  }
  /* The sources table cannot shrink below its columns: where the screen is
     too narrow for it beside the drift and findings column, that column goes
     below it instead of over it. */
  @container (max-width: 1100px) {
    .cols {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .left,
  .right {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .divider {
    margin-top: 10px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border);
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--ink-tertiary);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
  }
  th {
    text-align: left;
    font-size: 10.5px;
    font-weight: 600;
    color: var(--ink-tertiary);
    padding: 4px 6px;
  }
  td {
    padding: 6px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  tr.gone {
    opacity: 0.55;
  }
  .repo {
    display: flex;
    flex-direction: column;
    gap: 2px;
    max-width: 320px;
    border: none;
    background: transparent;
    padding: 0;
    font: inherit;
    text-align: left;
    color: inherit;
    cursor: pointer;
  }
  .repo:hover .rname {
    color: var(--accent-deep);
  }
  .row-actions {
    white-space: nowrap;
  }
  .kinds {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 14px;
    border: 1px solid color-mix(in srgb, var(--needs-input) 35%, var(--border));
    border-radius: var(--radius-card);
    background: color-mix(in srgb, var(--needs-input) 6%, var(--surface));
    font-size: 12.5px;
  }
  .kinds-head {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .kinds-head > span {
    flex: 1;
    line-height: 1.5;
  }
  .kind-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .kind-row .rname {
    width: 180px;
    flex: none;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tag {
    flex: none;
    width: 70px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--ink-tertiary);
  }
  .entry > span.finding {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .ftitle {
    font-weight: 600;
  }
  .rname {
    font-weight: 600;
    font-size: 13px;
  }
  .sub {
    font-size: 11px;
    color: var(--ink-tertiary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sel {
    position: relative;
    display: inline-flex;
    align-items: center;
    color: var(--ink-tertiary);
  }
  .sel :global(svg) {
    position: absolute;
    right: 8px;
    pointer-events: none;
  }
  .sel select {
    appearance: none;
    -webkit-appearance: none;
    font: inherit;
    font-size: 12px;
    color: var(--ink);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px 26px 4px 9px;
    min-width: 120px;
    cursor: pointer;
    outline: none;
  }
  .sel select:hover:not(:disabled) {
    border-color: var(--border-strong);
  }
  .sel select:focus-visible {
    border-color: var(--accent);
  }
  .sel select:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .sel select option {
    background: var(--surface);
    color: var(--ink);
  }
  .remove {
    display: inline-grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--ink-tertiary);
    cursor: pointer;
  }
  .remove:hover:not(:disabled) {
    background: var(--sunken);
    color: var(--danger);
  }
  .bad {
    color: var(--needs-input);
  }
  .diff {
    margin: 4px 0 0;
    padding: 6px 8px;
    border-radius: 6px;
    background: var(--sunken-2);
    font-size: 11.5px;
    white-space: pre-wrap;
  }
  .ignores {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
  }
  .ignores .mono {
    flex: 1;
    color: var(--ink-secondary, var(--ink));
  }
  textarea {
    width: 100%;
    font-size: 12px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .entry {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12.5px;
    padding: 6px 0;
    border-bottom: 1px dashed var(--border);
  }
  .entry > span:first-child:not(.tag) {
    flex: 1;
  }
  .e-actions {
    display: flex;
    gap: 6px;
    flex: none;
  }
  .page {
    background: none;
    border: none;
    text-align: left;
    padding: 4px 0;
    font-size: 13px;
    color: var(--accent);
    cursor: pointer;
  }
  .page:hover {
    text-decoration: underline;
  }
  .add-rule {
    display: flex;
    gap: 6px;
  }
  .add-rule input {
    flex: 1;
    font-size: 12.5px;
  }
  .note {
    margin: 2px 0;
    font-size: 12.5px;
    color: var(--ink-tertiary);
  }
  .warn-text {
    margin: 0;
    font-size: 12.5px;
    color: var(--needs-input);
  }
</style>
