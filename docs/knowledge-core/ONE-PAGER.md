# Ken's knowledge base, on one page

## The idea

Ken applies Karpathy's [LLM wiki](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) to a project. The model doesn't search raw documents again for every question. It keeps a wiki of the project up to date, so the summaries, cross-links and contradictions are already worked out when someone asks.

- **The code is the truth** for how things work today. The knowledge base holds what code can't: why, what was decided, and what is true now.
- **People bring sources and ask questions. Ken does the bookkeeping:** notes, page edits, links, findings and the log.
- **Everything is one git repo of Markdown.** History, undo and sharing come from git.

## What's in it

| part | holds |
|---|---|
| `START-HERE.md` | where to begin, the sources covered, reading order |
| `CLAUDE.md` | the brief: how any agent should work in this knowledge base |
| `Rules/` | how the team works; binding |
| `Decisions/DECISIONS.md` | rulings, `## D-001 · date · topic`; binding until superseded |
| `Current/` | what is true now: Project, Roadmap, Feature Status, `RAID.md` |
| `Design/` | why something was built this way |
| `Reference/` | vocabulary, platform facts |
| `Research/Ingestion/` | `Raw/` to drop sources in; `Ingested/` for the notes |
| `tickets/` | actions, `KEY-001` |
| `log.md` | one line per ingest, drift run and lint pass |

## How Ken keeps it

- **Ingest.** A source dropped in `Raw/` becomes a dated note, and the pages it changes are edited section by section. Rulings, actions, risks and issues wait as cards for a click. An edit to a Rules page, or one that goes against the code, waits as a diff. Each ingest is one commit.
- **Search.** Keyword and meaning search over every source. Every hit reads `kind · tier · date`, and binding knowledge ranks first.
- **Drift.** When cited code moves, Ken re-reads only the pages that cite it, then fixes the page or stamps it checked.
- **Lint.** Broken links, missing pages, contradictions and stale pages become findings in the Inbox.
- **Log.** The project's RAID register: risks, actions, issues, decisions and milestones.

## Get started

1. **Set up:** Ken › Set up a new workspace, then a name, then where the knowledge base goes (new from the template, or an existing wiki), then the sources it covers.
2. **Accept** the two proposals in the Inbox: each source's controls and file roles.
3. **Drop** a meeting note or document into `Research/Ingestion/Raw/`.
4. **Review** the Ingest cards: add the ruling, the ticket and the risk; apply or discard the held edit.
5. **Ask** in the search bar or in Ask Ken. Answers cite their sources.
6. **Connect agents:** Settings › Agents › copy the command into Claude Code. It gives the agent the whole workspace.

## Ground rules

- Ken commits only in the knowledge base, and only its own changes. It never commits in a code repo.
- Ken pushes only through sync.
- Background work is throttled. Gentle, the default, allows at most 20 Claude calls an hour, and Pause stops it all.
- The decisions behind all of this: [DECISIONS.md](DECISIONS.md).
