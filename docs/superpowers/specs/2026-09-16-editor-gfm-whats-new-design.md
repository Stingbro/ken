# Editor GFM polish, What's New dialog, and agent-driven releases

Date: 2026-09-16

## Goals

1. Make the Markdown editor (Crepe/Milkdown, `src/files/MarkdownEditor.svelte`)
   behave like a modern GitHub-flavored editor:
   - Inline Mermaid rendering for ```` ```mermaid ```` fences, with a
     diagram/code toggle whose controls only appear on hover in diagram mode.
   - Backspace at the start of any heading (any level) turns it straight into a
     paragraph (Milkdown's default downgrades h1→h2→…→p one step per press).
   - Typing `/h1 ` (slash, keyword, space) in an empty block applies the block
     type directly; same for `/h2`…`/h6`, `/quote`, `/code`, `/divider`,
     `/bullet`, `/number`, `/todo`, and the alert kinds (`/note`, `/tip`,
     `/important`, `/warning`, `/caution`).
   - The block "+" (add) menu and slash menu can insert a Quote reliably.
   - All GitHub-style formatting renders: tables, task lists, strikethrough,
     footnotes, autolinks (already in Crepe's GFM preset — verify + style), and
     GitHub alert banners `> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`,
     `> [!WARNING]`, `> [!CAUTION]` (new).
2. Show a "What's New" dialog the first time each new app version launches.
3. Make releases an agent task: a `/release` skill that writes the summarized
   What's New entry, bumps the version, and pushes; CI does the rest.

## Non-goals

- Alerts/Mermaid in the chat markdown renderer (`src/lib/markdown.ts`).
- Changing the CI tagging/changelog pipeline beyond the gate check below.

## Editor design

All new editor code lives in `src/files/markdown/` as plain TS modules with
vitest coverage where logic is pure, and is wired into `MarkdownEditor.svelte`.

### Mermaid (`src/files/markdown/mermaid.ts`)
- Dependency: `mermaid` (v11/12). Lazy `import("mermaid")` on first mermaid
  block so the editor bundle stays light.
- Crepe `CodeMirror` feature config: `renderPreview(language, content,
  applyPreview)` returns `null` for non-mermaid languages; for `mermaid` it
  renders asynchronously (`mermaid.render`) into an `HTMLElement` and calls
  `applyPreview`. Render errors show a compact inline error element instead of
  breaking the block. Renders are debounced/keyed so stale results are dropped.
- `previewOnlyByDefault: true` (only matters when a preview exists, i.e.
  mermaid). `previewToggleText`: "Code" when in diagram mode, "Diagram" when in
  code mode.
- CSS in `MarkdownEditor.svelte`: when the code block's `.codemirror-host` has
  class `hidden` (diagram mode), the sibling `.tools` bar is `opacity: 0` and
  becomes visible on `:hover`/`:focus-within` of the block. In code mode the
  tools stay visible as today. Theme mermaid with Paper & Ink tokens where
  cheap (background transparent, font var(--font-sans)); dark mode uses
  mermaid's `dark` theme when `document.documentElement` has the dark theme.

### Heading backspace (`src/files/markdown/headingBackspace.ts`)
- Disable Milkdown's `DowngradeHeading` shortcut via
  `headingKeymap.ctx` config (set its `shortcuts` to `[]`) and add a `$prose`
  keymap plugin: on Backspace with an empty selection at `parentOffset === 0`
  inside a `heading`, `setBlockType(paragraph)` and return true; otherwise
  return false so default behaviour runs.

### Slash shortcuts (`src/files/markdown/slashShortcuts.ts`)
- `$inputRule` matching `^/(h[1-6]|quote|code|divider|bullet|number|todo|
  note|tip|important|warning|caution)\s$` at the start of a paragraph/heading
  textblock (the whole block text must be just that token). Clears the text
  and applies the matching block type / wrap / insert. Pure helper
  `parseSlashShortcut(text): SlashShortcut | null` is unit-tested.

### Quote from the add button
- Reproduce in the harness. Expected root cause: `clearTextInCurrentBlockCommand`
  followed by `wrapInBlockTypeCommand` on an empty paragraph, or the menu
  filter hiding the item. Fix at the smallest scope (e.g. override the `quote`
  item via `buildMenu`/`textGroup.quote` with a robust command that replaces
  the empty block with `blockquote(paragraph)`), keep the item in the menu.

### GitHub alerts (`src/files/markdown/githubAlert.ts`, `githubAlert.css`)
- mdast: a remark plugin (`$remark`) that walks `blockquote` nodes whose first
  child paragraph starts with `[!NOTE]|[!TIP]|[!IMPORTANT]|[!WARNING]|[!CAUTION]`
  (case-insensitive, GitHub's syntax) and converts them to a
  `githubAlert { kind }` mdast node containing the remaining children (the
  marker paragraph is dropped or trimmed of the marker). The reverse direction
  (toMarkdown) writes `> [!KIND]` then the children as a blockquote.
- ProseMirror: `$nodeSchema("github_alert")` — group block, content `block+`,
  attrs `{ kind }`, `parseDOM`/`toDOM` as
  `div.github-alert[data-kind]` containing `div.github-alert-title` (icon +
  label, `contenteditable=false`) and `div.github-alert-body` (content hole).
  Parser/serializer hooks map to/from the `githubAlert` mdast node.
- Slash menu: an "Alerts" group (Note, Tip, Important, Warning, Caution) via
  Crepe `BlockEdit.buildMenu`. `/note ` etc. handled by slash shortcuts.
- Styling: GitHub's five colors mapped to Paper & Ink tokens (note→accent,
  tip→healthy green, important→purple-ish ink, warning→needs-input amber,
  caution→conflict red), left border 3px, title bold 13px with lucide-style
  inline SVG icons. Dark-mode friendly via tokens.
- Unit tests (`githubAlert.test.ts`) run a Milkdown editor headlessly in
  happy-dom: markdown → editor → markdown round trip for each kind, nested
  content, and a plain blockquote staying a blockquote.

### Harness
- `harness/markdown.html` + `harness/markdown-main.ts` mounting
  `MarkdownEditor.svelte` with `?md=<url>` fixture from `harness/decks/`.
  Playwright (system Chrome) drives it for manual/visual verification.

## What's New design

### Content: `WHATS_NEW.md` (repo root)
```
# What's New

