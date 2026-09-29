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
  once: the knowledge graph, search, and the links between its pages. If search
  returns little, read the repos directly before answering.
- **Use the code map before grep.** Definition, usages, outline and imports.
  For why code changed, read its git history.
- **A finding is evidence at the time, not current state.**
- **Open the Repo Map or a how-to before searching files.** Repo Map = *where*,
  how-to = *how*, code = the exact signature.
- **An ingest proposes; a person confirms.** From a source, Ken shows what it
  overturns, where it contradicts the library or itself, and each page it would
  change or add. A change is written when a person applies it, one at a time or
  all together, and cites the note. A ruling waits for its decider. A change to
  Ways-of-Working, Conventions or a rule becomes a ticket, because work and
  reviews are read against them.
- {{project-specific gotcha}}
- {{project-specific gotcha}}
