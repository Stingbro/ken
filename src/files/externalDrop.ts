// Copying files and folders dragged in from the OS into the file tree. WebKit
// hands over `File` objects without paths, so everything is copied as bytes:
// folders are walked through `webkitGetAsEntry()`, recreated under the drop
// target, and filled file by file. Pure apart from the injected backend ops,
// so vitest covers target resolution, the walk and the no-merge rule.

/** Whether a drag carries OS files (as opposed to an in-app drag). */
export function hasFiles(dt: { types: ArrayLike<string> } | null | undefined): boolean {
  return !!dt && Array.from(dt.types).includes("Files");
}

/** What the drop landed on; `null` is the tree's empty area. */
export interface TreeDropTarget {
  relPath: string;
  isFolder: boolean;
  excluded?: boolean;
}

/**
 * The folder a drop copies into ("" = project root): a folder row is its own
 * folder, a file row its parent, the empty area the root. Excluded folders
 * refuse imports exactly as they refuse moves — Ken doesn't index them, so a
 * copy there would vanish from view. Null means "not droppable".
 */
export function dropFolderFor(target: TreeDropTarget | null): string | null {
  if (!target) return "";
  if (target.isFolder) return target.excluded ? null : target.relPath;
  const i = target.relPath.lastIndexOf("/");
  return i >= 0 ? target.relPath.slice(0, i) : "";
}

/** A dropped item, abstracted from WebKit's FileSystemEntry for testing. */
export interface DropEntry {
  name: string;
  isDirectory: boolean;
  file?: () => Promise<File>;
  children?: () => Promise<DropEntry[]>;
}

/** Everything to create, as paths relative to the drop folder: folders
 *  parents-first, then each file with the folder it goes in ("" = top). */
export interface ImportPlan {
  dirs: string[];
  files: { dir: string; file: File }[];
}

const join = (dir: string, name: string) => (dir ? `${dir}/${name}` : name);

export async function planImport(entries: DropEntry[]): Promise<ImportPlan> {
  const plan: ImportPlan = { dirs: [], files: [] };
  const walk = async (list: DropEntry[], dir: string) => {
    for (const entry of list) {
      if (entry.isDirectory) {
        const path = join(dir, entry.name);
        plan.dirs.push(path);
        await walk((await entry.children?.()) ?? [], path);
      } else if (entry.file) {
        plan.files.push({ dir, file: await entry.file() });
      }
    }
  };
  await walk(entries, "");
  return plan;
}

/** Backend calls an import needs; both return the project path created. */
export interface ImportOps {
  /** Create a NEW folder `name` in `parentRel`, deduped, never merged. */
  createFolder(parentRel: string, name: string): Promise<string>;
  /** Write `file` as a new file in `dirRel`, deduped, never overwriting. */
  writeFile(dirRel: string, file: File): Promise<string>;
}

/**
 * Carry out `plan` under `dest`. A dropped folder whose name is taken gets a
 * fresh one (`Photos-1`) rather than merging into the existing folder, and
 * everything inside follows it. Failures don't stop the rest; they come back
 * as messages.
 */
export async function runImport(
  plan: ImportPlan,
  dest: string,
  ops: ImportOps,
): Promise<{ written: string[]; errors: string[] }> {
  const written: string[] = [];
  const errors: string[] = [];
  // Plan-relative folder → the project path it was actually created at.
  const made = new Map<string, string>([["", dest]]);
  for (const dir of plan.dirs) {
    const i = dir.lastIndexOf("/");
    const parent = made.get(i >= 0 ? dir.slice(0, i) : "");
    if (parent === undefined) continue; // its parent failed; already reported
    try {
      made.set(dir, await ops.createFolder(parent, dir.slice(i + 1)));
    } catch (e) {
      errors.push(`${dir}: ${e}`);
    }
  }
  for (const { dir, file } of plan.files) {
    const target = made.get(dir);
    if (target === undefined) continue;
    try {
      written.push(await ops.writeFile(target, file));
    } catch (e) {
      errors.push(`${join(dir, file.name)}: ${e}`);
    }
  }
  return { written, errors };
}

/** Wrap a WebKit FileSystemEntry. */
function fromEntry(entry: FileSystemEntry): DropEntry {
  if (entry.isDirectory) {
    const reader = (entry as FileSystemDirectoryEntry).createReader();
    return {
      name: entry.name,
      isDirectory: true,
      children: async () => {
        // readEntries hands back a batch at a time; empty means done.
        const all: FileSystemEntry[] = [];
        for (;;) {
          const batch = await new Promise<FileSystemEntry[]>((resolve, reject) =>
            reader.readEntries(resolve, reject),
          );
          if (batch.length === 0) break;
          all.push(...batch);
        }
        return all.map(fromEntry);
      },
    };
  }
  return {
    name: entry.name,
    isDirectory: false,
    file: () =>
      new Promise<File>((resolve, reject) =>
        (entry as FileSystemFileEntry).file(resolve, reject),
      ),
  };
}

/**
 * The dropped items. Must run synchronously inside the `drop` handler: the
 * DataTransfer is emptied once the event returns. Falls back to plain files
 * where entries aren't available.
 */
export function dropEntries(dt: DataTransfer): DropEntry[] {
  const entries: DropEntry[] = [];
  for (const item of Array.from(dt.items ?? [])) {
    if (item.kind !== "file") continue;
    const entry = item.webkitGetAsEntry?.();
    if (entry) entries.push(fromEntry(entry));
  }
  if (entries.length) return entries;
  return Array.from(dt.files).map((f) => ({
    name: f.name,
    isDirectory: false,
    file: async () => f,
  }));
}
