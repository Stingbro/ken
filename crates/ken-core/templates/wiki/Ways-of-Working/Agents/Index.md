---
title: "Agents — Session Start"
aliases: ["Session Start", "where do I start a session"]
status: current
tags: [agents, moc]
---

# Agents — Session Start

**Read this page first** on every new ticket or fresh session. Then open **one**
map or how-to — do not start by grepping half the repo.

Pattern: [Karpathy's LLM wiki](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)
— compound knowledge into linked pages; the wiki is the map; code is ground
truth for detail.

## Source of Truth

| layer | role |
|---|---|
| this wiki | how we work, where things live, the gotchas |
| the code | implementation truth |
| {{upstream/vendor docs}} | the official account |
| {{decompiled or generated sources}} | signatures only, when the rest fails |

## Session Checklist

1. Open the ticket. Set `status: in-progress`.
2. Read the **Repo Map** for "where does X live?"
3. Open **one** matching how-to.
4. Only then search the code, under the paths the map named. Read the lines
   around each hit. Do not read a long file whole.
5. Generated or decompiled sources **last** — see the Research Ladder.
6. When you learn something lasting, **write it back** into the wiki. Don't
   leave truth only in a chat log.

## Token Hygiene

1. Repo Map = *where* · how-to = *how* · code = the exact signature.
2. After a hard bug, **compound** the lesson into the how-to or the gotchas.
3. Do not re-derive what a reference page already holds.
