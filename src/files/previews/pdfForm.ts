/**
 * Auto-save for filled-in PDF form fields.
 *
 * pdf.js keeps every widget's value in the document's `AnnotationStorage` and
 * calls `onSetModified` on each edit. This turns that stream of keystrokes into
 * the same "type, pause, saved" rhythm the text editors use: debounce the edits,
 * serialize the document once, write the bytes, and tell the storage it is clean
 * again so pdf.js's own modified flag stays in step with the file on disk.
 */

/** The slice of pdf.js's `AnnotationStorage` this module touches. */
export interface FormStorage {
  onSetModified: (() => void) | null;
  onResetModified: (() => void) | null;
  resetModified(): void;
}

export interface FormSaverOpts {
  storage: FormStorage;
  /** `doc.saveDocument()` — the filled PDF's bytes. */
  serialize: () => Promise<Uint8Array>;
  /** Writes the bytes and returns the file's new mtime. */
  write: (bytes: Uint8Array) => Promise<number>;
  /** The first edit of a dirty cycle (i.e. the document just became unsaved). */
  onchange?: () => void;
  onsaved?: (mtime: number) => void;
  onerror?: (message: string) => void;
  debounceMs?: number;
}

export interface FormSaver {
  /** Saves right now if there are unsaved edits (used on close). */
  flush(): Promise<void>;
  /** Cancels anything pending and unhooks the storage. */
  dispose(): void;
}

export function createFormSaver(opts: FormSaverOpts): FormSaver {
  const debounceMs = opts.debounceMs ?? 800;
  const { storage } = opts;
  const previousHook = storage.onSetModified;

  let timer: ReturnType<typeof setTimeout> | undefined;
  let dirty = false;
  /** The in-flight save, so flush() can wait rather than start a second one. */
  let inflight: Promise<void> | null = null;
  /** An edit that arrived while a save was in flight — it needs its own save. */
  let modifiedDuringSave = false;
  let disposed = false;

  storage.onSetModified = () => {
    previousHook?.();
    if (disposed) return;
    if (inflight) modifiedDuringSave = true;
    if (!dirty) {
      dirty = true;
      opts.onchange?.();
    }
    // While a save is running, the reschedule happens when it settles — two
    // concurrent saveDocument()/write pairs would race for the same file.
    if (!inflight) schedule();
  };

  function schedule() {
    cancel();
    timer = setTimeout(() => {
      timer = undefined;
      void save();
    }, debounceMs);
  }

  function cancel() {
    if (timer) clearTimeout(timer);
    timer = undefined;
  }

  function save(): Promise<void> {
    if (disposed || !dirty || inflight) return inflight ?? Promise.resolve();
    modifiedDuringSave = false;
    const run = (async () => {
      try {
        const bytes = await opts.serialize();
        const mtime = await opts.write(bytes);
        // Only clean if nothing was typed while the bytes were being written —
        // that edit is already in the storage map, so the next save picks it up.
        if (!modifiedDuringSave) dirty = false;
        storage.resetModified();
        opts.onsaved?.(mtime);
      } catch (e) {
        // Stay dirty: the next edit (or a flush on close) tries again. We
        // deliberately don't self-retry — a failing write would spin forever.
        opts.onerror?.(String(e));
      } finally {
        inflight = null;
        if (modifiedDuringSave && !disposed) schedule();
      }
    })();
    inflight = run;
    return run;
  }

  return {
    async flush() {
      cancel();
      if (inflight) await inflight;
      if (disposed || !dirty) return;
      await save();
    },
    dispose() {
      disposed = true;
      cancel();
      storage.onSetModified = previousHook;
    },
  };
}
