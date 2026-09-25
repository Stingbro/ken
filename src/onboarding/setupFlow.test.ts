import { describe, expect, it } from "vitest";
import type { SetupRepoRow } from "../lib/api";
import {
  coveredRepos,
  defaultWikiChoice,
  indexSummary,
  mergeRows,
  newWikiPath,
  noTeams,
  suggestedTeams,
  teamWikis,
  toggleKind,
  wikiChoicesReady,
} from "./setupFlow";

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

  it("a team uses its wiki repo, or none until a person creates one", () => {
    const rows = [row("docs", { kind: ["wiki"] }), row("game", { kind: ["code"], description: "The client" }), row("ops", { team: "Ops" })];
    expect(teamWikis(rows, "Realms").map((r) => r.member)).toEqual(["docs"]);
    expect(defaultWikiChoice(rows, "Realms")).toEqual({ mode: "existing", member: "docs" });
    expect(defaultWikiChoice(rows, "Ops")).toEqual({ mode: "none" });
    expect(coveredRepos(rows, "Realms")).toEqual([{ name: "game", description: "The client" }]);
  });

  it("a new wiki needs a folder and a name", () => {
    expect(newWikiPath("C:\\Code\\", "Realms-Wiki")).toBe("C:\\Code\\Realms-Wiki");
    expect(newWikiPath("/home/me/code", " Realms-Wiki ")).toBe("/home/me/code/Realms-Wiki");
    expect(wikiChoicesReady({ A: { mode: "new", parent: null, name: "W" } })).toBe(false);
    expect(wikiChoicesReady({ A: { mode: "new", parent: "C:/x", name: "W" }, B: { mode: "none" } })).toBe(true);
  });
});
