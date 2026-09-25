---
title: "{{System}} — architecture"
status: current
updated: {{date}}       # when this page was last edited
verified: {{date}}      # a person read this against its sources; never stamped in bulk
pin: {{commit}}         # the commit those sources were verified at
sources:
  - {{repo:path/to/the/entry/point}}
lens: arch
---

# {{System}} — architecture

## Layers

| layer | holds | may call |
|---|---|---|
| {{name}} | {{what lives here}} | {{the layers below it, named}} |

## May not

- {{the call that must never happen, and what happened the time it did}}

## Diagram

```mermaid
flowchart TD
  A[{{layer}}] --> B[{{layer}}]
```

<!-- A diagram that only shows boxes cannot be read by a reviewer, and a
     reviewer is what it is for. Name the layers, name what may call what,
     and name what may not. -->
