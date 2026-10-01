import { describe, it, expect } from "vitest";
import { pageName, sourcesWaiting, stagingLine, writtenLine } from "./ingestCard";
import type { InboxItem } from "./api";

describe("writtenLine", () => {
  it("counts pages, ideas and escalations, then what waits", () => {
    expect(writtenLine(["edit", "page", "escalation"], 3)).toBe("2 pages, 1 escalation written · 3 wait");
    expect(writtenLine(["edit", "idea", "idea"], 1)).toBe("1 page, 2 ideas written · 1 waits");
  });
  it("names the items on my day", () => {
    expect(writtenLine(["edit", "task"], 0)).toBe("1 page written, 1 on my day");
    expect(writtenLine(["task"], 2)).toBe("1 on my day · 2 wait");
  });
  it("says when nothing was written", () => {
    expect(writtenLine([], 0)).toBe("nothing written");
    expect(writtenLine([], 4)).toBe("nothing written · 4 wait");
  });
});

describe("pageName", () => {
  it("reads a path as sections", () => {
    expect(pageName("Current/Project.md")).toBe("Current › Project");
    expect(pageName("ideas/I-031.md")).toBe("ideas › I-031");
    expect(pageName("Vocabulary.md")).toBe("Vocabulary");
  });
});

describe("stagingLine", () => {
  it("says none or how many were held", () => {
    expect(stagingLine(0)).toBe("Every page edit went through staging; none was held.");
    expect(stagingLine(2)).toBe("2 held, below.");
  });
});

describe("sourcesWaiting", () => {
  const item = (id: string, kind: InboxItem["kind"], payload: string | null): InboxItem => ({
    id,
    kind,
    title: "",
    body: "",
    when: 0,
    sourceRef: "",
    payload,
  });
  it("counts distinct notes with something waiting", () => {
    const items = [
      item("stored-1", "page-proposal", JSON.stringify({ page: "a", base: "", proposed: "", from: "n1.md" })),
      item("stored-2", "page-proposal", JSON.stringify({ page: "b", base: "", proposed: "", from: "n1.md" })),
      item("stored-3", "page-proposal", JSON.stringify({ page: "c", base: "", proposed: "", from: "n2.md" })),
      item("stored-4", "page-proposal", JSON.stringify({ page: "d", base: "", proposed: "" })),
      item("stored-5", "ingest", null),
      item("stored-6", "page-proposal", "not json"),
    ];
    expect(sourcesWaiting(items)).toBe(2);
    expect(sourcesWaiting([])).toBe(0);
  });
});
