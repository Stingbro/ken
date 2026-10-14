import { describe, expect, it } from "vitest";
import { controlLine, controlResult, editorLines, findingLabel, kindlessRepos, orderFindings, parseControls, proposedKinds } from "./workspaceSettings";
import type { Finding, RepoKind } from "./api";

const finding = (kind: string, title = kind): Finding => ({ kind, title, detail: "", path: null, projectId: null });

describe("findingLabel", () => {
  it("names each kind plainly", () => {
    expect(findingLabel("drift")).toBe("drift");
    expect(findingLabel("aged")).toBe("not checked");
    expect(findingLabel("orphan")).toBe("links");
    expect(findingLabel("broken-link")).toBe("links");
    expect(findingLabel("unknown-ruling")).toBe("ruling");
    expect(findingLabel("suggested-page")).toBe("page to write");
    expect(findingLabel("question")).toBe("question");
    expect(findingLabel("")).toBe("finding");
  });
});

describe("orderFindings", () => {
  it("puts drift first and keeps the order within a kind", () => {
    const list = [
      finding("orphan", "o"),
      finding("contradiction", "c1"),
      finding("drift", "d"),
      finding("question", "q"),
      finding("contradiction", "c2"),
      finding("supersession", "s"),
      finding("aged", "a"),
    ];
    expect(orderFindings(list).map((f) => f.title)).toEqual(["d", "a", "s", "c1", "c2", "o", "q"]);
  });
});

describe("kindlessRepos", () => {
  it("lists the repos with no kind that are still there", () => {
    const repos = [
      { id: "a", kind: [] as RepoKind[], available: true },
      { id: "b", kind: ["code"] as RepoKind[], available: true },
      { id: "c", kind: [] as RepoKind[], available: false },
    ];
    expect(kindlessRepos(repos).map((r) => r.id)).toEqual(["a"]);
  });
});

describe("proposedKinds", () => {
  it("matches a proposal row by folder, then by name", () => {
    const repos = [
      { id: "1", name: "ATT-Wiki", path: "C:\\ken-eval\\ATT\\ATT-Wiki" },
      { id: "2", name: "att-opmodel", path: "C:/elsewhere/att-opmodel" },
      { id: "3", name: "notes", path: "C:/notes" },
    ];
    const rows = [
      { member: "ATT-Wiki", path: "C:/ken-eval/ATT/ATT-Wiki/", kind: ["wiki"] as RepoKind[] },
      { member: "att-opmodel", path: null, kind: ["code"] as RepoKind[] },
      { member: "notes", path: "C:/notes", kind: [] as RepoKind[] },
    ];
    expect(proposedKinds(repos, rows)).toEqual({ "1": ["wiki"], "2": ["code"] });
  });
});

describe("editorLines", () => {
  it("trims each line and drops the blank ones", () => {
    expect(editorLines(" Rules/ = binding \r\n\n  \nResearch/ = record")).toEqual(["Rules/ = binding", "Research/ = record"]);
    expect(editorLines("")).toEqual([]);
  });
});

describe("controls", () => {
  it("reads question | file | source lines, and says which it could not", () => {
    const { controls, bad } = parseControls(
      "how are worlds saved | src/Save.java | Game\nwhere do rules live | Rules/Index.md\njust a question\n | x.md",
    );
    expect(controls).toEqual([
      { question: "how are worlds saved", expect: "src/Save.java", source: "Game" },
      { question: "where do rules live", expect: "Rules/Index.md", source: null },
    ]);
    expect(bad).toEqual(["just a question", "| x.md"]);
  });

  it("writes a control back as the editor reads it", () => {
    const c = { question: "how are worlds saved", expect: "src/Save.java", source: "Game" };
    expect(controlLine(c)).toBe("how are worlds saved | src/Save.java | Game");
    expect(controlLine({ ...c, source: null })).toBe("how are worlds saved | src/Save.java");
    expect(parseControls(controlLine(c)).controls).toEqual([c]);
  });

  it("says how a control did on its last run", () => {
    expect(controlResult({ ran: false, rank: null, passed: false })).toBe("not run yet");
    expect(controlResult({ ran: true, rank: 2, passed: true })).toBe("passes, at 2");
    expect(controlResult({ ran: true, rank: null, passed: false })).toBe("fails: not in the top 5");
  });
});
