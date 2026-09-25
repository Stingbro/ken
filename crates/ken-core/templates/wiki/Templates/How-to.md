---
title: "{{How to do the thing}}"
aliases: ["{{how do I do the thing}}", "{{where is the thing configured}}"]   # the questions people will type; a wiki link never resolves by title
status: current
updated: {{date}}       # when this page was last edited
verified: {{date}}      # a person read this against its sources; never stamped in bulk
pin: {{commit}}         # the commit those sources were verified at
sources:              # with none, the docs check never flags this page and the page vouches for nothing; the standing sweep still raises it after thirty days unverified
  - {{repo:path/to/file}}
---

# {{How to do the thing}}

**When you need this:** one line, so search finds it and a reader can bail early.

## Steps

1.
2.

## What goes wrong

The failure modes, each with its symptom.

<!-- The docs check diffs `sources:` from the pin to the repo's default branch
     and flags this page when they have moved. -->
