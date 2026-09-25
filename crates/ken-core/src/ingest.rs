//! The library inbox (Ways of Working, docs-system "Ingest"): a source
//! dropped in `Research/Ingestion/Raw/` is read, distilled into one dated
//! note from `Templates/Ingested-note.md`, and moved beside the note under
//! `Research/Ingestion/Ingested/`. A person reads the key takeaways on one
//! Review card and undoes what is wrong; nobody confirms a note.
//!
//! This is the Ingest half of what Ken called ingests; the other half, a
//! stored rule that keeps an output page fresh from its sources, is a
//! recipe (`recipe.rs`), and stays one.
//!
//! The model call is passed in (`generate`), so the app runs Claude headless
//! and a test runs a stand-in. What follows from a note is proposed, never
//! written ([`follow_ups`]): a change to each Current page it overturns, and
//! a decisions-log entry for each ruling said in the room, left for its
//! decider. Actions become tickets in the team repo, which is Wright's; the
//! card lists them.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::{Error, Result};

pub const RAW: &str = "Research/Ingestion/Raw";
pub const INGESTED: &str = "Research/Ingestion/Ingested";
pub const TEMPLATE: &str = "Templates/Ingested-note.md";
pub const REVIEW_KIND: &str = "ingest";

/// The method's note template, used when the library has none of its own.
const DEFAULT_TEMPLATE: &str = "---\ntitle: \"{{What it was}} - {{date}}\"\naliases: [\"{{What it was}} - {{date}}\"]\nstatus: evidence\nupdated: {{date}}\nsource: {{source path}}\npresent: [{{who was there}}]\n---\n\n# {{What it was}} - {{date}}\n\nEvidence, at the time. Cite it; do not read it as current doctrine.\n\nSource: `{{source file}}` ({{length: turns, minutes or pages}}).\n\n## What It Overturns\n\n**{{The one thing that changes what we wrote.}}** {{Which page said otherwise.}}\n\n## What Was Said\n\n- **{{Topic}}.** {{Name}}: *\"{{quote}}\"* → {{what follows from it}}.\n\n## Rulings Said in the Room\n\n- {{The ruling, in the decider's words}} — {{decider}}.\n\n## Actions and Requests\n\n- {{Action}} → {{who}}.\n\n## Open Questions\n\n- {{Question}} → {{who}}.\n";

/// Where one ingested source ends up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    /// Where it was dropped (`Research/Ingestion/Raw/standup.txt`).
    pub raw: String,
    /// The note (`Research/Ingestion/Ingested/2026-09-24-standup.md`).
    pub note: String,
    /// The source, moved beside the note
    /// (`Research/Ingestion/Ingested/2026-09-24-standup/standup.txt`).
    pub source: String,
}

/// What an ingest card carries: where things went, and a hash of the note
/// as written, so Undo can tell whether a person has edited it since.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    #[serde(flatten)]
    pub placement: Placement,
    pub note_hash: String,
}

fn hash(text: &str) -> String {
    format!("{:016x}", twox_hash::XxHash64::oneshot(0, text.as_bytes()))
}

/// Whether this project has the inbox at all.
pub fn has_inbox(root: &Path) -> bool {
    root.join(RAW).is_dir()
}

/// Sources waiting in `Raw/`, oldest name first: every file there except
/// dotfiles and an index page.
pub fn waiting(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = fs::read_dir(root.join(RAW))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| !n.starts_with('.') && !n.eq_ignore_ascii_case("index.md") && !n.eq_ignore_ascii_case("readme.md"))
        .map(|n| format!("{RAW}/{n}"))
        .collect();
    out.sort();
    out
}

fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if s.is_empty() { "source".into() } else { s }
}

/// Where a source goes on `date`, not colliding with an earlier one.
pub fn place(root: &Path, raw: &str, date: &str) -> Placement {
    let file = raw.rsplit('/').next().unwrap_or(raw).to_string();
    let stem = file.rsplit_once('.').map_or(file.as_str(), |(s, _)| s);
    let mut base = format!("{date}-{}", slug(stem));
    let mut n = 2;
    while root.join(INGESTED).join(format!("{base}.md")).exists() || root.join(INGESTED).join(&base).exists() {
        base = format!("{date}-{}-{n}", slug(stem));
        n += 1;
    }
    Placement {
        raw: raw.to_string(),
        note: format!("{INGESTED}/{base}.md"),
        source: format!("{INGESTED}/{base}/{file}"),
    }
}

