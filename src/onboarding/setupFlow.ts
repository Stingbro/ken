// Pure helpers behind SetupFlow, kept out of the .svelte file so they can be
// unit-tested without a DOM.
import type { RepoKind, SetupRepoRow } from "../lib/api";

export const KINDS: RepoKind[] = ["team", "wiki", "code", "reference"];

/** Add or remove one kind, keeping the list in the fixed order. */
export function toggleKind(kind: RepoKind[], k: RepoKind): RepoKind[] {
  const has = kind.includes(k);
  return KINDS.filter((x) => (x === k ? !has : kind.includes(x)));
}

/** "None, Ken only": every row loses its team. */
export function noTeams(rows: SetupRepoRow[]): SetupRepoRow[] {
  return rows.map((r) => ({ ...r, team: null }));
}

/** How many included repos land in each index state; an excluded row, or
 *  one set to off, counts as not indexed. */
export function indexSummary(rows: SetupRepoRow[]): { entities: number; search: number; off: number } {
  const out = { entities: 0, search: 0, off: 0 };
  for (const r of rows) {
    if (!r.include || r.index === "off") out.off++;
    else out[r.index]++;
  }
  return out;
}

/** Rows for a fresh proposal, keeping what a person already changed on a
 *  repo they had picked before (matched by folder). */
export function mergeRows(old: SetupRepoRow[], fresh: SetupRepoRow[]): SetupRepoRow[] {
  return fresh.map((r) => {
    const was = old.find((o) => o.path && o.path === r.path);
    return was
      ? { ...r, include: was.include, kind: [...was.kind], team: was.team, index: was.index, description: was.description }
      : { ...r, kind: [...r.kind], evidence: [...r.evidence] };
  });
}

/** The team names the rows carry, once each, sorted. */
export function suggestedTeams(rows: SetupRepoRow[]): string[] {
  return [...new Set(rows.filter((r) => r.include && r.team).map((r) => r.team as string))].sort();
}

/** What a team does for its wiki at set-up: use a wiki repo already picked,
 *  create a new one from the template, or none for now. */
export type WikiChoice =
  | { mode: "existing"; member: string }
  | { mode: "new"; parent: string | null; name: string }
  | { mode: "none" };

/** Included wiki repos on a team. */
export function teamWikis(rows: SetupRepoRow[], team: string): SetupRepoRow[] {
  return rows.filter((r) => r.include && r.team === team && r.kind.includes("wiki"));
}

/** A team with a wiki repo picked uses it; otherwise none until a person
 *  chooses to create one. */
export function defaultWikiChoice(rows: SetupRepoRow[], team: string): WikiChoice {
  const w = teamWikis(rows, team)[0];
  return w ? { mode: "existing", member: w.member } : { mode: "none" };
}

/** The team's repos a new wiki lists on its Start Here page. */
export function coveredRepos(rows: SetupRepoRow[], team: string): { name: string; description: string }[] {
  return rows
    .filter((r) => r.include && r.team === team && !r.kind.includes("wiki"))
    .map((r) => ({ name: r.member, description: r.description }));
}

/** A new wiki's folder: `name` inside `parent`, in the parent's own separator. */
export function newWikiPath(parent: string, name: string): string {
  const sep = parent.includes("\\") ? "\\" : "/";
  return parent.replace(/[\\/]+$/, "") + sep + name.trim();
}

/** Every "create" choice has a folder and a name. */
export function wikiChoicesReady(choices: Record<string, WikiChoice>): boolean {
  return Object.values(choices).every((c) => c.mode !== "new" || (!!c.parent && c.name.trim().length > 0));
}
