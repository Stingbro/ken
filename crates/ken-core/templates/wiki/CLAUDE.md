# {{wiki_repo}}

You're in the {{workspace_name}} knowledge base. Start at `START-HERE.md`.

This file loads into every session, so it holds the short form of every rule and how to work in this library. The full rules are in `Rules/`. When the two disagree, this file wins and the rule's page gets fixed the same day.

## What This Is

A library kept for the project, beside the code. **The code is the truth about how the product works today. The library holds what the code can't tell you,** such as why it's built this way, what was decided and reversed, what came out of a meeting and how the parts fit. For the detail, a page points to a file by its path, in backticks. A page that copies code goes stale with the next commit.

| folder | holds |
|---|---|
| `Rules/` | how the team works and the conventions it holds itself to, one rule per page · binding |
| `Decisions/DECISIONS.md` | every ruling, added and never edited · binding until superseded |
| `Design/` | what we chose to build, and why we turned down the alternative |
| `Current/` | what's true now: the project, the roadmap, feature status · rewritten in place, never appended to |
| `Current/RAID.md` | the risks and issues, `## R-001 · date · title` then `status:`, `owner:` and `sources:` · an entry gets the next number, then its status changes in place |
| `Reference/` | the vocabulary, how-tos, where things live · `Platform/`, one page per dependency we build on |
| `Research/` | dated findings · `Ingestion/Raw/` is the inbox and `Ingestion/Ingested/` holds the notes · evidence from its date |
| `tickets/` | the work in flight and the work done |
| `Templates/` | the blanks a new page is copied from |
| `log.md` | one dated line per ingest, drift run and lint pass |

## Answering

- **Search first, with Ken's tools.** For code, use the code index (definition, usages, outline, imports) before grep, and read the lines around a hit. Don't read a long file from top to bottom. To find out why code changed, read its git history.
- **A ruling is settled.** Don't reopen it in an answer. When the code doesn't match it, say so and cite both.
- **The code wins on what was built.** A page, ticket or plan says what was intended. When it disagrees with the code, say so and cite both.
- **An empty search proves nothing.** Check `Reference/Vocabulary.md` for the other word, then read the repos directly before you say something isn't there, and say where you looked.
- **A Research page or an ingested note is evidence from its date.** Cite it with its date.
- **File a good answer back** as a source when it'll be asked again. Ingest makes the pages.

## Ingesting

A source dropped in `Research/Ingestion/Raw/` becomes a dated note in `Ingested/`, and the source moves beside it. The pages it changes get updated one section at a time, citing the note, and a new page joins its section's index.

- What it contradicts, in a page, a ruling, a ticket or the code, gets named and becomes a finding. So does each question it leaves open.
- A ruling waits for one confirm into the decisions log, and a risk or issue for one confirm into `Current/RAID.md`. An action becomes a proposed ticket.
- An edit that rewrites more than a fifth of a page, lands on a `Rules/` page, touches a page someone changed during the run, or contradicts a ruling or the code waits for a person.
- Each ingest is one commit in this repo with one line in `log.md`, so git can take it back.

## Keeping It True

- A page lists what its claims rest on under `sources:`, each with the commit it was read at.
- When a page is no longer true, retire it and keep it: `status: superseded`, and `superseded_by:` naming the page that replaces it. Move anything true that exists nowhere else first.
- A move rewrites the links to the page in the same change.
- Tag what the team inferred. When a line or a field value is something the team concluded and nobody said, mark it inferred, as in `influence: high (inferred)`.
- A quoted number carries the date it was measured.
- Write only inside this repo. A change to this file waits for a person.
- {{a trap that costs time on this team, and the page that holds it}}
