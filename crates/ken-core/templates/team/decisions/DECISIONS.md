# Decisions

Append-only. One entry per ruling. A ruling that lives only in a ticket
disappears when the ticket closes.

Format:

```
D-001 · 2026-01-01 · topic — THE RULING IN ONE LINE
  Why: the incident or argument behind it.
  - "the words the decider actually used" -
  sources: repo:path/to/file.ext:120, [[Note]], commit@sha
  aliases: every term anyone might search for this by
  [REFINES] D-nnn   (or [SUPERSEDED BY D-nnn])
```

`sources:` is what G8's decision check reads — it compares the cited lines
against what is there now. The locator grammar is fixed: `repo:path:line`, a
vault wikilink, or `repo@sha`.

**Supersession, never deletion.** A reversed ruling stays in place, stamped
`[SUPERSEDED BY D-nnn]`, and is never renumbered — commits cite these ids. Never
re-open a listed decision without checking its supersession field; if it has
none, it stands.

The verbatim quote is evidence: the decider's exact words, typos included and
marked `[sic]`. A paraphrase is an interpretation, and interpretations drift.

Conflicts get their own section at the top, flagged *do not build on this until
asked*, rather than being silently resolved by whoever noticed. Open questions
are listed but marked NOT decisions — never citable as settled.
