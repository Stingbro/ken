import { describe, expect, it } from "vitest";
import {
  closeAll,
  closeOthers,
  closeRight,
  closeTab,
  makePersistent,
  openTab,
  renameTab,
  renameTabsForMove,
  setPinned,
  type TabState,
} from "./tabs";

const empty: TabState = { tabs: [], active: null };

describe("tab reducers", () => {
  it("opens a preview tab and replaces it on the next preview open", () => {
    let s = openTab(empty, "a.md", false);
    expect(s.tabs).toEqual([{ path: "a.md", pinned: false, preview: true }]);
    expect(s.active).toBe("a.md");

    // Second single-click replaces the preview tab in place.
    s = openTab(s, "b.md", false);
    expect(s.tabs.map((t) => t.path)).toEqual(["b.md"]);
    expect(s.active).toBe("b.md");
  });

  it("keeps persistent tabs and appends new ones", () => {
    let s = openTab(empty, "a.md", true);
    s = openTab(s, "b.md", true);
    expect(s.tabs.map((t) => t.path)).toEqual(["a.md", "b.md"]);
    expect(s.tabs.every((t) => !t.preview)).toBe(true);
  });

  it("promotes a preview tab to persistent", () => {
    let s = openTab(empty, "a.md", false);
    s = makePersistent(s, "a.md");
    expect(s.tabs[0].preview).toBe(false);
    // A later preview open then does NOT replace it.
    s = openTab(s, "b.md", false);
    expect(s.tabs.map((t) => t.path)).toEqual(["a.md", "b.md"]);
  });

  it("re-opening an existing tab persistently clears its preview flag", () => {
    let s = openTab(empty, "a.md", false);
    s = openTab(s, "a.md", true);
    expect(s.tabs).toEqual([{ path: "a.md", pinned: false, preview: false }]);
  });

  it("activates the neighbor when closing the active tab", () => {
    let s = openTab(empty, "a.md", true);
    s = openTab(s, "b.md", true);
    s = openTab(s, "c.md", true);
    s = { ...s, active: "b.md" };
    s = closeTab(s, "b.md");
    expect(s.tabs.map((t) => t.path)).toEqual(["a.md", "c.md"]);
    expect(s.active).toBe("c.md"); // right neighbor
  });

  it("pins tabs leftmost and never auto-replaces them", () => {
    let s = openTab(empty, "a.md", true);
    s = openTab(s, "b.md", true);
    s = setPinned(s, "b.md", true);
    expect(s.tabs.map((t) => t.path)).toEqual(["b.md", "a.md"]);
    expect(s.tabs[0].pinned).toBe(true);
  });

  it("close others spares pinned tabs and the target", () => {
    let s = openTab(empty, "a.md", true);
    s = openTab(s, "b.md", true);
    s = openTab(s, "c.md", true);
    s = setPinned(s, "a.md", true);
    s = closeOthers(s, "c.md");
    expect(s.tabs.map((t) => t.path).sort()).toEqual(["a.md", "c.md"]);
    expect(s.active).toBe("c.md");
  });

  it("close to the right leaves the target and everything before it", () => {
    let s = openTab(empty, "a.md", true);
    s = openTab(s, "b.md", true);
    s = openTab(s, "c.md", true);
    s = openTab(s, "d.md", true);
    s = closeRight(s, "b.md");
    expect(s.tabs.map((t) => t.path)).toEqual(["a.md", "b.md"]);
    // The active tab was closed, so the target takes over.
    expect(s.active).toBe("b.md");
  });

  it("close to the right spares pinned tabs and keeps a surviving active", () => {
    let s = openTab(empty, "a.md", true);
    s = openTab(s, "b.md", true);
    s = openTab(s, "c.md", true);
    s = setPinned(s, "c.md", true); // pinned tabs move leftmost
    s = { ...s, active: "c.md" };
    s = closeRight(s, "a.md");
    expect(s.tabs.map((t) => t.path)).toEqual(["c.md", "a.md"]);
    expect(s.active).toBe("c.md");
  });

  it("close to the right is a no-op for the last tab and for unknown paths", () => {
    let s = openTab(empty, "a.md", true);
    s = openTab(s, "b.md", true);
    expect(closeRight(s, "b.md")).toEqual(s);
    expect(closeRight(s, "gone.md")).toBe(s);
  });

  it("close all clears everything but pinned tabs", () => {
    let s = openTab(empty, "a.md", true);
    s = openTab(s, "b.md", true);
    s = openTab(s, "c.md", true);
    s = closeAll(s);
    expect(s.tabs).toEqual([]);
    expect(s.active).toBe(null);

    let p = openTab(empty, "a.md", true);
    p = openTab(p, "b.md", true);
    p = setPinned(p, "a.md", true);
    p = { ...p, active: "b.md" };
    p = closeAll(p);
    expect(p.tabs.map((t) => t.path)).toEqual(["a.md"]);
    expect(p.active).toBe("a.md");
  });

  it("close all keeps the active tab when it is pinned", () => {
    let s = openTab(empty, "a.md", true);
    s = openTab(s, "b.md", true);
    s = setPinned(s, "a.md", true);
    s = setPinned(s, "b.md", true);
    s = { ...s, active: "b.md" };
    s = closeAll(s);
    expect(s.tabs.map((t) => t.path)).toEqual(["a.md", "b.md"]);
    expect(s.active).toBe("b.md");
  });

  it("rewrites a tab path and active on move", () => {
    let s = openTab(empty, "a.md", true);
    s = renameTab(s, "a.md", "sub/a.md");
    expect(s.tabs[0].path).toBe("sub/a.md");
    expect(s.active).toBe("sub/a.md");
  });
});

describe("renameTabsForMove", () => {
  it("renames an exact file move, including the active path", () => {
    let s = openTab(empty, "a.md", true);
    s = renameTabsForMove(s, "a.md", "sub/a.md");
    expect(s.tabs.map((t) => t.path)).toEqual(["sub/a.md"]);
    expect(s.active).toBe("sub/a.md");
  });

  it("renames every tab under a moved folder's prefix", () => {
    let s = openTab(empty, "Meetings/a.md", true);
    s = openTab(s, "Meetings/2026/b.md", true);
    s = openTab(s, "Research/c.md", true);
    s = renameTabsForMove(s, "Meetings", "Archive/Meetings");
    expect(s.tabs.map((t) => t.path)).toEqual([
      "Archive/Meetings/a.md",
      "Archive/Meetings/2026/b.md",
      "Research/c.md",
    ]);
    expect(s.active).toBe("Research/c.md");
  });

  it("does not touch a sibling sharing the name prefix", () => {
    let s = openTab(empty, "Meetings Archive/x.md", true);
    s = renameTabsForMove(s, "Meetings", "Old");
    expect(s.tabs[0].path).toBe("Meetings Archive/x.md");
  });
});
