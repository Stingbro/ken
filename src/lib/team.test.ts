import { describe, expect, it } from "vitest";
import { findingLabel, kindlessRepos, orderFindings, proposedKinds } from "./team";
import type { RepoKind, TeamFinding } from "./api";

const finding = (kind: string, title = kind): TeamFinding => ({ kind, title, detail: "", path: null, projectId: null });

describe("findingLabel", () => {
  it("names each kind plainly", () => {
    expect(findingLabel("drift")).toBe("drift");
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
    ];
    expect(orderFindings(list).map((f) => f.title)).toEqual(["d", "s", "c1", "c2", "o", "q"]);
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
