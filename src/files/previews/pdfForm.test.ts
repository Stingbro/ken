import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createFormSaver, type FormStorage } from "./pdfForm";

/** Stand-in for pdf.js's AnnotationStorage: the two hooks plus resetModified. */
function makeStorage(): FormStorage & { modify(): void; resets: number } {
  const storage = {
    onSetModified: null as (() => void) | null,
    onResetModified: null as (() => void) | null,
    resets: 0,
    resetModified() {
      storage.resets += 1;
      storage.onResetModified?.();
    },
    /** A field edit, as pdf.js reports it. */
    modify() {
      storage.onSetModified?.();
    },
  };
  return storage;
}

/** A promise whose settlement the test controls. */
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("createFormSaver", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("coalesces a burst of edits into one debounced save", async () => {
    const storage = makeStorage();
    const serialize = vi.fn(async () => new Uint8Array([1, 2, 3]));
    const write = vi.fn(async (_bytes: Uint8Array) => 42);
    const onchange = vi.fn();
    const onsaved = vi.fn();
    const saver = createFormSaver({ storage, serialize, write, onchange, onsaved });

    storage.modify();
    await vi.advanceTimersByTimeAsync(300);
    storage.modify();
    await vi.advanceTimersByTimeAsync(300);
    storage.modify();
    expect(write).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(800);
    expect(serialize).toHaveBeenCalledTimes(1);
    expect(write).toHaveBeenCalledTimes(1);
    expect(write.mock.calls[0][0]).toEqual(new Uint8Array([1, 2, 3]));
    // The dirty flag is only reported once per cycle, however many keystrokes.
    expect(onchange).toHaveBeenCalledTimes(1);
    expect(storage.resets).toBe(1);
    expect(onsaved).toHaveBeenCalledWith(42);
    saver.dispose();
  });

  it("resets the storage only after the bytes are written", async () => {
    const storage = makeStorage();
    const gate = deferred<number>();
    const order: string[] = [];
    const saver = createFormSaver({
      storage,
      serialize: async () => {
        order.push("serialize");
        return new Uint8Array();
      },
      write: () => {
        order.push("write");
        return gate.promise;
      },
    });
    storage.onResetModified = () => order.push("reset");

    storage.modify();
    await vi.advanceTimersByTimeAsync(800);
    expect(order).toEqual(["serialize", "write"]);
    gate.resolve(7);
    await vi.advanceTimersByTimeAsync(0);
    expect(order).toEqual(["serialize", "write", "reset"]);
    saver.dispose();
  });

  it("reports a new dirty cycle after each completed save", async () => {
    const storage = makeStorage();
    const onchange = vi.fn();
    const saver = createFormSaver({
      storage,
      serialize: async () => new Uint8Array(),
      write: async () => 1,
      onchange,
    });

    storage.modify();
    storage.modify();
    await vi.advanceTimersByTimeAsync(800);
    expect(onchange).toHaveBeenCalledTimes(1);

    storage.modify();
    expect(onchange).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(800);
    expect(onchange).toHaveBeenCalledTimes(2);
    saver.dispose();
  });

  it("stays dirty and reports a failed write", async () => {
    const storage = makeStorage();
    const onerror = vi.fn();
    const onsaved = vi.fn();
    const write = vi.fn(async (): Promise<number> => {
      throw new Error("disk full");
    });
    const saver = createFormSaver({
      storage,
      serialize: async () => new Uint8Array(),
      write,
      onerror,
      onsaved,
    });

    storage.modify();
    await vi.advanceTimersByTimeAsync(800);
    expect(onerror).toHaveBeenCalledTimes(1);
    expect(String(onerror.mock.calls[0][0])).toContain("disk full");
    expect(onsaved).not.toHaveBeenCalled();
    expect(storage.resets).toBe(0);
    // A failure doesn't spin: nothing retries until the next edit or a flush.
    await vi.advanceTimersByTimeAsync(5000);
    expect(write).toHaveBeenCalledTimes(1);

    // Still dirty, so a flush tries again.
    write.mockImplementation(async () => 9);
    await saver.flush();
    expect(write).toHaveBeenCalledTimes(2);
    expect(storage.resets).toBe(1);
    saver.dispose();
  });

  it("never overlaps two saves and re-saves an edit made mid-flight", async () => {
    const storage = makeStorage();
    const first = deferred<number>();
    const write = vi.fn(() => first.promise);
    const serialize = vi.fn(async () => new Uint8Array());
    const onsaved = vi.fn();
    const saver = createFormSaver({ storage, serialize, write, onsaved });

    storage.modify();
    await vi.advanceTimersByTimeAsync(800);
    expect(write).toHaveBeenCalledTimes(1);

    // An edit lands while the write is still in flight: no second write starts.
    storage.modify();
    await vi.advanceTimersByTimeAsync(2000);
    expect(write).toHaveBeenCalledTimes(1);

    write.mockImplementation(async () => 5);
    first.resolve(4);
    await vi.advanceTimersByTimeAsync(0);
    expect(onsaved).toHaveBeenCalledWith(4);
    // …and the mid-flight edit gets its own save once the first one lands.
    await vi.advanceTimersByTimeAsync(800);
    expect(write).toHaveBeenCalledTimes(2);
    expect(onsaved).toHaveBeenCalledWith(5);
    saver.dispose();
  });

  it("flush saves at once and is a no-op when clean", async () => {
    const storage = makeStorage();
    const write = vi.fn(async () => 3);
    const saver = createFormSaver({
      storage,
      serialize: async () => new Uint8Array(),
      write,
    });

    await saver.flush();
    expect(write).not.toHaveBeenCalled();

    storage.modify();
    await saver.flush();
    expect(write).toHaveBeenCalledTimes(1);
    // The pending debounce was cancelled by the flush — no double save.
    await vi.advanceTimersByTimeAsync(2000);
    expect(write).toHaveBeenCalledTimes(1);
    saver.dispose();
  });

  it("dispose cancels a pending save and unhooks the storage", async () => {
    const storage = makeStorage();
    const original = vi.fn();
    storage.onSetModified = original;
    const write = vi.fn(async () => 1);
    const saver = createFormSaver({
      storage,
      serialize: async () => new Uint8Array(),
      write,
    });

    storage.modify();
    expect(original).toHaveBeenCalledTimes(1); // the previous hook still runs
    saver.dispose();
    await vi.advanceTimersByTimeAsync(2000);
    expect(write).not.toHaveBeenCalled();
    expect(storage.onSetModified).toBe(original);

    // Unhooked: further edits are ignored, and flush no longer writes.
    storage.modify();
    await saver.flush();
    await vi.advanceTimersByTimeAsync(2000);
    expect(write).not.toHaveBeenCalled();
  });

  it("honours a custom debounce", async () => {
    const storage = makeStorage();
    const write = vi.fn(async () => 1);
    const saver = createFormSaver({
      storage,
      serialize: async () => new Uint8Array(),
      write,
      debounceMs: 50,
    });
    storage.modify();
    await vi.advanceTimersByTimeAsync(49);
    expect(write).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(write).toHaveBeenCalledTimes(1);
    saver.dispose();
  });
});
