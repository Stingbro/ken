---
title: "Ingestion"
aliases: ["Ingestion", "the inbox", "where do I drop a meeting recording", "where do transcripts go"]
status: current
audience: business
updated: {{date}}
---

# Ingestion

A meeting ends, a recording lands or someone pastes in notes, and none of it is in the library yet. It comes into this inbox first: meeting transcripts, recordings, design sessions, documents, exports and pasted notes. Not here: what's true now → [[Current]]; a ruling once it's confirmed → the decisions log; a dated finding from a session → [[Research]].

[[#Folders]] · [[#Workflow]]

## Folders

| folder | holds | rule |
|---|---|---|
| `Raw/` | what was dropped, each filename dated or naming its source | empties · never a research source |
| `Ingested/` | one dated note per source, with the source filed beside it | evidence from its date · cited, never read as current rules · never a research source |

## Workflow

1. Drop the source in `Raw/`.
2. Ken writes it up from `Templates/Ingested-note.md`, keeping the sections that kind of source needs.
3. The pages it changes get updated, one section at a time, citing the note. An edit that needs a person waits on the source's card.
4. What it contradicts and the questions it leaves open go to the findings list. A ruling waits for one confirm into the decisions log, a risk or issue for one confirm into [[RAID]], and an action becomes a proposed ticket.
5. The note, the source and every page it changed go in as one commit, with one line in `log.md`.

Nobody edits a source. If a copy is shorter than its source, it says where the full one is.
