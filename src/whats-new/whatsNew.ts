// Pure logic behind the "What's new" dialog: parsing WHATS_NEW.md and deciding
// which releases a given install still has to be told about.
//
// The format is documented at the top of WHATS_NEW.md and written by the
// /release skill. Parsing stays deliberately forgiving — a malformed entry
// should degrade to a plain line, never throw and swallow the whole dialog.

/** One `- **Key part**: description` bullet. `key` is null for a plain line. */
export interface ReleaseItem {
  key: string | null;
  body: string;
}

/** One `### Section` of a release. `title` is null for items before any `###`. */
export interface ReleaseSection {
  title: string | null;
  items: ReleaseItem[];
}

/** One `## <version> — <date>` entry. */
export interface Release {
  version: string;
  date: string | null;
  sections: ReleaseSection[];
}

/** localStorage key holding the newest version this install has been shown. */
export const LAST_SEEN_KEY = "ken.whatsNew.lastSeen";

// `## 0.1.3 — 2026-09-16`, `## 0.1.3 - 2026-09-16`, or just `## 0.1.3`.
const VERSION_HEADING = /^##\s+v?(\d+(?:\.\d+)*(?:[-+][0-9A-Za-z.-]+)?)\s*(?:[—–-]\s*(.+?))?\s*$/;
const SECTION_HEADING = /^###\s+(.+?)\s*$/;
const ITEM = /^[-*]\s+(.*)$/;
const ITEM_KEY = /^\*\*(.+?)\*\*\s*:\s*(.*)$/;

/**
 * Parse WHATS_NEW.md into releases, in file order (newest first by convention).
 * Anything that isn't a recognised heading or bullet is ignored.
 */
export function parseWhatsNew(md: string): Release[] {
  const releases: Release[] = [];
  let release: Release | null = null;
  let section: ReleaseSection | null = null;

  for (const raw of md.split(/\r?\n/)) {
    const line = raw.trim();

    const version = VERSION_HEADING.exec(line);
    if (version) {
      release = { version: version[1]!, date: version[2]?.trim() || null, sections: [] };
      releases.push(release);
      section = null;
      continue;
    }
    if (!release) continue; // preamble: the title, the format comment, …

    const heading = SECTION_HEADING.exec(line);
    if (heading) {
      section = { title: heading[1]!, items: [] };
      release.sections.push(section);
      continue;
    }

    const item = ITEM.exec(line);
    if (!item) continue;
    if (!section) {
      // Items before any `### Section` still belong to the release.
      section = { title: null, items: [] };
      release.sections.push(section);
    }
    const text = item[1]!.trim();
    const keyed = ITEM_KEY.exec(text);
    section.items.push(
      keyed ? { key: keyed[1]!.trim(), body: keyed[2]!.trim() } : { key: null, body: text },
    );
  }

  return releases;
}

/**
 * Compare dotted versions numerically per segment. Missing segments count as
 * zero and pre-release/build suffixes are ignored, so `1.2` === `1.2.0` and
 * `1.2.3-beta.1` === `1.2.3`. Returns <0, 0 or >0 like a sort comparator.
 */
export function compareVersions(a: string, b: string): number {
  const parts = (v: string) =>
    v
      .trim()
      .replace(/^v/, "")
      .split(/[-+]/, 1)[0]!
      .split(".")
      .map((n) => Number.parseInt(n, 10) || 0);
  const pa = parts(a);
  const pb = parts(b);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const diff = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

/**
 * The releases to show on this launch: everything newer than `lastSeen` and no
 * newer than the running version, newest first. A null `lastSeen` means a fresh
 * install (or a pre-dialog one) — it gets nothing, so the dialog never greets a
 * first run.
 */
export function releasesToShow(
  releases: Release[],
  currentVersion: string,
  lastSeen: string | null,
): Release[] {
  if (lastSeen === null) return [];
  return releases
    .filter(
      (r) =>
        compareVersions(r.version, lastSeen) > 0 &&
        compareVersions(r.version, currentVersion) <= 0,
    )
    .sort((a, b) => compareVersions(b.version, a.version));
}
