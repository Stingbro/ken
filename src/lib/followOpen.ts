// Whether the Files tree follows the open file — selecting a tab expands the
// tree to it. On by default: most people expect the tree to say where they are.
// localStorage-backed like the sidebar width, so it survives reloads without
// touching project config.

const KEY = "ken.files.followOpen";

export function loadFollowOpen(): boolean {
  try {
    const raw = localStorage.getItem(KEY);
    // Absent means never chosen — default on, not off.
    return raw === null ? true : raw === "true";
  } catch {
    return true;
  }
}

export function saveFollowOpen(value: boolean) {
  try {
    localStorage.setItem(KEY, String(value));
  } catch {
    /* best-effort */
  }
}