/// The prompt: the template, the source's text, and the rules the method
/// sets for a note (quotes verbatim, evidence not doctrine, what it
/// overturns first).
pub fn prompt(template: &str, placement: &Placement, text: &str, date: &str) -> String {
    let file = placement.raw.rsplit('/').next().unwrap_or(&placement.raw);
    format!(
        "You are distilling one source from a team's library inbox into one dated note.\n\n\
         Fill in this template. Replace every {{{{…}}}} placeholder; drop a section only if the \
         source has nothing for it, and say so in one line. Today is {date}. The source file is \
         `{file}` and will be kept at `{source}`: use that for `source:`.\n\n\
         Rules:\n\
         - Quotes are the speaker's exact words, typos included, marked [sic]. Never paraphrase inside quotes.\n\
         - This is evidence at the time, not current doctrine. Do not state anything the source does not.\n\
         - Lead \"What It Overturns\" with the one thing that changes what the team wrote, or say nothing overturns.\n\
         - Name who said what. List who was present if the source shows it.\n\
         - Reply with the finished note only, in Markdown, starting with its `---` frontmatter. No preamble, no code fences.\n\n\
         TEMPLATE:\n{template}\n\nSOURCE ({file}):\n{text}\n",
        source = placement.source,
    )
}

/// The model's reply as a note: fences stripped, and the frontmatter
/// `source:` set to where the source really went.
pub fn finish_note(reply: &str, placement: &Placement) -> Result<String> {
    let mut t = reply.trim();
    if let Some(rest) = t.strip_prefix("```markdown").or_else(|| t.strip_prefix("```md")).or_else(|| t.strip_prefix("```")) {
        t = rest.trim_start();
        t = t.strip_suffix("```").unwrap_or(t).trim_end();
    }
    if !t.starts_with("---") {
        return Err(Error::Other("the note did not start with its frontmatter".into()));
    }
    let mut out = String::with_capacity(t.len() + 64);
    let mut in_fm = false;
    let mut fm_done = false;
    let mut wrote_source = false;
    for (i, line) in t.lines().enumerate() {
        if line.trim_end() == "---" {
            if i == 0 {
                in_fm = true;
            } else if in_fm && !fm_done {
                if !wrote_source {
                    out.push_str(&format!("source: {}\n", placement.source));
                }
                in_fm = false;
                fm_done = true;
            }
            out.push_str("---\n");
            continue;
        }
        if in_fm && line.trim_start().starts_with("source:") {
            out.push_str(&format!("source: {}\n", placement.source));
            wrote_source = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !fm_done {
        return Err(Error::Other("the note's frontmatter was never closed".into()));
    }
    Ok(out)
}

/// The card's body: the note's "What It Overturns" and "Actions and
/// Requests" sections, which are what a person reads to undo a wrong row.
pub fn card_body(note: &str, placement: &Placement) -> String {
    let mut s = String::new();
    for heading in ["## What It Overturns", "## Actions and Requests", "## Rulings Said in the Room"] {
        if let Some(start) = note.find(heading) {
            let rest = &note[start..];
            let end = rest[heading.len()..].find("\n## ").map_or(rest.len(), |e| e + heading.len());
            s.push_str(rest[..end].trim());
            s.push_str("\n\n");
        }
    }
    s.push_str(&format!("Note: {}\nSource moved to: {}\n", placement.note, placement.source));
    s
}

/// Read one waiting source, write its note, move the source beside it and
/// file the Review card. `generate` turns the prompt into the note.
pub fn ingest_one(
    root: &Path,
    db: &mut Db,
    raw: &str,
    date: &str,
    now: i64,
    generate: impl FnOnce(&str) -> Result<String>,
) -> Result<Placement> {
    let abs = root.join(raw);
    let text = crate::extract::extract(&abs)?.text;
    if text.trim().is_empty() {
        return Err(Error::Other(format!("{raw} has no text Ken can read (a recording needs its transcript first)")));
    }
    let placement = place(root, raw, date);
    let template = fs::read_to_string(root.join(TEMPLATE)).unwrap_or_else(|_| DEFAULT_TEMPLATE.to_string());
    let note = finish_note(&generate(&prompt(&template, &placement, &text, date))?, &placement)?;

    let note_abs = root.join(&placement.note);
    let src_abs = root.join(&placement.source);
    if let Some(dir) = src_abs.parent() {
        fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    fs::write(&note_abs, &note).map_err(|e| Error::io(&note_abs, e))?;
    fs::rename(&abs, &src_abs).map_err(|e| Error::io(&abs, e))?;

    let stem = placement.note.rsplit('/').next().unwrap_or(&placement.note).trim_end_matches(".md");
    let card = Card { placement: placement.clone(), note_hash: hash(&note) };
    let payload = serde_json::to_string(&card).map_err(|e| Error::Other(e.to_string()))?;
    db.insert_review_item(REVIEW_KIND, &format!("Ingested: {stem}"), &card_body(&note, &placement), &placement.note, Some(&payload), now)?;
    Ok(placement)
}

// --- What follows from a note: Current pages rewritten, rulings for their
// deciders. Both are proposals on Review, never writes: a person applies a
// Current change, and a ruling waits for the person who decides it. -------

/// The Current pages an ingested note may change, when the library has them.
pub const CURRENT_PAGES: [&str; 3] = ["Current/Project.md", "Current/Team.md", "Current/Who-Does-What.md"];

/// A section's bullet lines, placeholders and "nothing" lines left out.
fn section_bullets(note: &str, heading: &str) -> Vec<String> {
    let Some(start) = note.find(heading) else { return Vec::new() };
    let rest = &note[start + heading.len()..];
    let body = &rest[..rest.find("\n## ").unwrap_or(rest.len())];
    body.lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("- "))
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.contains("{{") && !l.to_lowercase().starts_with("none") && !l.to_lowercase().starts_with("nothing"))
        .map(str::to_string)
        .collect()
}

/// Whether the note says something overturns what the team wrote.
fn overturns(note: &str) -> bool {
    let Some(start) = note.find("## What It Overturns") else { return false };
    let rest = &note[start + "## What It Overturns".len()..];
    let body = rest[..rest.find("\n## ").unwrap_or(rest.len())].trim().to_lowercase();
    !body.is_empty() && !body.contains("{{") && !body.starts_with("nothing") && !body.contains("nothing overturns")
}

/// "The ruling — decider" lines from the note's "Rulings Said in the Room".
pub fn rulings_of(note: &str) -> Vec<(String, Option<String>)> {
    section_bullets(note, "## Rulings Said in the Room")
        .into_iter()
        .map(|l| match l.rsplit_once(" — ") {
            Some((ruling, decider)) => (ruling.trim().to_string(), Some(decider.trim().trim_end_matches('.').to_string())),
            None => (l, None),
        })
        .collect()
}

/// The decisions log with a new entry appended: the next `D-nnn`, today, the
/// ruling in the decider's words, and the note it was said in as its source.
pub fn decisions_entry(log: &str, ruling: &str, decider: Option<&str>, date: &str, note_stem: &str) -> String {
    let next = log
        .lines()
        .filter_map(|l| l.trim().strip_prefix("D-"))
        .filter_map(|r| r.split(|c: char| !c.is_ascii_digit()).next()?.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    let topic: String = ruling.split_whitespace().take(6).collect::<Vec<_>>().join(" ");
    let mut out = log.trim_end().to_string();
    out.push_str(&format!(
        "\n\nD-{next:03} · {date} · {topic} — {ruling}\n  Why: said in the room; see the note.\n  sources: [[{note_stem}]]\n"
    ));
    if let Some(d) = decider {
        out.push_str(&format!("  decider: {d}\n"));
    }
    out
}

/// What follow-ups an ingest filed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowUps {
    /// Current pages with a proposed change.
    pub current: Vec<String>,
    /// Rulings proposed for the decisions log.
    pub rulings: usize,
}

/// After a note is written: propose a change to each Current page the note
/// changes (only when it says something overturns), and a decisions-log
/// entry for each ruling said in the room, each as its own Review card.
/// `generate` is asked once per Current page that exists.
pub fn follow_ups(
    root: &Path,
    db: &mut Db,
    placement: &Placement,
    note: &str,
    date: &str,
    now: i64,
    mut generate: impl FnMut(&str) -> Result<String>,
) -> Result<FollowUps> {
    let mut out = FollowUps::default();
    let stem = placement.note.rsplit('/').next().unwrap_or(&placement.note).trim_end_matches(".md").to_string();
    let source = [crate::wikidraft::Source { label: format!("[[{stem}]]"), text: note.to_string() }];

    if overturns(note) {
        for page in CURRENT_PAGES {
            let Ok(existing) = fs::read_to_string(root.join(page)) else { continue };
            let change = "rewrite what the note changes about what is true now, citing the note as [[its name]]; keep every other line";
            let reply = generate(&crate::wikidraft::update_prompt(page, change, &existing, &source, date))
                .and_then(|r| crate::wikidraft::finish_update(&r, &existing));
            if let Ok(Some(proposed)) = reply {
                let p = crate::wikidraft::Proposal { page: page.to_string(), base: existing, proposed };
                let title = format!("Proposed: {page} from {stem}");
                let body = format!(
                    "The note {stem} says something here is no longer true. Ken proposes this change to {page}; apply it \
                     to write it, or discard it. It applies only while the page still reads as it did."
                );
                crate::wikidraft::file_page_proposal(db, &p, &title, &body, now)?;
                out.current.push(page.to_string());
            }
        }
    }

    let rulings = rulings_of(note);
    if !rulings.is_empty() {
        let log = db.paths_named(&["decisions.md"])?.into_iter().next().unwrap_or_else(|| "_meta/DECISIONS.md".to_string());
        let mut text = fs::read_to_string(root.join(&log)).unwrap_or_else(|_| "# Decisions\n".to_string());
        for (ruling, decider) in &rulings {
            let proposed = decisions_entry(&text, ruling, decider.as_deref(), date, &stem);
            let p = crate::wikidraft::Proposal { page: log.clone(), base: text.clone(), proposed: proposed.clone() };
            let who = decider.as_deref().unwrap_or("its decider");
            let title = format!("Ruling for {who}: {}", ruling.chars().take(60).collect::<String>());
            let body = format!(
                "Said in the room, per {stem}: \"{ruling}\". A ruling is its decider's to record: {who} applies this entry \
                 to the decisions log, or discards it."
            );
            crate::wikidraft::file_page_proposal(db, &p, &title, &body, now)?;
            // The next ruling's entry follows this one, so both can apply in turn.
            text = proposed;
            out.rulings += 1;
        }
    }
    Ok(out)
}

/// Undo one ingest: the source goes back to `Raw/`, and the note is removed
/// unless someone edited it since (then it is kept and said so). Returns
/// whether the note was removed.
pub fn undo(root: &Path, card: &Card) -> Result<bool> {
    let placement = &card.placement;
    let src = root.join(&placement.source);
    let raw = root.join(&placement.raw);
    if src.exists() {
        if let Some(dir) = raw.parent() {
            fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        }
        fs::rename(&src, &raw).map_err(|e| Error::io(&src, e))?;
        if let Some(dir) = src.parent() {
            let _ = fs::remove_dir(dir); // only when empty
        }
    }
    let note = root.join(&placement.note);
    let untouched = match fs::read_to_string(&note) {
        Ok(now) => hash(&now) == card.note_hash,
        Err(_) => return Ok(false),
    };
    if untouched {
        fs::remove_file(&note).map_err(|e| Error::io(&note, e))?;
    }
    Ok(untouched)
}

/// The card stored on an ingest Review item.
pub fn card_of(payload: Option<&str>) -> Option<Card> {
    payload.and_then(|p| serde_json::from_str(p).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join(RAW)).unwrap();
        fs::create_dir_all(d.path().join(INGESTED)).unwrap();
        fs::write(d.path().join(RAW).join("Standup notes.txt"), "Ana: \"we ship on Friday\". Ben will fix the save bug.\n").unwrap();
        fs::write(d.path().join(RAW).join(".DS_Store"), "x").unwrap();
        d
    }

    const REPLY: &str = "```markdown\n---\ntitle: \"Standup - 2026-09-24\"\nstatus: evidence\nsource: wrong/path.txt\npresent: [Ana, Ben]\n---\n\n# Standup - 2026-09-24\n\n## What It Overturns\n\n**Ship date is Friday, not Monday.** Current/Project.md said Monday.\n\n## What Was Said\n\n- **Ship.** Ana: *\"we ship on Friday\"*.\n\n## Actions and Requests\n\n- Fix the save bug → Ben.\n```";

    #[test]
    fn a_source_becomes_a_dated_note_beside_it_with_one_review_card() {
        let d = library();
        let root = d.path();
        assert_eq!(waiting(root), vec![format!("{RAW}/Standup notes.txt")]);
        let mut db = Db::open_in_memory().unwrap();
        let mut seen_prompt = String::new();
        let p = ingest_one(root, &mut db, &format!("{RAW}/Standup notes.txt"), "2026-09-24", 100, |prompt| {
            seen_prompt = prompt.to_string();
            Ok(REPLY.to_string())
        })
        .unwrap();

        assert_eq!(p.note, format!("{INGESTED}/2026-09-24-standup-notes.md"));
        assert_eq!(p.source, format!("{INGESTED}/2026-09-24-standup-notes/Standup notes.txt"));
        assert!(seen_prompt.contains("we ship on Friday") && seen_prompt.contains("What It Overturns"));
        assert!(root.join(&p.source).exists() && !root.join(&p.raw).exists(), "the source moved");
        let note = fs::read_to_string(root.join(&p.note)).unwrap();
        assert!(note.starts_with("---\n"), "fences stripped");
        assert!(note.contains(&format!("source: {}", p.source)) && !note.contains("wrong/path"));
        assert!(waiting(root).is_empty());

        let (id, body) = db.open_review_item_of_kind(REVIEW_KIND).unwrap().unwrap();
        assert!(body.contains("Ship date is Friday") && body.contains("Fix the save bug"));
        assert!(id > 0);

        // Undo puts it back: the source in Raw, the note gone.
        let open = db.list_open_review_items().unwrap();
        let card = card_of(open[0].payload.as_deref()).unwrap();
        assert_eq!(card.placement, p);
        assert!(undo(root, &card).unwrap());
        assert_eq!(waiting(root), vec![format!("{RAW}/Standup notes.txt")]);
        assert!(!root.join(&p.note).exists());
    }

    #[test]
    fn an_edited_note_survives_undo_and_a_second_source_does_not_collide() {
        let d = library();
        let root = d.path();
        let mut db = Db::open_in_memory().unwrap();
        let raw = format!("{RAW}/Standup notes.txt");
        let p = ingest_one(root, &mut db, &raw, "2026-09-24", 1, |_| Ok(REPLY.into())).unwrap();
        let written = fs::read_to_string(root.join(&p.note)).unwrap();
        fs::write(root.join(&p.note), format!("{written}\nA person added this.\n")).unwrap();
        let card = Card { placement: p.clone(), note_hash: hash(&written) };
        assert!(!undo(root, &card).unwrap(), "edited: kept");
        assert!(root.join(&p.note).exists());

        let again = place(root, &raw, "2026-09-24");
        assert_eq!(again.note, format!("{INGESTED}/2026-09-24-standup-notes-2.md"));
    }

    #[test]
    fn a_reply_without_frontmatter_is_refused_and_nothing_moves() {
        let d = library();
        let root = d.path();
        let mut db = Db::open_in_memory().unwrap();
        let raw = format!("{RAW}/Standup notes.txt");
        assert!(ingest_one(root, &mut db, &raw, "2026-09-24", 1, |_| Ok("Sure! Here is the note.".into())).is_err());
        assert!(root.join(&raw).exists());
        assert!(db.open_review_item_of_kind(REVIEW_KIND).unwrap().is_none());
    }
    const NOTE: &str = "---\ntitle: \"Standup - 2026-09-24\"\nstatus: evidence\n---\n\n# Standup - 2026-09-24\n\n## What It Overturns\n\n**Ship date is Friday, not Monday.** Current/Project.md said Monday.\n\n## Rulings Said in the Room\n\n- We ship region saves before combat — Ana.\n- {{The ruling, in the decider's words}} — {{decider}}.\n\n## Actions and Requests\n\n- Fix the save bug → Ben.\n";

    #[test]
    fn rulings_are_read_with_their_decider_and_placeholders_skipped() {
        assert_eq!(rulings_of(NOTE), vec![("We ship region saves before combat".to_string(), Some("Ana".to_string()))]);
        assert!(rulings_of("## Rulings Said in the Room\n\n- None said.\n").is_empty());
    }

    #[test]
    fn a_ruling_becomes_the_next_decisions_entry() {
        let log = "# Decisions\n\nD-001 · 2026-09-01 · saves — Worlds save as regions.\n  sources: [[Save]]\n\nD-007 · 2026-09-10 · combat — Later.\n";
        let out = decisions_entry(log, "We ship region saves before combat", Some("Ana"), "2026-09-24", "2026-09-24-standup");
        assert!(out.starts_with(log.trim_end()), "the log is appended to, never rewritten");
        assert!(out.contains("D-008 · 2026-09-24 · We ship region saves before combat — We ship region saves before combat"));
        assert!(out.contains("sources: [[2026-09-24-standup]]") && out.contains("decider: Ana"));
        assert!(decisions_entry("# Decisions\n", "x", None, "2026-09-24", "n").contains("D-001 ·"));
    }

    #[test]
    fn a_note_proposes_current_changes_and_rulings_never_writes_them() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::create_dir_all(root.join("Current")).unwrap();
        fs::create_dir_all(root.join("_meta")).unwrap();
        let project = "---\ntitle: Project\n---\nWe ship on Monday.\n";
        fs::write(root.join("Current/Project.md"), project).unwrap();
        fs::write(root.join("Current/Team.md"), "---\ntitle: Team\n---\nAna leads.\n").unwrap();
        let log = "# Decisions\n\nD-001 · 2026-09-01 · saves — Regions.\n";
        fs::write(root.join("_meta/DECISIONS.md"), log).unwrap();
        let mut db = Db::open_in_memory().unwrap();
        crate::scan::scan(&crate::project::Project::create(root, "Wiki").unwrap(), &mut db).unwrap();
        let placement = place(root, "Research/Ingestion/Raw/standup.txt", "2026-09-24");

        let mut asked = Vec::new();
        let done = follow_ups(root, &mut db, &placement, NOTE, "2026-09-24", 9, |p| {
            asked.push(p.to_string());
            Ok(if p.contains("`Current/Project.md`") {
                "---\ntitle: Project\n---\nWe ship on Friday ([[2026-09-24-standup]]).\n".into()
            } else {
                "NO CHANGE".into()
            })
        })
        .unwrap();
        assert_eq!(done, FollowUps { current: vec!["Current/Project.md".into()], rulings: 1 });
        assert_eq!(asked.len(), 2, "one ask per Current page that exists");
        assert_eq!(fs::read_to_string(root.join("Current/Project.md")).unwrap(), project, "proposed, not written");
        assert_eq!(fs::read_to_string(root.join("_meta/DECISIONS.md")).unwrap(), log);
        let items = db.list_open_review_items().unwrap();
        let titles: Vec<&str> = items.iter().filter(|i| i.kind == crate::wikidraft::PROPOSAL_KIND).map(|i| i.title.as_str()).collect();
        assert!(titles.iter().any(|t| t.starts_with("Ruling for Ana:")), "{titles:?}");
        assert!(titles.contains(&"Proposed: Current/Project.md from 2026-09-24-standup"), "{titles:?}");

        // Nothing overturned: Current pages are not asked about.
        let quiet = NOTE.replace("**Ship date is Friday, not Monday.** Current/Project.md said Monday.", "Nothing overturns.");
        let mut asked_again = 0;
        follow_ups(root, &mut db, &placement, &quiet, "2026-09-24", 9, |_| {
            asked_again += 1;
            Ok("NO CHANGE".into())
        })
        .unwrap();
        assert_eq!(asked_again, 0);
    }
}
