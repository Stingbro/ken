---
title: "Feature Status"
aliases: ["Feature Status", "is it built", "does the product do this"]
status: current
audience: business
updated: {{date}}
sources:
  - "{{repo@sha:path of the changelog or the code that registers each feature}}"
---

# Feature Status

Is it built? Each feature gets one row here, so that question gets answered here first. Not here: why it was built this way → [[Design]]; what's planned → [[Roadmap]].

A change that lands or retires a feature updates its row in the same branch. Drift measures the code in this page's `sources:`, so list there every file the evidence column cites.

| feature | built | evidence |
|---|---|---|
| {{the feature, in the user's words}} | {{yes · partly · no · retired}} | {{repo:path, D-nnn or ticket id}} |
