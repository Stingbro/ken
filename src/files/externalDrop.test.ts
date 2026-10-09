import { describe, expect, it } from "vitest";

import {
  dropFolderFor,
  hasFiles,
  planImport,
  runImport,
  type DropEntry,
  type ImportOps,
} from "./externalDrop";

const file = (name: string): DropEntry => ({
  name,
  isDirectory: false,
  file: async () => new File(["x"], name),
});
const dir = (name: string, children: DropEntry[]): DropEntry => ({
  name,
  isDirectory: true,
  children: async () => children,
});

describe("hasFiles", () => {
  it("is true only for drags carrying OS files", () => {
    expect(hasFiles({ types: ["Files"] })).toBe(true);
    expect(hasFiles({ types: ["text/plain", "Files"] })).toBe(true);
    expect(hasFiles({ types: ["text/plain"] })).toBe(false);
    expect(hasFiles(null)).toBe(false);
    expect(hasFiles(undefined)).toBe(false);
  });
});

describe("dropFolderFor", () => {
  it("drops into a folder row's own folder", () => {
    expect(dropFolderFor({ relPath: "Notes/2026", isFolder: true })).toBe("Notes/2026");
  });

  it("drops beside a file row, in its parent folder", () => {
    expect(dropFolderFor({ relPath: "Notes/plan.md", isFolder: false })).toBe("Notes");
    expect(dropFolderFor({ relPath: "plan.md", isFolder: false })).toBe("");
  });

  it("drops into the project root for the empty area", () => {
    expect(dropFolderFor(null)).toBe("");
  });

  it("refuses an excluded folder, as moves do", () => {
    expect(dropFolderFor({ relPath: "archive", isFolder: true, excluded: true })).toBeNull();
  });
});

describe("planImport", () => {
  it("lists loose files at the top level", async () => {
    const plan = await planImport([file("a.png"), file("b.pdf")]);
    expect(plan.dirs).toEqual([]);
    expect(plan.files.map((f) => [f.dir, f.file.name])).toEqual([
      ["", "a.png"],
      ["", "b.pdf"],
    ]);
  });

  it("walks folders parents-first", async () => {
    const plan = await planImport([
      dir("Photos", [file("x.jpg"), dir("2024", [file("y.jpg")]), dir("empty", [])]),
      file("top.txt"),
    ]);
    expect(plan.dirs).toEqual(["Photos", "Photos/2024", "Photos/empty"]);
    expect(plan.files.map((f) => [f.dir, f.file.name])).toEqual([
      ["Photos", "x.jpg"],
      ["Photos/2024", "y.jpg"],
      ["", "top.txt"],
    ]);
  });
});

/** Fake backend: dedupes folders like the real one and records writes. */
function fakeOps(existing: string[] = []) {
  const folders = new Set(existing);
  const writes: string[] = [];
  const ops: ImportOps = {
    async createFolder(parent, name) {
      const join = (n: string) => (parent ? `${parent}/${n}` : n);
      let n = 0;
      while (folders.has(join(n ? `${name}-${n}` : name))) n++;
      const rel = join(n ? `${name}-${n}` : name);
      folders.add(rel);
      return rel;
    },
    async writeFile(dirRel, f) {
      if (f.name === "boom.bin") throw new Error("disk full");
      const rel = dirRel ? `${dirRel}/${f.name}` : f.name;
      writes.push(rel);
      return rel;
    },
  };
  return { ops, writes, folders };
}

describe("runImport", () => {
  it("writes loose files into the destination folder", async () => {
    const { ops, writes } = fakeOps();
    const plan = await planImport([file("a.png")]);
    const result = await runImport(plan, "Notes", ops);
    expect(writes).toEqual(["Notes/a.png"]);
    expect(result.errors).toEqual([]);
  });

  it("never merges into an existing folder: the copy gets its own name", async () => {
    const { ops, writes, folders } = fakeOps(["Notes/Photos"]);
    const plan = await planImport([dir("Photos", [file("x.jpg"), dir("2024", [file("y.jpg")])])]);
    await runImport(plan, "Notes", ops);
    expect(folders.has("Notes/Photos-1")).toBe(true);
    expect(folders.has("Notes/Photos-1/2024")).toBe(true);
    expect(writes).toEqual(["Notes/Photos-1/x.jpg", "Notes/Photos-1/2024/y.jpg"]);
  });

  it("keeps going past a failure and reports it", async () => {
    const { ops, writes } = fakeOps();
    const plan = await planImport([file("boom.bin"), file("ok.txt")]);
    const result = await runImport(plan, "", ops);
    expect(writes).toEqual(["ok.txt"]);
    expect(result.errors).toHaveLength(1);
    expect(result.errors[0]).toContain("boom.bin");
  });
});
