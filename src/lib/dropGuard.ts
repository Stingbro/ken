// Window-level safety net for files dragged in from the OS. With Tauri's own
// drag-drop handling off (`dragDropEnabled: false`), the webview owns OS file
// drops, and a drop nobody claims runs WebKit's default action: navigating the
// whole window to the dropped file. Anything that accepts files (the editor,
// the file tree) cancels `dragover` itself; this guard runs last, in the bubble
// phase on the window, and makes every other spot refuse the drop. It never
// stops propagation, and in-app drags (no "Files" type) pass untouched.

function carriesFiles(event: Event): DataTransfer | null {
  const dt = (event as DragEvent).dataTransfer;
  return dt && Array.from(dt.types ?? []).includes("Files") ? dt : null;
}

/** Install the guard on `target` (the window); returns the uninstaller. */
export function installDropGuard(target: Window | EventTarget): () => void {
  const onDragOver = (event: Event) => {
    const dt = carriesFiles(event);
    if (!dt || event.defaultPrevented) return;
    // Cancelled with "none": the cursor shows no-drop and no drop follows.
    event.preventDefault();
    dt.dropEffect = "none";
  };
  const onDrop = (event: Event) => {
    if (carriesFiles(event)) event.preventDefault();
  };
  target.addEventListener("dragover", onDragOver);
  target.addEventListener("drop", onDrop);
  return () => {
    target.removeEventListener("dragover", onDragOver);
    target.removeEventListener("drop", onDrop);
  };
}
