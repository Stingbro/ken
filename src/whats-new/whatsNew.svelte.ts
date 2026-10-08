// "What's new" state (Svelte 5 runes). Shows the dialog once per new version:
// the newest version this install has been told about lives in localStorage,
// and everything released between it and the running build is shown on the
// next launch. A fresh install records the current version silently — nobody
// wants a changelog before they've used the app once.
import whatsNewMd from "../../WHATS_NEW.md?raw";
import { parseWhatsNew, releasesToShow, compareVersions, LAST_SEEN_KEY, type Release } from "./whatsNew";

const version = __APP_VERSION__;
const all = parseWhatsNew(whatsNewMd);

let open = $state(false);
let releases = $state<Release[]>([]);
// True when `releases` is the whole changelog because Settings asked for the
// current version and WHATS_NEW.md has no entry for it — the dialog says so.
let showingAll = $state(false);
let started = false;

/** localStorage is unavailable in some webviews; never let it break launch. */
function readLastSeen(): string | null {
  try {
    return localStorage.getItem(LAST_SEEN_KEY);
  } catch {
    return null;
  }
}

function writeLastSeen(value: string) {
  try {
    localStorage.setItem(LAST_SEEN_KEY, value);
  } catch {
    // Nothing to do: the dialog simply shows again next launch.
  }
}

export const whatsNew = {
  get open() {
    return open;
  },
  get releases() {
    return releases;
  },
  get version() {
    return version;
  },
  get showingAll() {
    return showingAll;
  },

  /** Call once after the shell mounts (i.e. with a project open). */
  init() {
    if (started) return;
    started = true;
    const lastSeen = readLastSeen();
    if (lastSeen === null) {
      // First run on this machine: start the clock, stay quiet.
      writeLastSeen(version);
      return;
    }
    const pending = releasesToShow(all, version, lastSeen);
    if (pending.length === 0) return;
    releases = pending;
    showingAll = false;
    open = true;
  },

  /** Settings → "What's new in this version": the running version's entry. */
  show() {
    const current = all.filter((r) => compareVersions(r.version, version) === 0);
    showingAll = current.length === 0;
    releases = showingAll ? all : current;
    open = true;
  },

  /** Esc, the backdrop, or "Got it": close and don't show this version again. */
  dismiss() {
    writeLastSeen(version);
    open = false;
  },
};
