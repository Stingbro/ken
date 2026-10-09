// Wires OS file drops on the file tree to the backend: copy everything under
// `dest`, refresh the tree, and report failures where move errors show.
import { api } from "../lib/api";
import { app } from "../lib/app.svelte";
import { drag } from "./dnd.svelte";
import { dropEntries, planImport, runImport } from "./externalDrop";

/**
 * Copy the files and folders of a drop into `dest` ("" = root). Call it from
 * the `drop` handler without awaiting first: the items are read synchronously,
 * before the DataTransfer is emptied.
 */
export async function importDrop(dt: DataTransfer, dest: string): Promise<void> {
  const entries = dropEntries(dt);
  drag.error = null;
  try {
    const plan = await planImport(entries);
    const { errors } = await runImport(plan, dest, {
      createFolder: (parent, name) => api.createUniqueFolder(parent, name),
      writeFile: async (dir, file) =>
        api.writeNewFile(dir, file.name, new Uint8Array(await file.arrayBuffer())),
    });
    if (errors.length === 1) drag.error = `Couldn't copy ${errors[0]}`;
    else if (errors.length > 1) {
      drag.error = `Couldn't copy ${errors.length} items — ${errors[0]}`;
    }
  } catch (e) {
    drag.error = String(e);
  }
  await app.refreshTree();
}
