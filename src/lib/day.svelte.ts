// Your day: the team's tickets assigned to me, my tasks, the team digest and
// the team's index health. Refreshes on `day-changed`, the team-digest
// events, `index-updated`, a change of team, and local midnight (so done
// tasks leave the list and recurring ones come back).
import {
  api,
  memberLeaf,
  type DayState,
  type DayTask,
  type DayTaskInput,
  type DayTaskPatch,
  type IndexHealth,
  type TeamDigest,
} from "./api";
import { app } from "./app.svelte";
import { scope } from "./scope.svelte";
import { localToday, orderTasks } from "./day";

/** What the task panel shows: a new task (optionally pre-linked) or one
 *  existing task by id. */
export type DayPanel = { mode: "new"; links: string[] } | { mode: "edit"; id: string };

class DayStore {
  state = $state<DayState | null>(null);
  loadError = $state<string | null>(null);

  digest = $state<TeamDigest | null>(null);
  generating = $state(false);
  digestError = $state<string | null>(null);
  /** Optimistic until claude_doctor answers. */
  claudeFound = $state(true);

  health = $state<IndexHealth | null>(null);

  today = $state(localToday());
  panel = $state<DayPanel | null>(null);
  /** The inbox task an Accept/Not mine/Reply is running for, by id. */
  busy = $state<string | null>(null);

  private initDone = false;
  private midnightTimer: ReturnType<typeof setTimeout> | undefined;
  private seq = 0;

  /** Other, in display order. */
  get tasks(): DayTask[] {
    return orderTasks(this.state?.tasks ?? [], this.today);
  }

  get tickets() {
    return this.state?.tickets ?? [];
  }

  /** The task open in the panel, when editing. */
  get panelTask(): DayTask | null {
    const p = this.panel;
    if (!p || p.mode !== "edit") return null;
    return this.state?.tasks.find((t) => t.id === p.id) ?? null;
  }

  private get team(): string | null {
    return scope.team;
  }

  async init() {
    if (this.initDone) return;
    this.initDone = true;
    await api.onDayChanged(() => void this.refreshState());
    await api.onTeamDigestGenerating(() => {
      this.generating = true;
      this.digestError = null;
    });
    await api.onTeamDigestUpdated(() => {
      this.generating = false;
      this.digestError = null;
      // The event carries a digest but not which team it was for; read
      // ours back rather than show another team's.
      void this.refreshDigest();
    });
    await api.onTeamDigestError((message) => {
      this.generating = false;
      this.digestError = message;
    });
    await api.onIndexUpdated(() => {
      void this.refreshHealth();
      void this.refreshState();
    });
    // A change of team (or workspace) reads everything again.
    $effect.root(() => {
      $effect(() => {
        void scope.team;
        void app.workspace?.id;
        void this.refresh();
      });
    });
    this.scheduleMidnight();
    this.claudeFound = (await api.claudeDoctor().catch(() => null))?.found ?? false;
  }

