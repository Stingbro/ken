// Pure helpers for the Ingest screen (the library inbox, white box 8b): the
// one-line count of the pages a source wrote and what waits, page names as a
// row says them, the hold line, and what a Raw/ row says and offers.
import type { IngestOverview, RawState } from "./api";

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** "2 pages written · 3 wait": the pages written from a source and what
 *  waits, in one line. */
export function writtenLine(pages: number, waiting: number): string {
  let line = pages > 0 ? `${plural(pages, "page")} written` : "nothing written";
  if (waiting > 0) line += ` · ${waiting} ${waiting === 1 ? "waits" : "wait"}`;
  return line;
}

/** The line or two ingest wrote for a waiting risk or issue (its entry's
 *  text), read from the proposal; empty for anything else. */
export function raidDetail(payload: string | null): string {
  try {
    const raid = (JSON.parse(payload ?? "{}") as { raid?: { text?: unknown } }).raid;
    return typeof raid?.text === "string" ? raid.text.trim() : "";
  } catch {
    return "";
  }
}

/** A wiki page as a row names it: `Current/Project.md` → "Current › Project". */
export function pageName(path: string): string {
  return path
    .replace(/\.md$/i, "")
    .split("/")
    .filter((p) => p.length > 0)
    .join(" › ");
}

/** The line under the pages: none was held for a person, or how many. */
export function stagingLine(held: number): string {
  return held === 0 ? "No page edit was held for you." : `${held} held for you, above.`;
}

/** Where the ingest's commit is, so git takes it back. */
export function commitLine(commit: string | null, error: string | null): string {
  if (commit) return `One commit, ${commit}, in the knowledge base. git revert ${commit} takes it all back.`;
  if (error) return `Not committed: ${error}`;
  return "Not committed: the knowledge base is not a git repo, or nothing changed.";
}

/** How a source in Raw/ reads in its row. */
export function rawStateLabel(state: RawState): string {
  switch (state) {
    case "queued":
      return "in the queue";
    case "transcribing":
      return "transcribing";
    case "waiting":
      return "waiting for a transcript";
    case "reading":
      return "reading";
    case "read":
      return "read";
    case "failed":
      return "could not read";
  }
}

/** What a Raw/ row offers: Try again after a failure or a wait, Remove
 *  (to the system trash) whenever Ken is not on it. A read source has its
 *  card instead. */
export function rawActions(state: RawState): ("retry" | "remove")[] {
  switch (state) {
    case "failed":
    case "waiting":
      return ["retry", "remove"];
    case "queued":
      return ["remove"];
    default:
      return [];
  }
}

/** Read now: something is queued and no pass is running. */
export function canReadNow(overview: Pick<IngestOverview, "raw" | "running"> | null): boolean {
  return !!overview && !overview.running && overview.raw.some((r) => r.state === "queued");
}

/** A pass is on, or something in Raw/ is moving: the screen keeps reading. */
export function ingestBusy(overview: Pick<IngestOverview, "raw" | "running"> | null): boolean {
  return (
    !!overview &&
    (overview.running || overview.raw.some((r) => r.state === "queued" || r.state === "reading" || r.state === "transcribing"))
  );
}
