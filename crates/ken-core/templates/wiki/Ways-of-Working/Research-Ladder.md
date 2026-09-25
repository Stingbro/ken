---
aliases: ["Research ladder", "where do I look", "what order do I search in"]
status: current
updated: {{date}}
verified: {{date}}      # a person read this against its sources; never stamped in bulk
---

# The Research Ladder

Which source wins when two disagree, and the order to search them in.

## Authority

On what the system does, the lower row wins. Where a page records what was agreed and the code differs, escalate it: a person decides whether the page, the ruling or the code is wrong, and the page is never edited down to the code.

| rung | what it is |
|---|---|
| outside docs | a summary of something else; weakest |
| our library | what we wrote about our own work |
| our code | what we built; wins over the library |
| the platform source | the upstream code we build on; the final word on what it does |

## Search Order

Cheapest first, with the team repo after the library, because the cheap sources tell you where to look in the expensive ones:

1. the library, starting at [[START-HERE]] and [[Vocabulary]]
2. the team repo: tickets and the decisions log
3. our code and the data files, by grep
4. the platform's config and source

The instant a question takes the shape "does X exist" or "what are the options for X", the next action is a library search. Search the library three ways at once: the knowledge graph, search, and the links between its pages. An empty result proves nothing until you have ruled out a wrong term, a filtered path and a timed-out tool; for a wrong term, check [[Vocabulary]]. Before you report that something does not exist, search all four sources in the search order. Name what you searched.

A finding lists every page it read. It marks each page accurate, or stale and corrected. It also lists each page it found missing. When you find the answer below the library, write it back into the page that owns it before you close the question.
