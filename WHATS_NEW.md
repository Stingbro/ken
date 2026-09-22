<!--
  Ken's user-facing changelog, shown in the in-app "What's new" dialog.

  Format (parsed by src/whats-new/whatsNew.ts — keep it exact):
    - Newest version first, directly under the `# What's New` title.
    - Version heading: `## <semver> — <YYYY-MM-DD>` (em dash; the date is
      optional but preferred).
    - Section heading: `### <Section name>` — one or more per version.
    - Item: `- **Key part**: short description.`
    - Only user-facing changes, in the user's words. No commit-message noise,
      no internal refactors, no file paths.
    - Inline markdown in a description is limited to code spans and links.

  The `/release` skill (.claude/skills/release/SKILL.md) writes each entry and
  scripts/check-whats-new.sh fails the release if the version is missing here.
-->

# What's New

## 0.2.0 — 2026-09-22

### Editor

- **Mermaid diagrams**: ` ```mermaid ` fences render as diagrams right in the document; hover a diagram to switch back to its code.
- **GitHub alerts**: `> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!WARNING]` and `> [!CAUTION]` blockquotes render as colored banners and round-trip to plain Markdown.
- **GitHub-flavored polish**: tables, task lists, strikethrough, footnotes and autolinks are styled to match the rest of the app.
- **Backspace clears a heading**: pressing Backspace at the start of any heading turns it straight into a paragraph instead of stepping down one level at a time.
- **Slash shortcuts**: type `/h1 ` (or `/quote`, `/code`, `/todo`, `/note`, …) in an empty block to apply that block type immediately.
- **Quote from the add menu**: the block "+" menu now inserts a quote reliably.
- **Table editing**: hover a table to add or remove rows and columns, set column alignment, and drag to reorder; the handles are now visible on paper.
- **Table menu**: right-click any cell to insert or delete rows and columns, set column alignment, switch the table to full width, or delete the table.
- **Table width**: tables now size to their content and sit in the text column; hover a table and use the button above its top-right corner (or the right-click menu) to let it spread across the pane. The choice is saved in the file in a way other Markdown tools ignore. Table lines are also clearer and tables have more room around them.
- **Table columns**: each column is now sized to how much content it holds. Short columns — dates, counts, names, statuses — get the room to render whole, and wide description columns give up the difference, so nothing wraps that did not have to. Tables still never scroll sideways.
- **Headings**: every heading level is larger, with a clearer step between levels.
- **Heading links**: hover a heading and click the link icon to copy `[Heading](#slug)` to the clipboard.
- **Table of contents**: type `/toc ` in an empty block (or pick "Table of contents" from the block "+" menu) to drop in a table of contents. It lists every heading below it, nested by level, and rewrites itself as you add, rename or reorder headings. On disk it is an ordinary list of links, so other Markdown tools show it too.
- **List markers**: bullets and numbers now match the colour of their text.
- **In-document links**: clicking a `#heading` link scrolls to that heading instead of doing nothing.
- **Diagram and image viewer**: click a Mermaid diagram or an image to open it full-pane, then zoom with the wheel, pinch or `+`/`-` and drag to pan. Esc closes it.

### Files

- **PDF forms**: fill in the fields of a PDF form right in the preview pane and save the filled copy.
- **Diagrams.net files**: `.drawio` diagrams open in a preview with page tabs, zoom and layers, and their text labels are indexed for search.
- **Web shortcuts**: `.url` internet shortcuts get their own preview and icon, can be created from the file tree, and open their link or reveal the file in Finder.
- **Folder tools**: mark a whole folder as viewed in one action, and optionally have the tree follow whichever file is open.
- **Tab strip**: right-click a tab to close tabs to the right or close all, reveal the file, or copy its path; hover a shortened tab name to see it in full.
- **Cloud images**: images that live only in the cloud are fetched in the background so their text can be recognized and searched.
- **Backup files**: `.bak` files are skipped, the same way Office lock files are.
- **Right-click menus**: context menus no longer vanish after you switch between screens.
- **Folder moves**: Moving a folder no longer freezes the app or rebuilds the whole index.
- **Trash**: Moving a folder to the Trash works when it holds iCloud files that are not downloaded, and no longer freezes the app.

### Chat

- **Questions from the assistant**: when the assistant needs to ask you something, a question card appears in the conversation and answering it resumes the turn.

### Releases

- **What's new dialog**: this dialog, shown once after each update, with the highlights of everything you got. Reopen it any time from Settings.
- **Install without admin rights**: the Windows installer installs per user, and the macOS install script falls back to `~/Applications` when `/Applications` is not writable.

### Performance

- **Map and Timeline**: paused for now — the background analysis that fed them kept the GPU busy and fans spinning even while Ken was idle.
