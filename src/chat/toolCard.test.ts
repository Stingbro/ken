import { describe, expect, it } from "vitest";
import { cardHits, parseToolCard } from "./toolCard";

describe("parseToolCard", () => {
  it("reads a card the engine stored", () => {
    const c = parseToolCard(
      JSON.stringify({ toolUseId: "t1", name: "Grep", summary: "Grep save", status: "error", result: "boom" }),
    );
    expect(c).toEqual({ toolUseId: "t1", name: "Grep", summary: "Grep save", status: "error", result: "boom" });
  });

  it("stops spinning once the chat is no longer working", () => {
    const raw = JSON.stringify({ toolUseId: "t1", name: "Read", summary: "Read a.md", status: "running" });
    expect(parseToolCard(raw, true)?.status).toBe("running");
    expect(parseToolCard(raw, false)?.status).toBe("done");
  });

  it("is null for anything that isn't a card", () => {
    expect(parseToolCard("Read a.md")).toBeNull();
    expect(parseToolCard(JSON.stringify({ name: "Read" }))).toBeNull();
  });
});

describe("cardHits", () => {
  it("lists a search's hits with each code hit's symbol beside its path", () => {
    const result = [
      "1. [code · service · 2026-10-01] game:src/world.ts:3 `saveWorld` (ken://a/src/world.ts#L3) [keyword]",
      "2. [decision D-410 · binding · 2026-10-06] [[DECISIONS]] · wiki:Decisions/DECISIONS.md:9 (ken://b/Decisions/DECISIONS.md#L9) [both]",
    ].join("\n");
    expect(cardHits(result)).toEqual([
      { place: "game:src/world.ts:3", symbol: "saveWorld" },
      { place: "[[DECISIONS]] · wiki:Decisions/DECISIONS.md:9", symbol: null },
    ]);
  });

  it("is empty for a result that lists no hits", () => {
    expect(cardHits("Read 40 lines")).toEqual([]);
    expect(cardHits(undefined)).toEqual([]);
  });
});
