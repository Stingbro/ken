# Decisions

Months later, someone will ask what was decided and who said so. Every ruling is here, in the words it was made in, added and never edited. Not here: why a design went the way it did → `Design/`; two rulings that disagree, or a question still open → the findings list.

## Entry Format

```
## D-001 · 2026-01-01 · topic

**The ruling, in one sentence.**

> "The words it was made in, typos included and marked [sic]." (who made it, if known)

Why: the reason, in one line.
sources: [[Note Name]], repo:path:line
aliases: every other term anyone might search for it by
supersedes: D-nnn, when it replaces an earlier ruling
refines: D-nnn, when it narrows one
```

| rule | why |
|---|---|
| Write the ruling down when it's made. | A ruling that lives only in a ticket or a chat is lost when they close. |
| Quote the words it was made in. | A paraphrase is your reading of it, and readings drift. |
| Add the term you searched for to `aliases:` when a search missed the entry. | The next person to search may use the same term. |
| Cite code in the fixed form `repo@sha:path`. | Drift measures only that form, from the ruling's own date. |
| Never delete or renumber. A reversed ruling stays, marked `[SUPERSEDED BY D-nnn]` under its ruling, and a narrowed one `[REFINED BY D-nnn]`. | Commits, tickets and code cite the numbers. |
| Check the marker before you treat a ruling as open. | With no marker, it stands. |

A ruling found in an ingested note waits on its card as a ready entry, and one confirm adds it here with the next number.

## Log
