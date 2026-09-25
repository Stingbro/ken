# _meta

The checks that keep the vault honest, and the vault's own housekeeping. Each script is one of three kinds: an **instrument** is a read-only script that measures and reports through its exit code; a **gate** runs in CI, reads an instrument and makes the pass-or-fail call; a **historical** script ran once for a migration or backfill and is kept to be read, never run again as-is.

| script | kind | does |
|---|---|---|
| `linkcheck` | instrument | every link resolves · path links and name links counted apart · reads `aliases:` · reports names claimed by two pages · run before and after any bulk move |
| `linkgate` | gate | runs linkcheck and fails on exit 1 · reports exit 2 |
| `driftcheck` | instrument | scope, decision, docs and answer, each declared against actual · movement measured against each repo's default branch, read from git · a cited line past the end of its file flagged · only code citations measured for movement · a page unverified for thirty days raised even when nothing moved · the decisions log checked per entry, from each entry's own date · `Research/` in its own bucket |
| `driftgate` | gate | runs driftcheck and fails on exit 1 · reports exit 2 |
| `inventory` | instrument | section sizes · orphans · pages with no index entry · rule notes the Rules page does not link |

## The Instrument Contract

- **It carries a case it must fail.** A known-bad control and a known-good one (for `driftcheck`, a page pinned to a commit known to have moved and one known clean; for `linkcheck`, a link known broken and one known good), and a minimum count of pages examined. A run is void if either control comes back wrong: the known-bad one reads clean, or the known-good one reads flagged. A run that examined fewer pages than the minimum is void too. The run says that it is void.
- **It is read-only**, so the number stays quotable outside CI and the pass-or-fail policy changes without touching the measurement.
- **Three exit codes.** 0 clean; 1 instrument broken, or a live locator (a cited `repo:path:line`) missing; 2 findings only, such as drift or broken links. CI fails hard on 1 and reports 2.
- **It finds its root from its own location**, with sibling repos beside the vault and one environment variable to override. A hardcoded parent path walks zero files on any other checkout and reports a clean run.
- **It says what it could not reach.** A sibling repo that is absent is listed as not measured, with how to enable it.
- **It strips before it counts.** Fenced blocks and placeholders are never links or citations; `linkcheck` also strips inline code, while `driftcheck` reads inside it, because a code reference is written as inline code; build output, dependency folders and logs are never missing paths.
- **What a check finds becomes a ticket**, and the ticket fixes it. That includes what a new check finds on its first run.

## Generated Pages

A generated page carries `generated: true`, `upstream:`, `upstream_commit:`, `updated:` (the date it was built) and `regenerate:` (the exact command) in its frontmatter, and says so in its first line. Fix a defect in the generator and re-emit; never patch the output. `driftcheck` asserts every page from one generator pins the same `upstream_commit`.
