import { describe, expect, it } from "vitest";
import { linkAt, resolveWikiLink } from "./wikiLinks";

const files = [
  "START-HERE.md",
  "Current/Index.md",
  "Current/Project.md",
  "Current/Team.md",
  "Current/Who-Does-What.md",
  "Research/Ingestion/Index.md",
  "Ways-of-Working/Agents/Index.md",
  "Conventions/ARCHITECTURE.md",
  "notes.txt",
];

describe("wiki links", () => {
  it("a bare name is the page of that name, in the linking page's folder first", () => {
    expect(resolveWikiLink("Project", "Current/Index.md", files)).toBe("Current/Project.md");
    expect(resolveWikiLink("team|the team", "START-HERE.md", files)).toBe("Current/Team.md");
    expect(resolveWikiLink("Index", "Research/Ingestion/Index.md", files)).toBe("Research/Ingestion/Index.md");
    expect(resolveWikiLink("Index#Pages", "START-HERE.md", files)).toBe("Current/Index.md");
    expect(resolveWikiLink("Nowhere", "START-HERE.md", files)).toBeNull();
  });

  it("a path is matched from the root, then from the page's folder", () => {
    expect(resolveWikiLink("Conventions/ARCHITECTURE", "START-HERE.md", files)).toBe("Conventions/ARCHITECTURE.md");
    expect(resolveWikiLink("Agents/Index", "Ways-of-Working/Lifecycle.md", files)).toBe("Ways-of-Working/Agents/Index.md");
  });

  it("finds the link under a click", () => {
    const text = "see [[Project]] and [[Team|the team]].";
    expect(linkAt(text, 6)).toBe("Project");
    expect(linkAt(text, 22)).toBe("Team|the team");
    expect(linkAt(text, 1)).toBeNull();
  });
});
