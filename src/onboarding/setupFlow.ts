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