  private scheduleMidnight() {
    clearTimeout(this.midnightTimer);
    const now = new Date();
    const next = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 0, 0, 5);
    this.midnightTimer = setTimeout(() => {
      this.today = localToday();
      void this.refresh();
      this.scheduleMidnight();
    }, next.getTime() - now.getTime());
  }

  async refresh() {
    await Promise.all([this.refreshState(), this.refreshDigest(), this.refreshHealth()]);
  }

  async refreshState() {
    if (!app.workspace) {
      this.state = null;
      return;
    }
    const n = ++this.seq;
    try {
      const next = await api.dayState(this.team);
      if (n !== this.seq) return;
      this.state = next;
      this.loadError = null;
      this.today = localToday();
    } catch (e) {
      if (n === this.seq) this.loadError = String(e);
    }
  }

  async refreshDigest() {
    if (!app.workspace) {
      this.digest = null;
      return;
    }
    const team = this.team;
    const d = await api.teamDigest(team).catch(() => null);
    if (team === this.team) this.digest = d;
  }

  async refreshHealth() {
    if (!app.workspace) {
      this.health = null;
      return;
    }
    const team = this.team;
    const h = await api.indexHealth(team).catch(() => null);
    if (team === this.team) this.health = h;
  }

  /** "Write it now". */
  async writeDigest() {
    this.digestError = null;
    this.generating = true;
    try {
      await api.refreshTeamDigest(this.team);
    } catch (e) {
      this.generating = false;
      this.digestError = String(e);
    }
  }

  // ── Tasks ───────────────────────────────────────────────────────────

  openNew(links: string[] = []) {
    this.panel = { mode: "new", links };
    app.screen = "home";
  }

  openTask(id: string) {
    this.panel = { mode: "edit", id };
  }

  closePanel() {
    this.panel = null;
  }

  private replace(task: DayTask) {
    if (!this.state) return;
    const at = this.state.tasks.findIndex((t) => t.id === task.id);
    const tasks = [...this.state.tasks];
    if (at === -1) tasks.push(task);
    else tasks[at] = task;
    this.state = { ...this.state, tasks };
  }

  async create(input: DayTaskInput): Promise<DayTask> {
    const task = await api.dayTaskCreate(input);
    this.replace(task);
    return task;
  }

  /** Patch a task, showing the change at once and taking the backend's copy
   *  back (it computes the next target of a recurring task). */
  async update(id: string, patch: DayTaskPatch): Promise<DayTask | null> {
    const before = this.state?.tasks.find((t) => t.id === id);
    if (before) {
      const local: DayTask = { ...before };
      if (patch.title !== undefined) local.title = patch.title;
      if (patch.target !== undefined) local.target = patch.target;
      if (patch.description !== undefined) local.description = patch.description;
      if (patch.repeat !== undefined) local.repeat = patch.repeat;
      if (patch.links !== undefined) local.links = patch.links;
      if (patch.state !== undefined) {
        local.state = patch.state;
        // Keep a just-done task in today's list until the backend answers.
        if (patch.state === "done") local.updated = localToday();
      }
      this.replace(local);
    }
    try {
      const task = await api.dayTaskUpdate(id, patch);
      this.replace(task);
      return task;
    } catch (e) {
      if (before) this.replace(before);
      throw e;
    }
  }

  async toggle(task: DayTask) {
    await this.update(task.id, { state: task.state === "done" ? "open" : "done" });
  }

  async remove(id: string) {
    await api.dayTaskDelete(id);
    if (this.state) this.state = { ...this.state, tasks: this.state.tasks.filter((t) => t.id !== id) };
    if (this.panel?.mode === "edit" && this.panel.id === id) this.panel = null;
  }

  // ── A task from a teammate, not yet accepted ────────────────────────

  private async inboxAction(task: DayTask, run: (familyId: string, itemId: string) => Promise<unknown>) {
    if (!task.inbox) return;
    this.busy = task.id;
    try {
      await run(task.inbox.familyId, task.inbox.itemId);
      await this.refreshState();
    } finally {
      this.busy = null;
    }
  }

  accept(task: DayTask) {
    return this.inboxAction(task, (f, i) => api.familyAcceptTask(f, i));
  }

  notMine(task: DayTask) {
    return this.inboxAction(task, (f, i) => api.familySetItemStatus(f, i, "archived"));
  }

  reply(task: DayTask, note: string) {
    return this.inboxAction(task, (f, i) => api.familyPushBack(f, i, note));
  }
}

export const day = new DayStore();

/** The repo name for a project id, as the workspace names it. */
export function repoName(projectId: string): string {
  const m = app.workspace?.members.find((x) => x.projectId === projectId);
  if (m) return memberLeaf(m.name);
  return app.registry.find((e) => e.id === projectId)?.name ?? projectId;
}

/** Open a file in a repo of the workspace: focus that repo, then Files. */
export async function openInRepo(projectId: string, relPath: string) {
  if (projectId !== app.focused) await app.focusMember(projectId);
  app.openInFiles(relPath);
}

/** Open a member-relative path (`repo/path/to/file.md`), as the team digest
 *  cites them. The longest member name (full or leaf) that prefixes the
 *  path wins; with none, the path opens in the focused repo. */
export async function openMemberPath(path: string) {
  const lower = path.toLowerCase();
  const candidates = (app.workspace?.members ?? []).flatMap((m) =>
    m.projectId ? [{ id: m.projectId, name: m.name }, { id: m.projectId, name: memberLeaf(m.name) }] : [],
  );
  const hit = candidates
    .sort((a, b) => b.name.length - a.name.length)
    .find((c) => lower.startsWith(c.name.toLowerCase() + "/"));
  if (hit) await openInRepo(hit.id, path.slice(hit.name.length + 1));
  else app.openInFiles(path);
}
