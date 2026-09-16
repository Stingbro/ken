import { describe, it, expect } from "vitest";
import {
  parseWhatsNew,
  compareVersions,
  releasesToShow,
  LAST_SEEN_KEY,
} from "./whatsNew";
import whatsNewMd from "../../WHATS_NEW.md?raw";

const SAMPLE = `<!-- a format comment -->

# What's New

## 0.2.0 — 2026-10-01

### Editor

- **Mermaid diagrams**: fences render inline.
- **GitHub alerts**: \`> [!NOTE]\` banners round-trip.

### Releases

- **What's new dialog**: shown once per version.

## 0.1.3

### Fixes

- A loose item with no bold key.
`;

describe("parseWhatsNew", () => {
  it("parses versions, dates, sections and items", () => {
    const releases = parseWhatsNew(SAMPLE);
    expect(releases.map((r) => r.version)).toEqual(["0.2.0", "0.1.3"]);
    const first = releases[0]!;
    expect(first.date).toBe("2026-10-01");
    expect(first.sections.map((s) => s.title)).toEqual(["Editor", "Releases"]);
    expect(first.sections[0]!.items).toEqual([
      { key: "Mermaid diagrams", body: "fences render inline." },
      { key: "GitHub alerts", body: "`> [!NOTE]` banners round-trip." },
    ]);
  });

  it("tolerates a missing date and items without a bold key", () => {
    const second = parseWhatsNew(SAMPLE)[1]!;
    expect(second.date).toBeNull();
    expect(second.sections[0]!.items).toEqual([
      { key: null, body: "A loose item with no bold key." },
    ]);
  });

  it("returns nothing for markdown with no version headings", () => {
    expect(parseWhatsNew("# What's New\n\nNothing yet.\n")).toEqual([]);
  });

  it("keeps items that appear before any ### section", () => {
    const releases = parseWhatsNew("## 1.0.0 — 2026-01-01\n\n- **Key**: body\n");
    expect(releases[0]!.sections).toEqual([
      { title: null, items: [{ key: "Key", body: "body" }] },
    ]);
  });

  it("parses the shipped WHATS_NEW.md", () => {
    const releases = parseWhatsNew(whatsNewMd);
    expect(releases.length).toBeGreaterThan(0);
    expect(releases[0]!.version).toMatch(/^\d+\.\d+\.\d+$/);
    expect(releases[0]!.sections.length).toBeGreaterThan(0);
  });
});

describe("compareVersions", () => {
  it("compares segments numerically, not lexically", () => {
    expect(compareVersions("0.1.10", "0.1.9")).toBeGreaterThan(0);
    expect(compareVersions("0.2.0", "0.10.0")).toBeLessThan(0);
    expect(compareVersions("1.0.0", "1.0.0")).toBe(0);
  });

  it("treats missing segments as zero and ignores pre-release suffixes", () => {
    expect(compareVersions("1.2", "1.2.0")).toBe(0);
    expect(compareVersions("1.2.3-beta.1", "1.2.3")).toBe(0);
    expect(compareVersions("2.0.0-rc1", "1.9.9")).toBeGreaterThan(0);
  });
});

describe("releasesToShow", () => {
  const releases = parseWhatsNew(SAMPLE);

  it("returns nothing when there is no lastSeen (first ever launch)", () => {
    expect(releasesToShow(releases, "0.2.0", null)).toEqual([]);
  });

  it("returns releases newer than lastSeen up to the current version", () => {
    expect(releasesToShow(releases, "0.2.0", "0.1.2").map((r) => r.version)).toEqual([
      "0.2.0",
      "0.1.3",
    ]);
    expect(releasesToShow(releases, "0.2.0", "0.1.3").map((r) => r.version)).toEqual([
      "0.2.0",
    ]);
  });

  it("returns nothing when lastSeen is already current", () => {
    expect(releasesToShow(releases, "0.2.0", "0.2.0")).toEqual([]);
  });

  it("never shows a release newer than the running version", () => {
    expect(releasesToShow(releases, "0.1.3", "0.1.2").map((r) => r.version)).toEqual([
      "0.1.3",
    ]);
  });

  it("sorts newest first even when the file is out of order", () => {
    const shuffled = [releases[1]!, releases[0]!];
    expect(releasesToShow(shuffled, "0.2.0", "0.1.0").map((r) => r.version)).toEqual([
      "0.2.0",
      "0.1.3",
    ]);
  });
});

describe("LAST_SEEN_KEY", () => {
  it("is namespaced to Ken", () => {
    expect(LAST_SEEN_KEY).toBe("ken.whatsNew.lastSeen");
  });
});
