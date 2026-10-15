import { describe, it, expect } from "vitest";
import { canReadNow, commitLine, ingestBusy, pageName, raidDetail, rawActions, rawStateLabel, stagingLine, writtenLine } from "./ingestCard";
import type { RawSource } from "./api";

describe("writtenLine", () => {
  it("counts the pages, then what waits", () => {
    expect(writtenLine(2, 3)).toBe("2 pages written · 3 wait");
    expect(writtenLine(1, 1)).toBe("1 page written · 1 waits");
  });
  it("says when nothing was written", () => {
    expect(writtenLine(0, 0)).toBe("nothing written");
    expect(writtenLine(0, 4)).toBe("nothing written · 4 wait");
  });
});

describe("pageName", () => {
  it("reads a path as sections", () => {
    expect(pageName("Current/Project.md")).toBe("Current › Project");
    expect(pageName("tickets/SR-031.md")).toBe("tickets › SR-031");
    expect(pageName("Vocabulary.md")).toBe("Vocabulary");
  });
});

describe("stagingLine", () => {
  it("says none or how many were held", () => {
    expect(stagingLine(0)).toBe("No page edit was held for you.");
    expect(stagingLine(2)).toBe("2 held for you, above.");
  });
});

describe("commitLine", () => {
  it("names the commit git takes back, or why there is none", () => {
    expect(commitLine("abc1234", null)).toBe("One commit, abc1234, in the knowledge base. git revert abc1234 takes it all back.");
    expect(commitLine(null, "git commit failed: no identity")).toBe("Not committed: git commit failed: no identity");
    expect(commitLine(null, null)).toContain("not a git repo");
  });
});

describe("Raw rows", () => {
  it("say where each source is", () => {
    expect(rawStateLabel("queued")).toBe("in the queue");
    expect(rawStateLabel("transcribing")).toBe("transcribing");
    expect(rawStateLabel("waiting")).toBe("waiting for a transcript");
    expect(rawStateLabel("failed")).toBe("could not read");
  });
  it("offer Try again and Remove only where they make sense", () => {
    expect(rawActions("failed")).toEqual(["retry", "remove"]);
    expect(rawActions("waiting")).toEqual(["retry", "remove"]);
    expect(rawActions("queued")).toEqual(["remove"]);
    expect(rawActions("reading")).toEqual([]);
    expect(rawActions("transcribing")).toEqual([]);
    expect(rawActions("read")).toEqual([]);
  });
});

describe("Read now", () => {
  const raw = (state: RawSource["state"]) => ({ state }) as RawSource;
  it("shows when something is queued and no pass runs", () => {
    expect(canReadNow({ running: false, raw: [raw("queued")] })).toBe(true);
    expect(canReadNow({ running: true, raw: [raw("queued")] })).toBe(false);
    expect(canReadNow({ running: false, raw: [raw("failed")] })).toBe(false);
    expect(canReadNow(null)).toBe(false);
  });
  it("keeps the screen reading while anything moves", () => {
    expect(ingestBusy({ running: false, raw: [raw("transcribing")] })).toBe(true);
    expect(ingestBusy({ running: false, raw: [raw("waiting"), raw("read")] })).toBe(false);
  });
});

describe("raidDetail", () => {
  it("reads the text ingest wrote for a risk or issue", () => {
    expect(raidDetail(JSON.stringify({ raid: { kind: "risk", title: "Friday may slip", text: " The build waits on QA. " } }))).toBe(
      "The build waits on QA.",
    );
  });

  it("is empty for anything else", () => {
    expect(raidDetail(JSON.stringify({ append: { ruling: "x" } }))).toBe("");
    expect(raidDetail(null)).toBe("");
    expect(raidDetail("not json")).toBe("");
  });
});