## 0.1.3 — 2026-09-16

### Editor
- **Mermaid diagrams**: ```mermaid fences render inline; hover to switch to code.
- **GitHub alerts**: `> [!NOTE]` style banners render and round-trip.

### Releases
- **What's New dialog**: this dialog, shown once per new version.
```
Rules: newest version first; `## <semver> — <YYYY-MM-DD>`; `###` sections;
`- **Key part**: description` items. Only user-facing changes.

### Code
- `vite.config.ts`: `define: { __APP_VERSION__: JSON.stringify(pkg.version) }`
  and a `?raw` import of `WHATS_NEW.md` (the file is outside `src`; Vite
  allows `../WHATS_NEW.md?raw`). Type in `src/vite-env.d.ts`.
- `src/whats-new/whatsNew.ts` (pure, tested): `parseWhatsNew(md): Release[]`
  (`{version, date, sections:[{title, items:[{key, body}]}]}`),
  `compareVersions`, `releasesToShow(releases, currentVersion, lastSeen)` →
  releases with `lastSeen < v <= current` (empty when lastSeen is null or no
  match). Inline markdown in item bodies limited to code spans and links,
  rendered via `renderMarkdown` from `src/lib/markdown.ts`.
- `src/whats-new/whatsNew.svelte.ts`: `whatsNew.init()` reads
  `localStorage["ken.whatsNew.lastSeen"]`; if null → store current version,
  don't show. Else compute releases to show; `open` state; `dismiss()` writes
  current version. `show()` forces open with the current version's entry
  (Settings link).
- `src/whats-new/WhatsNewDialog.svelte`: overlay (`role="dialog"`,
  `aria-modal`, Esc + backdrop click + "Got it" close), Paper & Ink styling
  (radius 14, `--shadow-overlay`, serif title "What's new in Ken <version>",
  overline section titles, bold key + body). Multiple releases stack with a
  version subheading each. Mounted in `src/shell/Shell.svelte` (after project
  open) so onboarding isn't interrupted.
- Settings: a "What's new in this version" link in `SettingsScreen.svelte`
  next to the version/about area, calls `whatsNew.show()`.

### Release gate
- `scripts/release-gate.sh`: when `should_release=true`, also require
  `## <version>` present in `WHATS_NEW.md`; otherwise print a clear error and
  `exit 1` (do not silently skip). Unit-ish check: a `scripts/check-whats-new.sh
  <version>` helper used by the gate and by the `/release` skill.

## Release skill: `.claude/skills/release/SKILL.md`
Steps the agent follows: preconditions (clean tree, on `main`, up to date),
`git describe --tags --abbrev=0` → `git log <tag>..HEAD --oneline`, decide
patch/minor bump, write the `WHATS_NEW.md` entry (sections + bold key parts,
user-facing only, no commit noise), bump `package.json`, run
`scripts/sync-version.sh <version>` and `scripts/check-whats-new.sh <version>`,
run `pnpm test && pnpm check && cargo test -p ken-core` (or `make test`),
commit `chore(release): bump version to <version>` including WHATS_NEW.md,
push `main`. Document that the CI gate then tags, regenerates CHANGELOG.md and
builds installers, and how to recover if the gate fails (fix, re-push).

## Testing
- vitest for pure modules (parser, slash shortcut parser, version compare,
  releasesToShow, alert round trip).
- `pnpm check` must pass.
- Visual verification through `harness/markdown.html` with Playwright, and a
  dev build if screen capture is available.
