---
title: "{{Area}} — conventions"
status: current
updated: {{date}}       # when this page was last edited
verified: {{date}}      # a person read this against its sources; never stamped in bulk
pin: {{commit}}         # the commit those sources were verified at
sources:
  - {{repo:path/to/the/code/this/describes}}
lens: conventions          # which review lens reads this page
---

# {{Area}} — conventions

**Covers:** {{which paths this page describes}}

## The way we do it here

| instead of | use | why |
|---|---|---|
| {{the thing people reach for}} | {{the thing that already exists}} | {{the incident, or the cost}} |

## Already exists — do not build a second one

- {{name it, and say where it lives}}

## What this page is not

Preferences. If it is not enforceable by a reviewer reading this page against a
diff, it belongs in a personal convention, not here.

<!-- `pin:` is the commit `sources:` were verified at, and `verified:` is when.
     G8's doc check diffs those sources from the pin to now: if they moved after
     the pin, this page is suspect and gets flagged rather than rotting quietly. -->
