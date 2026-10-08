// Naming rules for inline tree edits (§12): what a valid new name is, and the
// deduped default a new document gets. Pure so vitest covers the policy; the
// backend re-checks (move_file refuses overwrites, create_document dedupes) as
// the race-safety layer.

/** Names of the entries directly inside `folder` ("" = root): the distinct
 *  first segment of every path under the folder, so intermediate folders that
 *  only appear as ancestors of deeper paths are still listed. */
export function siblingNames(paths: string[], folder: string): string[] {
  const prefix = folder === "" ? "" : folder + "/";
  const out = new Set<string>();
  for (const p of paths) {
    if (!p.startsWith(prefix)) continue;
    const rest = p.slice(prefix.length);
    if (rest.length === 0) continue;
    const slash = rest.indexOf("/");
    out.add(slash >= 0 ? rest.slice(0, slash) : rest);
  }
  return [...out];
}

/** Human message for an invalid name, or null when valid. The duplicate check
 *  is case-insensitive — projects live on case-insensitive filesystems (macOS
 *  default, Dropbox/OneDrive). */
export function validateName(name: string, siblings: Iterable<string>): string | null {
  const trimmed = name.trim();
  if (trimmed.length === 0) return "Give it a name.";
  if (trimmed.includes("/")) return "Names can't contain “/”.";
  const lower = trimmed.toLowerCase();
  for (const s of siblings) {
    if (s.toLowerCase() === lower) return `“${trimmed}” already exists here.`;
  }
  return null;
}

/** "Untitled<ext>", counting up past collisions ("Untitled 2<ext>", …) so the
 *  prefilled default is already committable. Mirrors ken-core
 *  fsops::numbered_name, which the backend applies again. */
function dedupedUntitled(siblings: Iterable<string>, ext: string): string {
  const taken = new Set([...siblings].map((s) => s.toLowerCase()));
  if (!taken.has(`untitled${ext}`.toLowerCase())) return `Untitled${ext}`;
  for (let n = 2; ; n++) {
    const candidate = `Untitled ${n}${ext}`;
    if (!taken.has(candidate.toLowerCase())) return candidate;
  }
}

/** The prefilled name for a new document: "Untitled.md", "Untitled 2.md", … */
export function dedupedDocName(siblings: Iterable<string>): string {
  return dedupedUntitled(siblings, ".md");
}

/** The prefilled name for a new link (a .url internet shortcut):
 *  "Untitled.url", "Untitled 2.url", … */
export function dedupedLinkName(siblings: Iterable<string>): string {
  return dedupedUntitled(siblings, ".url");
}
