# {{team_name}} — team repo

Operational state. Everything here changes hourly, which is why it is not in
the library: a page pins itself to a commit and the doc check reads
`git diff <pin>..<default branch>`, so the library's history has to stay readable.

| directory | holds |
|---|---|
| `tickets/` | one file per ticket, frontmatter + body + thread |
| `ideas/` | the Ideas list on Intake's Ideas tab: one file per rough idea, I-001 onwards |
| `decisions/` | the append-only decisions log |
| `method/` | steps, presets, gates — the shared definition |
| `team/` | this team's overlay. Add-only. |
| `people/` | who is on the team |
| `runs/` | run records: which gate ran, when, exit code, log |

## The Guard

`.github/workflows/guard.yml` runs on every push and checks: six core fields
present, the thread section present, never-droppable gates present, every
custom field typed, every status mapped. It carries a deliberately broken
overlay as its control, because a validator that cannot fail is not validating.
