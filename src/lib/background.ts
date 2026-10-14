// Pure helpers behind the background status panel (knowledge-core decision
// 34): the one throttle's paces, and how a job or a failure reads.
import type { BackgroundJob, BuildProgress, Pace } from "./api";

export const PACES: { value: Pace; label: string; note: string }[] = [
  { value: "gentle", label: "Gentle", note: "the default: a call every 10 seconds at most, 20 an hour; local work a quarter of the time" },
  { value: "normal", label: "Normal", note: "a call every 2 seconds at most, 60 an hour; local work half the time" },
  { value: "fast", label: "Fast", note: "calls back to back, 240 an hour; local work without rests" },
];

const KINDS: Record<string, string> = {
  summaries: "Summaries",
  checks: "Checks",
  ingest: "Ingest",
  build: "Building",
  meaning: "Meaning search",
  ocr: "Text in images",
  cloud: "Cloud files",
  index: "Index",
  analysis: "Repo analysis",
};

/** A kind of work in a person's words. */
export function kindLabel(kind: string): string {
  return KINDS[kind] ?? kind;
}

/** How far a job has got, as a percent, or null when it does not know. */
export function jobPct(job: BackgroundJob): number | null {
  return job.total > 0 ? Math.round((job.done / job.total) * 100) : null;
}

/** One running job as a line: `Meaning search · Game · 40 of 100`. */
export function jobLine(job: BackgroundJob): string {
  const head = `${kindLabel(job.kind)} · ${job.label}`;
  return job.total > 0 ? `${head} · ${job.done} of ${job.total}` : head;
}

/** A job that waits for its turn, as a line: `Ingest · Wiki · paused`. */
export function waitLine(job: BackgroundJob, paused: boolean): string {
  return `${jobLine(job)} · ${paused ? "paused" : "waiting for its turn"}`;
}

/** The build queue in one line: `12 of 60 read · 1 in the inbox · 2 failed`. */
export function buildLine(b: BuildProgress): string {
  const parts = [`${b.done} of ${b.total} read`];
  if (b.reading.length > 0) parts.push(`${b.reading.length} in the inbox`);
  if (b.queued > 0) parts.push(`${b.queued} queued`);
  if (b.failed.length > 0) parts.push(`${b.failed.length} failed`);
  return parts.join(" · ");
}
