// Pure helpers behind the home footer's stats and sync indicator. Kept out of
// the .svelte file so the counting and copy can be unit-tested without a DOM.
import type { FileRow, IndexHealth, SyncStateName } from "../lib/api";

export interface FileStats {
  total: number;
  /** Indexed and therefore searchable by content. */
  searchable: number;
  /** Cloud-only files the background worker will download & index on its own
   *  (text-bearing documents under the size cap, non-excluded). These are the
   *  only ones whose count actually drains over time. */
  cloudEligible: number;
  /** Cloud-only files the worker permanently skips — videos, images/binaries
   *  with no extractable text, oversized documents, excluded paths. Their bytes
   *  only come local when the user opens them, so their count never drops. */
  cloudSkipped: number;
  /** Present but couldn't be read/indexed. */
  unreadable: number;
}

export function fileStats(
  files: Pick<FileRow, "status" | "backgroundEligible">[],
): FileStats {
  const stats: FileStats = {
    total: files.length,
    searchable: 0,
    cloudEligible: 0,
    cloudSkipped: 0,
    unreadable: 0,
  };
  for (const f of files) {
    if (f.status === "indexed") stats.searchable++;
    else if (f.status === "cloud_only") {
      // Eligibility is decided once, in Rust (`wants_background_index`), and
      // plumbed through as `backgroundEligible` — never re-derived here.
      if (f.backgroundEligible) stats.cloudEligible++;
      else stats.cloudSkipped++;
    } else if (f.status === "failed") stats.unreadable++;
  }
  return stats;
}

/** One footer tile describing some slice of the cloud-offline files. */
export interface CloudPresentation {
  count: number;
  label: string;
  /** True only for files actively being pulled down — drives the progress
   *  styling. Skipped and "waiting" counts are static (never `active`). */
  active: boolean;
}

/**
 * Cloud-offline files framed for the footer, as zero, one, or two tiles.
 *
 * With background indexing on, only worker-*eligible* files read as work in
 * flight ("indexing … in the background") with active styling — that tile
 * vanishes the moment their backlog clears. Files the worker permanently skips
 * (video/binary/oversized/excluded) get a separate static tile that tells the
 * honest truth ("downloads when opened"): their count never drains, so framing
 * them as active progress showed a number stuck at "142 left" forever.
 *
 * With the feature off nothing downloads on its own, so the eligible/skipped
 * split is meaningless — every cloud file is one plain static "waiting to
 * download" backlog the user has to act on.
 */
export function cloudPresentation(
  cloudEligible: number,
  cloudSkipped: number,
  backgroundIndex: boolean,
): CloudPresentation[] {
  if (!backgroundIndex) {
    const waiting = cloudEligible + cloudSkipped;
    if (waiting <= 0) return [];
    return [
      { count: waiting, label: "waiting to download from cloud", active: false },
    ];
  }
  const tiles: CloudPresentation[] = [];
  if (cloudEligible > 0) {
    tiles.push({
      count: cloudEligible,
      label: "indexing in the background…",
      active: true,
    });
  }
  if (cloudSkipped > 0) {
    tiles.push({
      count: cloudSkipped,
      label: "in the cloud — downloads when opened",
      active: false,
    });
  }
  return tiles;
}

/** Visual tone for the sync dot; maps onto existing status token families. */
export type SyncTone = "healthy" | "progress" | "attention" | "muted";

export interface SyncPresentation {
  tone: SyncTone;
  label: string;
}

export function syncPresentation(
  state: SyncStateName,
  detail?: string | null,
): SyncPresentation {
  switch (state) {
    case "synced":
      return { tone: "healthy", label: "Synced with your team" };
    case "syncing":
      return { tone: "progress", label: "Syncing with your team…" };
    case "attention":
      return {
        tone: "attention",
        label: detail ?? "Needs your attention",
      };
    case "off":
    default:
      // "Not syncing" collided with the footer's live cloud-download activity
      // and read as a contradiction. This line is only about team sharing, so
      // say that plainly and let the cloud tile own the word "sync".
      return { tone: "muted", label: "Local project" };
  }
}

/** One figure in the index-health row. `warn` marks what needs a look. */
export interface HealthFigure {
  count: number;
  label: string;
  warn: boolean;
}

/**
 * The knowledge graph's side of the index, as Home shows it: what is read,
 * what waits, what failed and what is skipped on purpose. An index that has
 * stopped extracting still answers searches, just thinly, so these numbers
 * are the only way anyone notices. Zero figures other than "read" are left
 * out so a healthy project shows one tile.
 */
export function healthFigures(h: IndexHealth): HealthFigure[] {
  const out: HealthFigure[] = [
    { count: h.analyzed, label: `of ${h.extractable} read for the graph`, warn: false },
  ];
  if (h.pending + h.retrying > 0) {
    out.push({ count: h.pending + h.retrying, label: "waiting to be read", warn: false });
  }
  if (h.failed > 0) out.push({ count: h.failed, label: "failed", warn: true });
  if (h.skipped > 0) out.push({ count: h.skipped, label: "searchable only", warn: false });
  return out;
}

/**
 * The one sentence Home owes the reader when the index cannot be trusted:
 * no local model (so nothing is read for entities, while search still
 * works), or the control query missed its page. Null when all is well.
 */
export function healthWarning(h: IndexHealth): string | null {
  if (h.llmStatus === "notInstalled") {
    return "No on-device model is chosen, so nothing is read for the graph. Search still works. Choose the on-device model in Settings.";
  }
  if (h.llmStatus === "error") {
    return "The on-device model hit a snag, so nothing is read for the graph. Search still works. Open Settings to check the model.";
  }
  if (h.control && !h.control.ok) {
    const top = h.control.top ? `; ${h.control.top} came first` : "";
    return `The index check missed: searching “${h.control.query}” did not bring ${h.control.page} first${top}. Results may be incomplete; rebuild the index.`;
  }
  return null;
}
