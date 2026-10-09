# Templates

What Ken lays down. `wiki/` is the knowledge base: one repo holding the library, the decisions log, the tickets, `log.md` and `.ken/knowledge.json`. `code/CLAUDE.md` is the short pointer Ken proposes for a code repo the knowledge base covers; a person adds it, and Ken never overwrites one.

The method these follow is the Knowledge page of Ways of Working (`docs/manual/docs-system.html`); this folder is the one copy of the templates, and the method links here.

## What Ken Fills

At set-up Ken fills `{{team_name}}`, `{{wiki_repo}}`, `{{KEY}}`, the repo table on Start Here, the repos in `.ken/knowledge.json`, and every `updated:` date. The blanks in `Templates/` keep their placeholders: a page is copied from one when something writes it, so no page ships that only holds placeholders.

## Frontmatter

`title`, `aliases`, `status` (`draft` · `current` · `superseded`, or `evidence` for Research), `updated`, `sources` (each `repo@sha:path` or `[[Note Name]]`) and `checked` (when a page was last re-read against its sources, and at which commit, written by the check that read it).
