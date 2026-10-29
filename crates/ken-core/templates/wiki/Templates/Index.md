---
title: "Templates"
aliases: ["Templates", "page templates", "how do I start a new page", "which blank do I copy"]
status: current
audience: dev
updated: {{date}}
---

# Templates

Starting a new page? Here are the blanks you copy it from, and where each copy goes. A page is made from its blank the first time something writes it, so the library never holds a page of placeholders. Not here: the pages every library has from set-up → [[Start Here]].

[[#Blanks]]

## Blanks

| blank | copy to | one per |
|---|---|---|
| `Rule.md` | `Rules/{{The Rule as a Sentence}}.md` | rule |
| `Component-Library.md` | `Rules/Component-Library.md` | component library · a person copies it |
| `Design-Note.md` | `Design/{{Question}}.md` | design question |
| `Roadmap.md` | `Current/Roadmap.md` | library |
| `Feature-Status.md` | `Current/Feature-Status.md` | code project |
| `How-to.md` | `Reference/{{How to Do the Thing}}.md` | task or runbook |
| `Platform.md` | `Reference/Platform/{{Dependency}}.md` | dependency we build on |
| `Finding.md` | `Research/Findings/{{date}}-{{question}}.md` | question a session answered |
| `Ingested-note.md` | `Research/Ingestion/Ingested/{{Meetings, Recordings, Documents, Notes or Sessions}}/{{YYYY-MM}}/{{date}}-{{name}}.md` | source dropped in `Raw/` · Ken writes these |
| `Ticket.md` | `tickets/{{KEY}}-nnn.md` | piece of work |

A copy isn't done until:

- it keeps every frontmatter key
- it fills every `{{placeholder}}` or deletes its row
- it's linked from its section's index

If a page type keeps recurring (a stakeholder, say, or a deliverable outline), it gets its own blank when the team needs one. That blank carries `type:` and the fields every page of that type has to fill, one `key: value` per line.
