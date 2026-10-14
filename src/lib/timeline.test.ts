import { describe, expect, it } from "vitest";
import type { RaidEntry } from "./api";
import { filterLog, groupByDate, highlightMatches, kindLabel, kindsIn } from "./timeline";

function entry(date: string, kind: string, text: string, more: Partial<RaidEntry> = {}): RaidEntry {
  return { date, at: null, kind, text, repo: null, path: null, line: null, reference: null, ...more };
}

// Newest first, as `raid_log` returns them.
const log: RaidEntry[] = [
  entry("2026-10-06", "finding", "orphan resolved: Nothing links to Design/Lonely.md.", { at: 1, path: "Design/Lonely.md" }),
  entry("2026-10-06", "commit", "Fix SR-101", { repo: "game", reference: "1a2b3c4" }),
  entry("2026-10-04", "ticket", "SR-101 · done: Fix saves", { path: "tickets/SR-101.md" }),
  entry("2026-10-02", "ruling", "D-012: Regions save apart.", { path: "Decisions/DECISIONS.md", line: 3 }),
  entry("2026-10-01", "ingest", "Standup", { path: "log.md", line: 3 }),
  entry("2026-09-30", "rename", "a kind log.md names"),
];

describe("kindsIn", () => {
  it("lists the kinds present, the known ones in chip order first", () => {
    expect(kindsIn(log)).toEqual(["ingest", "ruling", "ticket", "commit", "finding", "rename"]);
    expect(kindsIn([])).toEqual([]);
  });

  it("labels the known kinds and keeps the rest", () => {
    expect(kindLabel("ruling")).toBe("Rulings");
    expect(kindLabel("rename")).toBe("rename");
  });
});

describe("filterLog", () => {
  it("keeps one kind, and the order", () => {
    expect(filterLog(log, "ticket", "").map((e) => e.text)).toEqual(["SR-101 · done: Fix saves"]);
    expect(filterLog(log, null, "")).toEqual(log);
  });

  it("matches words in the text, the repo, the reference and the path", () => {
    expect(filterLog(log, null, "sr-101").map((e) => e.kind)).toEqual(["commit", "ticket"]);
    expect(filterLog(log, null, "GAME").map((e) => e.kind)).toEqual(["commit"]);
    expect(filterLog(log, null, "1a2b").map((e) => e.kind)).toEqual(["commit"]);
    expect(filterLog(log, null, "decisions.md").map((e) => e.kind)).toEqual(["ruling"]);
    expect(filterLog(log, "ruling", "regions")).toHaveLength(1);
    expect(filterLog(log, "ticket", "regions")).toHaveLength(0);
  });
});

describe("groupByDate", () => {
  it("groups by day, newest first, keeping each day's order", () => {
    const groups = groupByDate(log);
    expect(groups.map((g) => g.date)).toEqual(["2026-10-06", "2026-10-04", "2026-10-02", "2026-10-01", "2026-09-30"]);
    expect(groups[0].entries.map((e) => e.kind)).toEqual(["finding", "commit"]);
    expect(groupByDate([])).toEqual([]);
  });
});

describe("highlightMatches", () => {
  it("marks case-insensitive matches", () => {
    expect(highlightMatches("The Cutover slipped", "cutover")).toBe("The <mark>Cutover</mark> slipped");
  });

  it("escapes HTML and never executes it", () => {
    expect(highlightMatches("<img src=x> & cutover", "cutover")).toBe("&lt;img src=x&gt; &amp; <mark>cutover</mark>");
    // A query matching escaped-looking text stays escaped around the mark.
    expect(highlightMatches("<b>bold</b>", "bold")).toBe("&lt;b&gt;<mark>bold</mark>&lt;/b&gt;");
  });

  it("empty query just escapes", () => {
    expect(highlightMatches("a < b", "  ")).toBe("a &lt; b");
  });

  it("marks every occurrence", () => {
    expect(highlightMatches("go go go", "go")).toBe("<mark>go</mark> <mark>go</mark> <mark>go</mark>");
  });
});
