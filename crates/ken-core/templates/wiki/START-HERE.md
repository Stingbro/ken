# Start Here

**The {{team_name}} knowledge library.** Open this folder as an Obsidian vault
for graph, backlinks and search across everything.

Repos, siblings of this vault — resolve them **relatively**, the parent differs
per machine, so never write an absolute path into a note.

| repo | what it is |
|---|---|
| `{{wiki_repo}}` | **this** — all documentation |
| `{{team_repo}}` | tickets · ideas · decisions · method and team config |
| `{{code_repo}}` | the code |

## The Library

Every section gets an **index note** saying what belongs in it, what does not,
and where that goes instead.

| section | what is in it |
|---|---|
| [[Ways-of-Working]] | how we work — **binding** |
| [[Current]] | what is true now: what the project is about · how the team is structured · who does what — **maintained, rewritten in place** |
| **Conventions** | the way we do it here, one page per area — descriptive: what a review reads a diff against |
| **{{Domain}}** | how the system works, verified against the source |
| **Design** | what we chose to build, and why not the alternative |
| **Work** | release notes and release history — tickets and ideas live in the team repo |
| **Reference** | standards, config, calculators |
| **Research** | 🗄️ what sessions found, **at the time**; [[Ingestion]] (`Research/Ingestion/`) is the inbox for meetings, recordings and documents, and it empties |
| **Repo-Map/** · **Templates/** | what lives where in the code · doc templates |

⚠️ Cite `Research/` as evidence; never read it as current state.

## Read in This Order

1. **{{The trap that costs the most}}** — the thing people lose the most time on
   before they find it. Put it first, above everything.
2. [[Vocabulary]] — our word vs the platform's, both directions. Check it before
   concluding a feature does not exist.
3. [[Research-Ladder]] — where to look, in what order.
4. [[Lifecycle]] — Plan → Size → Build → Release → Learn.
5. [[Rules]] — the binding rules, one note each, with the incident behind it.
6. **Repo-Map/** — what lives where in the code.
