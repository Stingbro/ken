import { afterEach, describe, expect, it } from "vitest";

import { installDropGuard } from "./dropGuard";

/** A drag event carrying a stand-in DataTransfer (happy-dom has none). */
function dragEvent(type: string, types: string[]) {
  const event = new Event(type, { bubbles: true, cancelable: true });
  const dataTransfer = { types, dropEffect: "copy" };
  Object.defineProperty(event, "dataTransfer", { value: dataTransfer });
  return { event, dataTransfer };
}

describe("installDropGuard", () => {
  let uninstall: (() => void) | undefined;
  afterEach(() => uninstall?.());

  it("makes an unclaimed OS-file drag inert", () => {
    uninstall = installDropGuard(window);
    const over = dragEvent("dragover", ["Files"]);
    document.body.dispatchEvent(over.event);
    expect(over.event.defaultPrevented).toBe(true);
    expect(over.dataTransfer.dropEffect).toBe("none");

    const drop = dragEvent("drop", ["Files"]);
    document.body.dispatchEvent(drop.event);
    expect(drop.event.defaultPrevented).toBe(true);
  });

  it("leaves a drag a target already accepted alone", () => {
    uninstall = installDropGuard(window);
    const target = document.createElement("div");
    document.body.append(target);
    target.addEventListener("dragover", (e) => e.preventDefault());
    const over = dragEvent("dragover", ["Files"]);
    target.dispatchEvent(over.event);
    expect(over.dataTransfer.dropEffect).toBe("copy");
    target.remove();
  });

  it("ignores in-app drags that carry no files", () => {
    uninstall = installDropGuard(window);
    const over = dragEvent("dragover", ["text/plain"]);
    document.body.dispatchEvent(over.event);
    expect(over.event.defaultPrevented).toBe(false);
    const drop = dragEvent("drop", ["text/plain"]);
    document.body.dispatchEvent(drop.event);
    expect(drop.event.defaultPrevented).toBe(false);
  });

  it("uninstalls cleanly", () => {
    installDropGuard(window)();
    const over = dragEvent("dragover", ["Files"]);
    document.body.dispatchEvent(over.event);
    expect(over.event.defaultPrevented).toBe(false);
  });
});
