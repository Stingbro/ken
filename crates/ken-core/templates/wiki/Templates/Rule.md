---
title: "{{The Rule as a Sentence}}"
aliases: ["{{The Rule as a Sentence}}", "{{the-rule-as-a-sentence}}", "{{the question someone asks before breaking it}}"]
status: current
audience: {{business or dev}}
updated: {{date}}
sources:
  - "{{repo@sha:path, ticket id or [[Ingested note]] that shows the incident}}"
---

# {{The Rule as a Sentence}}

{{The rule, in one imperative line. The filename is the rule too, so it reads as one in a backlink. Its short form goes in CLAUDE.md as well.}}

[[#Incident]] · [[#Missed By]] · [[#Symptom]]

## Incident

{{What happened, when, and what it cost, and the change that caused it.}}

## Missed By

| step | why it missed | where the fix landed |
|---|---|---|
| {{the step that should have caught it: kickoff · spec · review · smoke · live-verify · QA walk}} | {{what it could not see}} | {{what that step reads: a skill · a Rules page · a lane prompt · a test}} |

## Symptom

{{What a person sees when the rule is being broken, so the next person recognises it before it costs them.}}
