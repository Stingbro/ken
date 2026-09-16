import { describe, expect, it } from "vitest";
import { parseSlashShortcut, SLASH_SHORTCUT_RE } from "./slashShortcuts";

describe("parseSlashShortcut", () => {
  it("parses every heading level", () => {
    for (let level = 1; level <= 6; level++) {
      expect(parseSlashShortcut(`/h${level} `)).toEqual({
        type: "heading",
        level,
      });
    }
  });

  it("parses the simple block types", () => {
    expect(parseSlashShortcut("/quote ")).toEqual({ type: "quote" });
    expect(parseSlashShortcut("/code ")).toEqual({ type: "code" });
    expect(parseSlashShortcut("/divider ")).toEqual({ type: "divider" });
    expect(parseSlashShortcut("/bullet ")).toEqual({ type: "bulletList" });
    expect(parseSlashShortcut("/number ")).toEqual({ type: "orderedList" });
    expect(parseSlashShortcut("/todo ")).toEqual({ type: "taskList" });
  });

  it("parses the five alert kinds", () => {
    for (const kind of [
      "note",
      "tip",
      "important",
      "warning",
      "caution",
    ] as const) {
      expect(parseSlashShortcut(`/${kind} `)).toEqual({ type: "alert", kind });
    }
  });

  it("is case-insensitive", () => {
    expect(parseSlashShortcut("/H3 ")).toEqual({ type: "heading", level: 3 });
    expect(parseSlashShortcut("/Quote ")).toEqual({ type: "quote" });
  });

  it("requires the trailing space", () => {
    expect(parseSlashShortcut("/h1")).toBeNull();
    expect(parseSlashShortcut("/quote")).toBeNull();
  });

  it("rejects unknown keywords", () => {
    expect(parseSlashShortcut("/h7 ")).toBeNull();
    expect(parseSlashShortcut("/heading ")).toBeNull();
    expect(parseSlashShortcut("/ ")).toBeNull();
    expect(parseSlashShortcut("")).toBeNull();
  });

  it("rejects text around the shortcut", () => {
    expect(parseSlashShortcut("hello /h1 ")).toBeNull();
    expect(parseSlashShortcut("/h1 x ")).toBeNull();
    expect(parseSlashShortcut(" /h1 ")).toBeNull();
  });

  it("exposes a regex anchored to a whole block", () => {
    expect(SLASH_SHORTCUT_RE.test("/h1 ")).toBe(true);
    expect(SLASH_SHORTCUT_RE.test("a/h1 ")).toBe(false);
  });
});
