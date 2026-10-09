# {{wiki_repo}}

The {{team_name}} knowledge base. Entry point: `START-HERE.md`.

This file loads into every session, so it holds the short form of every rule and how to work in this library. The full rules are in `Rules/`. If the two disagree, this file wins and the rule's page is fixed the same day.

## What This Is

A library Claude keeps for the project, beside the code. **The code is the truth for how the product works today. The library complements it and never copies it:** a page holds what the code cannot say (why it is built this way, what was decided and reversed, what came out of a meeting, how the parts fit) and points to a file by its path, in backticks, for the detail.

| folder | holds |
|---|---|
| `Rules/` | how the team works and the conventions it holds itself to, one rule per page · binding |
| `Decisions/DECISIONS.md` | every ruling, added and never edited · binding until superseded |
| `Design/` | what we chose to build, and why not the alternative |
| `Current/` | what is true now: the project, the roadmap, feature status · rewritten in place, never appended to |
| `Reference/` | the vocabulary, how-tos, where things live · `Platform/`, one page per dependency we build on |
| `Research/` | dated findings · `Ingestion/Raw/` is the inbox, `Ingestion/Ingested/` the notes · evidence at its date, never current state |
| `tickets/` | the work in flight and the work done |
| `Templates/` | the blanks a new page is copied from |
| `log.md` | one dated line per ingest, drift run and lint pass |

## Answering

- **Search first, with Ken's tools.** For code, use the code index (definition, usages, outline, imports) before grep, and read the lines around a hit, not the whole file. For why code changed, read its git history.
- **A ruling is settled.** Do not reopen it in an answer. Where the code does not match it, say so and cite both.
- **The code wins on what was built.** A page, ticket or plan says what was intended; when it disagrees with the code, say so and cite both.
- **An empty search proves nothing.** Check `Reference/Vocabulary.md` for the other word, then read the repos directly before saying something is not there, and say where you looked.
- **A Research page or an ingested note is evidence from its date.** Cite it with its date.
- **File a good answer back** as a page when it will be asked again.

## Ingesting

A source dropped in `Research/Ingestion/Raw/` becomes a dated note in `Ingested/`. The pages it changes are updated, citing the note. What it contradicts, in a page, a ruling, a ticket or the code, is named. A ruling waits for one confirm into the decisions log; an action becomes a drafted ticket. An edit that rewrites a large part of a page, lands on a `Rules/` page, or contradicts a ruling or the code waits for a person.

## Keeping It True

- A page lists what its claims rest on under `sources:`, each with the commit it was read at.
- A page that is no longer true is retired, not deleted: `status: superseded` and a banner naming the page that replaces it. Move anything true that exists nowhere else first.
- A move rewrites the links to the page in the same change.
- A quoted number carries the date it was measured.
- {{a trap that costs time on this team, and the page that holds it}}
