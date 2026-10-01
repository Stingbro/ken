// Pure helpers for the Ingest screen (the library inbox, white box 8b): the
// one-line count of what a source wrote and what waits, page names as a row
// says them, staging's line, and the rail count.
import type { InboxItem, IngestWriteKind } from "./api";

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** "2 pages, 1 escalation written, 1 on my day · 3 wait" — what was written
 *  from a source and what waits, in one line. */
export function writtenLine(written: IngestWriteKind[], waiting: number): string {
  const count = (...kinds: IngestWriteKind[]) => written.filter((k) => kinds.includes(k)).length;
  const parts: string[] = [];
  const pages = count("edit", "page");
  const ideas = count("idea");
  const escalations = count("escalation");
  const tasks = count("task");
  if (pages > 0) parts.push(plural(pages, "page"));
  if (ideas > 0) parts.push(plural(ideas, "idea"));
  if (escalations > 0) parts.push(plural(escalations, "escalation"));
  let line = parts.length > 0 ? `${parts.join(", ")} written` : "";
  if (tasks > 0) line = line ? `${line}, ${tasks} on my day` : `${tasks} on my day`;
  if (!line) line = "nothing written";
  if (waiting > 0) line += ` · ${waiting} ${waiting === 1 ? "waits" : "wait"}`;
  return line;
}

/** A wiki page as a row names it: `Current/Project.md` → "Current › Project". */
export function pageName(path: string): string {
  return path
    .replace(/\.md$/i, "")
    .split("/")
    .filter((p) => p.length > 0)
    .join(" › ");
}

/** The line under the page edits: staging held none, or how many. */
export function stagingLine(held: number): string {
  return held === 0 ? "Every page edit went through staging; none was held." : `${held} held, below.`;
}

/** Sources with something waiting on Review: distinct notes behind the open
 *  page-proposal items (rulings, tickets, held edits). The Ingest tab's count. */
export function sourcesWaiting(items: InboxItem[]): number {
  const notes = new Set<string>();
  for (const it of items) {
    if (it.kind !== "page-proposal" || !it.payload) continue;
    try {
      const from = (JSON.parse(it.payload) as { from?: string }).from;
      if (from) notes.add(from);
    } catch {
      // A malformed payload names no source.
    }
  }
  return notes.size;
}
