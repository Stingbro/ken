# {{wiki_repo}}

The knowledge library. Entry point: `START-HERE.md`.

**This file is what auto-loads**, so it carries the short form of every rule.
The notes are the evidence behind it. If they ever disagree, **this file
wins** — then fix the library the same day.

## Sections

`Ways-of-Working/` is **binding**. It is the only section that constrains *how*
work is done rather than describing what exists.

| read | for |
|---|---|
| `Ways-of-Working/Ways-of-Working.md` | the method, and where to look, in what order |
| `Ways-of-Working/Rules/` | the binding rules — **one file per rule, named as the rule**, each carrying its incident |
| `Ways-of-Working/Agents/` | session start: the checklist · the source-of-truth order · token hygiene |
| `Conventions/` | the way the team does it here, one page per area · descriptive: what a review reads a diff against |
| `Current/` | what is true now about the project and the team; **maintained**, rewritten in place, never appended to |
| `Research/Ingestion/` | the inbox that empties: a source dropped in `Raw/` becomes a dated note in `Ingested/` · a person reads the takeaways and undoes what is wrong · neither folder is a research source |

## Binding Rules

- **Search the library first.** Re-deriving from code is slower than reading the
  page that holds the answer. If the answer is not here and you find it,
  **write it back** — the finding is not done until you do.
- **Search before saying something is missing.** Before reporting that something
  does not exist, search the code, the data files, the platform's config and the
  platform's source. Name what was searched. Search the library three ways at
  once: the knowledge graph, search, and the links between its pages.
- **A finding is evidence at the time, not current state.**
- **Open the Repo Map or a how-to before searching files.** Repo Map = *where*,
  how-to = *how*, code = the exact signature.
- **Every agent edit to a wiki page, from an ingest or a recipe (Ken's stored
  rule that keeps an output page current), goes through staging**: the tool
  applies it unless it rewrites more than a fifth of the page or lands on one a
  person changed while the run worked. Only three things wait: a ruling, for its
  decider; a change to Ways-of-Working, Conventions or a rule, as a ticket,
  because work and reviews are read against them; and an edit the tool holds in
  staging. Everything else is written, and an ingest's edits cite its note. Nobody confirms a
  note, a link or a small correction.
- {{project-specific gotcha}}
- {{project-specific gotcha}}
