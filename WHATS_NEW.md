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

## 0.1.3 — 2026-09-16

### Editor

- **Mermaid diagrams**: ` ```mermaid ` fences render as diagrams right in the document; hover a diagram to switch back to its code.
- **GitHub alerts**: `> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!WARNING]` and `> [!CAUTION]` blockquotes render as colored banners and round-trip to plain Markdown.
- **GitHub-flavored polish**: tables, task lists, strikethrough, footnotes and autolinks are styled to match the rest of the app.
- **Backspace clears a heading**: pressing Backspace at the start of any heading turns it straight into a paragraph instead of stepping down one level at a time.
- **Slash shortcuts**: type `/h1 ` (or `/quote`, `/code`, `/todo`, `/note`, …) in an empty block to apply that block type immediately.
- **Quote from the add menu**: the block "+" menu now inserts a quote reliably.
- **Table editing**: hover a table to add or remove rows and columns, set column alignment, and drag to reorder; the handles are now visible on paper. Wide tables grow past the text column and scroll sideways only when they outgrow the pane.

### Releases

- **What's new dialog**: this dialog, shown once after each update, with the highlights of everything you got. Reopen it any time from Settings.
