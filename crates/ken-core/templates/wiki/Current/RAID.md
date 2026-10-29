---
title: "RAID"
aliases: ["RAID", "risks and issues", "risk register", "issue log", "what might go wrong", "what is wrong now"]
status: current
audience: business
updated: {{date}}
---

# RAID

What might go wrong, and what's wrong now? Each risk and issue gets one entry here. A risk is something that might go wrong, and an issue is a problem now. The actions are the open tickets and the decisions are the decisions log, and Ken's Log shows all four with the milestones.

[[#Entry Format]] · [[#Entries]]

## Entry Format

An entry gets the next number and keeps it, because commits, tickets and notes cite the ids. Its status and owner change in place as it moves on, and a closed entry stays. A risk or issue found in an ingested note waits on its card, and one confirm adds it here.

```
## R-001 · 2026-01-01 · the risk in a few words

status: open
owner: who watches it, if anyone
sources: [[Note Name]]

What might go wrong and what would show it, in a line or two.
```

`R-nnn` is a risk and `I-nnn` an issue. A risk is `open`, `mitigated` or `closed`, and an issue is `open`, `resolved` or `closed`.

## Entries
