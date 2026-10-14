/**
 * Pure logic for the Timeline screen, the project's raid log
 * (knowledge-core decision 37): which kinds it shows, filtering by kind and
 * words, grouping by day, and marking matches. The entries come from
 * `api.raidLog()`, newest first.
 */
import type { RaidEntry } from "./api";

/** The kinds of entry, in the order the filter chips list them. */
export const RAID_KINDS = ["ingest", "ruling", "ticket", "commit", "finding", "release", "drift", "lint"] as const;

/** A kind's chip label. Kinds a `log.md` line names beyond these keep
 *  their own word. */
export function kindLabel(kind: string): string {
  switch (kind) {
    case "ingest":
      return "Ingests";
    case "ruling":
      return "Rulings";
    case "ticket":
      return "Tickets";
    case "commit":
      return "Commits";
    case "finding":
      return "Findings";
    case "release":
      return "Releases";
    case "drift":
      return "Drift";
    case "lint":
      return "Lint";
    default:
      return kind;
  }
}

/** The kinds present in `entries`, the known ones in chip order first. */
export function kindsIn(entries: RaidEntry[]): string[] {
  const present = new Set(entries.map((e) => e.kind));
  const known: string[] = RAID_KINDS.filter((k) => present.has(k));
  const other = [...present].filter((k) => !(RAID_KINDS as readonly string[]).includes(k)).sort();
  return [...known, ...other];
}

/** The entries of `kind` (all when null) whose text, repo, reference or
 *  path holds `query`, case-insensitively. Order is kept. */
export function filterLog(entries: RaidEntry[], kind: string | null, query: string): RaidEntry[] {
  const q = query.trim().toLowerCase();
  return entries.filter(
    (e) =>
      (!kind || e.kind === kind) &&
      (!q ||
        e.text.toLowerCase().includes(q) ||
        (e.repo ?? "").toLowerCase().includes(q) ||
        (e.reference ?? "").toLowerCase().includes(q) ||
        (e.path ?? "").toLowerCase().includes(q)),
  );
}

export interface DayGroup {
  date: string;
  entries: RaidEntry[];
}

/** Entries grouped by their day, in the order they come (newest first). */
export function groupByDate(entries: RaidEntry[]): DayGroup[] {
  const out: DayGroup[] = [];
  for (const e of entries) {
    const last = out[out.length - 1];
    if (last && last.date === e.date) last.entries.push(e);
    else out.push({ date: e.date, entries: [e] });
  }
  return out;
}

/**
 * Escape-then-mark: HTML in `text` is escaped, and the only tags in the
 * result are our own <mark>s around case-insensitive matches of `query`.
 */
export function highlightMatches(text: string, query: string): string {
  const esc = (s: string) => s.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
  const q = query.trim();
  if (!q) return esc(text);
  const lower = text.toLowerCase();
  const ql = q.toLowerCase();
  let out = "";
  let i = 0;
  for (;;) {
    const at = lower.indexOf(ql, i);
    if (at === -1) break;
    out += esc(text.slice(i, at)) + "<mark>" + esc(text.slice(at, at + q.length)) + "</mark>";
    i = at + q.length;
  }
  return out + esc(text.slice(i));
}
