import { describe, expect, it } from "vitest";
import type { SetupRepoRow } from "../lib/api";
import { indexSummary, mergeRows, noTeams, suggestedTeams, toggleKind } from "./setupFlow";

const row = (member: string, over: Partial<SetupRepoRow> = {}): SetupRepoRow => ({
  member,
  include: true,
  kind: [],
  team: "Realms",
  index: "entities",
  evidence: [],
  remote: null,
  existing: false,
  hasGit: true,
  path: `C:/Code/${member}`,
  description: "",
  ...over,
});

describe("setup flow", () => {
  it("keeps a person's edits when the picked set is proposed again", () => {
    const old = [row("docs", { description: "Our wiki", team: "Mine", include: false })];
    const fresh = [row("docs", { description: "From the README" }), row("game")];
    const merged = mergeRows(old, fresh);
    expect(merged.map((r) => [r.member, r.description, r.team, r.include])).toEqual([
      ["docs", "Our wiki", "Mine", false],
      ["game", "", "Realms", true],
    ]);
    expect(suggestedTeams(merged)).toEqual(["Realms"]);
  });

  it("toggles a kind and keeps the fixed order", () => {
    expect(toggleKind(["code"], "wiki")).toEqual(["wiki", "code"]);
    expect(toggleKind(["wiki", "code"], "wiki")).toEqual(["code"]);
  });

  it("Ken only clears every team", () => {
    expect(noTeams([row("a"), row("b")]).map((r) => r.team)).toEqual([null, null]);
  });

  it("counts excluded and off rows as not indexed", () => {
    const rows = [
      row("docs"),
      row("game", { index: "search" }),
      row("tools", { index: "search", include: false }),
      row("empty", { index: "off", include: false }),
    ];
    expect(indexSummary(rows)).toEqual({ entities: 1, search: 1, off: 2 });
  });
});
