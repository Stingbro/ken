---
title: "Vocabulary"
aliases: ["Vocabulary", "Glossary", "what do we call it", "what does the platform call it", "why does my search find nothing"]
status: current
audience: dev
updated: {{date}}
verified:               # the date a person read this page against its sources · empty until then · never stamped in bulk
sources:
  - "{{repo:path of the code, config or doc that shows each row}}"
---

# Vocabulary

The words where a search misses: our word and the code's or the platform's word differ, one word means two things, or a name changed. Not here: what a system does → [[Systems]]; what a method word means in full → the method's manual, `docs/manual/` in the Ways of Working repo.

[[#Words That Differ]] · [[#Words With Two Meanings]] · [[#Renames]] · [[#Method Words]]

## Words That Differ

| our word | the code's or platform's word | shown in |
|---|---|---|
| {{the word the team, the users or the business say}} | {{the word to search for}} | {{repo:path:line}} |

## Words With Two Meanings

| word | meaning here | other meaning | shown in |
|---|---|---|---|
| {{word}} | {{what it means in this product}} | {{what it means in the platform, or in another part of the product}} | {{repo:path:line}} |

## Renames

| old name | new name | since | still under the old name |
|---|---|---|---|
| {{old name}} | {{new name}} | {{date, or D-nnn}} | {{the ids, files or commands that kept it}} |

## Method Words

| word | means | lives in |
|---|---|---|
| backlog | the Board's ranked list of confirmed tickets | the team repo's `tickets/` |
| idea | one row on the Ideas list · numbered I-001, I-002 and so on · a series apart from ticket numbers | the team repo's `ideas/` |
| escalation | a decision raised to the person who owns it · numbered E-001 onwards | the team repo's `escalations/` |
| ruling | a decision a person made, in their words · numbered D-001 onwards | the team repo's `decisions/DECISIONS.md` |
