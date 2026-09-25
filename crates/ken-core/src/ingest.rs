//! The library inbox (Ways of Working, docs-system "Ingest"), in two parts.
//!
//! **Processing.** A source dropped in `Research/Ingestion/Raw/` is read and
//! distilled into one dated note from `Templates/Ingested-note.md` (the
//! summary, what it overturns, the quotes, rulings, actions), written
//! straight into its organized home. What follows from it is proposed on
//! Review, never written ([`follow_ups`]): a change to each wiki page it
//! touches, a new page where nothing covers a topic yet, and a decisions-log
//! entry for each ruling said in the room, left for its decider. The source
//! stays in `Raw/` while this is reviewed, and is not read again.
//!
//! **Filing.** When the person is done ([`file`]), the source moves beside
//! its note, under `Research/Ingestion/Ingested/<Meetings|Recordings|
//! Documents>/<YYYY-MM>/`. Undo takes either part back.
//!
//! This is the Ingest half of what Ken called ingests; the other half, a
//! stored rule that keeps an output page fresh from its sources, is a
//! recipe (`recipe.rs`), and stays one. Actions become tickets in the team
//! repo, which is Wright's; the card lists them. The model call is passed in
//! (`generate`), so the app runs Claude headless and a test a stand-in.

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
const DEFAULT_TEMPLATE: &str = "---\ntitle: \"{{What it was}} - {{date}}\"\naliases: [\"{{What it was}} - {{date}}\"]\nstatus: evidence\nkind: {{meeting, recording or document}}\nupdated: {{date}}\nsource: {{source path}}\npresent: [{{who was there}}]\n---\n\n# {{What it was}} - {{date}}\n\nEvidence, at the time. Cite it; do not read it as current doctrine.\n\nSource: `{{source file}}` ({{length: turns, minutes or pages}}).\n\n## What It Overturns\n\n**{{The one thing that changes what we wrote.}}** {{Which page said otherwise.}}\n\n## What Was Said\n\n- **{{Topic}}.** {{Name}}: *\"{{quote}}\"* → {{what follows from it}}.\n\n## Rulings Said in the Room\n\n- {{The ruling, in the decider's words}} — {{decider}}.\n\n## Actions and Requests\n\n- {{Action}} → {{who}}.\n\n## Open Questions\n\n- {{Question}} → {{who}}.\n";

/// What a source was, which decides its folder once filed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Meeting,
    Recording,
    Document,
}

impl SourceKind {
    pub fn folder(self) -> &'static str {
        match self {
            SourceKind::Meeting => "Meetings",
            SourceKind::Recording => "Recordings",
            SourceKind::Document => "Documents",
        }
    }
}

/// Where one ingested source ends up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    /// Where it was dropped (`Research/Ingestion/Raw/standup.txt`).
    pub raw: String,
    /// The note (`…/Ingested/Meetings/2026-09/2026-09-24-standup.md`).
    pub note: String,
    /// Where the source goes when filed, beside the note
    /// (`…/Ingested/Meetings/2026-09/2026-09-24-standup/standup.txt`).
    pub source: String,
}

/// What an ingest card carries: where things went, a hash of the note as
/// written (so Undo can tell whether a person has edited it since), and
/// whether the source has been filed yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    #[serde(flatten)]
    pub placement: Placement,
    pub note_hash: String,
    /// The source moved beside its note. Cards from before the two parts
    /// were always filed.
    #[serde(default = "filed_before")]
    pub filed: bool,
}

fn filed_before() -> bool {
    true
}

fn hash(text: &str) -> String {
    format!("{:016x}", twox_hash::XxHash64::oneshot(0, text.as_bytes()))
}

/// Whether this project has the inbox at all.
pub fn has_inbox(root: &Path) -> bool {
    root.join(RAW).is_dir()
}

/// Files in `Raw/`, oldest name first: every file there except dotfiles and
/// an index page. Includes sources being reviewed; see [`waiting_new`].
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

/// Sources in `Raw/` whose note is written and waiting for the person
/// (an open, unfiled ingest card).
pub fn in_review(db: &Db) -> Result<Vec<String>> {
    Ok(db
        .list_open_review_items()?
        .into_iter()
        .filter(|it| it.kind == REVIEW_KIND)
        .filter_map(|it| card_of(it.payload.as_deref()))
        .filter(|c| !c.filed)
        .map(|c| c.placement.raw)
        .collect())
}

/// Sources in `Raw/` not yet processed: [`waiting`] less those in review.
pub fn waiting_new(root: &Path, db: &Db) -> Result<Vec<String>> {
    let reviewing = in_review(db)?;
    Ok(waiting(root).into_iter().filter(|r| !reviewing.contains(r)).collect())
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

/// Where a source of `kind` goes on `date` (`YYYY-MM-DD`): its kind's
/// folder, then the month, not colliding with an earlier one.
pub fn place(root: &Path, raw: &str, date: &str, kind: SourceKind) -> Placement {
    let file = raw.rsplit('/').next().unwrap_or(raw).to_string();
    let stem = file.rsplit_once('.').map_or(file.as_str(), |(s, _)| s);
    let month = date.get(..7).unwrap_or(date);
    let dir = format!("{INGESTED}/{}/{month}", kind.folder());
    let mut base = format!("{date}-{}", slug(stem));
    let mut n = 2;
    while root.join(&dir).join(format!("{base}.md")).exists() || root.join(&dir).join(&base).exists() {
        base = format!("{date}-{}-{n}", slug(stem));
        n += 1;
    }
    Placement {
        raw: raw.to_string(),
        note: format!("{dir}/{base}.md"),
        source: format!("{dir}/{base}/{file}"),
    }
}

/// What kind of source a note came from: its frontmatter `kind:` when the
/// model set it, else a transcript file is a recording, a source with people
/// present a meeting, and anything else a document.
pub fn kind_of(note: &str, raw: &str) -> SourceKind {
    let field = |key: &str| {
        note.lines()
            .skip(1)
            .take_while(|l| l.trim_end() != "---")
            .find_map(|l| l.trim().strip_prefix(key).map(|v| v.trim().trim_matches('"').to_lowercase()))
    };
    match field("kind:").as_deref() {
        Some(k) if k.starts_with("meeting") => return SourceKind::Meeting,
        Some(k) if k.starts_with("recording") => return SourceKind::Recording,
        Some(k) if k.starts_with("document") => return SourceKind::Document,
        _ => {}
    }
    let ext = raw.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    if matches!(ext.as_str(), "vtt" | "srt") {
        return SourceKind::Recording;
    }
    let present = field("present:").unwrap_or_default();
    let people = present.trim_matches(|c| c == '[' || c == ']').trim();
    if !people.is_empty() && !people.contains("{{") {
        SourceKind::Meeting
    } else {
        SourceKind::Document
    }
}

/// The prompt: the template, the source's text, and the rules the method
/// sets for a note (quotes verbatim, evidence not doctrine, what it
/// overturns first).
pub fn prompt(template: &str, raw: &str, text: &str, date: &str) -> String {
    let file = raw.rsplit('/').next().unwrap_or(raw);
    format!(
        "You are distilling one source from a team's library inbox into one dated note.\n\n\
         Fill in this template. Replace every {{{{…}}}} placeholder; drop a section only if the \
         source has nothing for it, and say so in one line. Today is {date}. The source file is \
         `{file}`.\n\n\
         Rules:\n\
         - Quotes are the speaker's exact words, typos included, marked [sic]. Never paraphrase inside quotes.\n\
         - This is evidence at the time, not current doctrine. Do not state anything the source does not.\n\
         - Lead \"What It Overturns\" with the one thing that changes what the team wrote, or say nothing overturns.\n\
         - Name who said what. List who was present if the source shows it.\n\
         - Set `kind:` in the frontmatter to meeting (people talking: a standup, review or call), recording (a \
           transcript of audio or video that is not a meeting), or document (anything written).\n\
         - Reply with the finished note only, in Markdown, starting with its `---` frontmatter. No preamble, no code fences.\n\n\
         TEMPLATE:\n{template}\n\nSOURCE ({file}):\n{text}\n",
    )
}

/// The model's reply as a note: fences stripped, and the frontmatter
/// `source:` set to where the source is filed.
pub fn finish_note(reply: &str, placement: &Placement) -> Result<String> {
    let t = strip_fences(reply);
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

fn strip_fences(reply: &str) -> &str {
    let mut t = reply.trim();
    if let Some(rest) = t.strip_prefix("```markdown").or_else(|| t.strip_prefix("```md")).or_else(|| t.strip_prefix("```json")).or_else(|| t.strip_prefix("```")) {
        t = rest.trim_start();
        t = t.strip_suffix("```").unwrap_or(t).trim_end();
    }
    t
}

/// The card's body: the note's "What It Overturns" and "Actions and
/// Requests" sections, which are what a person reads to undo a wrong row,
/// and what filing will do.
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
    let folder = placement.note.rsplit_once('/').map_or(placement.note.as_str(), |(d, _)| d);
    s.push_str(&format!(
        "Note: {}\nProposed changes to the wiki follow on their own cards. When you are done with them, \
         **Done, file it** moves the source from Raw to {folder}/.\n",
        placement.note
    ));
    s
}

/// Part one: read one new source, write its note into its organized home and
/// file the Review card. The source stays in `Raw/` until [`file`].
/// `generate` turns the prompt into the note.
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
    let template = fs::read_to_string(root.join(TEMPLATE)).unwrap_or_else(|_| DEFAULT_TEMPLATE.to_string());
    let reply = generate(&prompt(&template, raw, &text, date))?;
    let kind = kind_of(strip_fences(&reply), raw);
    let placement = place(root, raw, date, kind);
    let note = finish_note(&reply, &placement)?;

    let note_abs = root.join(&placement.note);
    if let Some(dir) = note_abs.parent() {
        fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    fs::write(&note_abs, &note).map_err(|e| Error::io(&note_abs, e))?;

    let stem = placement.note.rsplit('/').next().unwrap_or(&placement.note).trim_end_matches(".md");
    let card = Card { placement: placement.clone(), note_hash: hash(&note), filed: false };
    let payload = serde_json::to_string(&card).map_err(|e| Error::Other(e.to_string()))?;
    db.insert_review_item(REVIEW_KIND, &format!("Ingested: {stem}"), &card_body(&note, &placement), &placement.note, Some(&payload), now)?;
    Ok(placement)
}

/// Part two: the person is done with a note; its source moves from `Raw/`
/// beside it. Returns the card, filed.
pub fn file(root: &Path, card: &Card) -> Result<Card> {
    if card.filed {
        return Ok(card.clone());
    }
    let raw = root.join(&card.placement.raw);
    let dest = root.join(&card.placement.source);
    if !raw.exists() {
        return Err(Error::Other(format!("{} is no longer in Raw", card.placement.raw)));
    }
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    fs::rename(&raw, &dest).map_err(|e| Error::io(&raw, e))?;
    Ok(Card { filed: true, ..card.clone() })
}

// --- What follows from a note: pages it changes, pages it calls for,
// rulings for their deciders. All are proposals on Review, never writes: a
// person applies a page change, and a ruling waits for its decider. -------

/// Sections an ingest never proposes changes to: the method routes a change
/// to how the team works through a ticket, and evidence stays evidence.
const OFF_LIMITS: [&str; 4] = ["Ways-of-Working/", "Research/", "Templates/", "_meta/"];

/// Most existing pages and new pages one note may propose.
pub const MAX_PAGES: usize = 5;

fn may_propose(page: &str) -> bool {
    page.ends_with(".md")
        && !page.contains("..")
        && !page.starts_with('/')
        && !OFF_LIMITS.iter().any(|p| page.starts_with(p))
        && !matches!(page, "START-HERE.md" | "CLAUDE.md" | "README.md")
}

/// The wiki's pages a note could change, with their titles, for the model to
/// choose from.
fn page_list(db: &Db) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for page in db.page_paths()?.into_iter().filter(|p| may_propose(p)) {
        let title = db.page_meta(&page)?.and_then(|m| m.title).unwrap_or_default();
        out.push((page, title));
        if out.len() >= 400 {
            break;
        }
    }
    Ok(out)
}

/// The plan: which pages the note changes, which new pages it calls for.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Plan {
    #[serde(default)]
    pub update: Vec<String>,
    #[serde(default)]
    pub create: Vec<NewPage>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NewPage {
    pub path: String,
    #[serde(default)]
    pub purpose: String,
}

pub fn plan_prompt(pages: &[(String, String)], note: &str) -> String {
    let mut s = String::from(
        "A note was just ingested into a team's wiki. Decide which existing pages it changes (something they say is \
         no longer true, or something they should now say) and which new pages it calls for (a topic nothing covers \
         yet). Only what the note itself supports; most notes change a few pages, many change none.\n\n\
         Reply with JSON only: {\"update\": [\"path\", …], \"create\": [{\"path\": \"Section/Name.md\", \"purpose\": \"what the page is for\"}, …]}. \
         Use paths from the list for updates; put a new page in the section it belongs to (Current, Platform, \
         Design, Conventions, Work, Reference). At most 5 of each.\n\nPAGES IN THE WIKI:\n",
    );
    for (p, t) in pages {
        if t.is_empty() {
            s.push_str(&format!("- {p}\n"));
        } else {
            s.push_str(&format!("- {p} — {t}\n"));
        }
    }
    s.push_str(&format!("\nTHE NOTE:\n{note}\n"));
    s
}

/// The model's plan, kept to real pages to update and new pages that may be
/// made, at most [`MAX_PAGES`] of each.
pub fn read_plan(reply: &str, pages: &[(String, String)], root: &Path) -> Plan {
    let t = strip_fences(reply);
    let json = t.find('{').and_then(|a| t.rfind('}').map(|b| &t[a..=b])).unwrap_or("{}");
    let mut plan: Plan = serde_json::from_str(json).unwrap_or_default();
    plan.update.retain(|p| pages.iter().any(|(q, _)| q == p));
    plan.update.dedup();
    plan.update.truncate(MAX_PAGES);
    plan.create.retain(|n| may_propose(&n.path) && !root.join(&n.path).exists());
    plan.create.truncate(MAX_PAGES);
    plan
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
    /// Existing pages with a proposed change.
    pub updated: Vec<String>,
    /// New pages proposed.
    pub created: Vec<String>,
    /// Rulings proposed for the decisions log.
    pub rulings: usize,
}

/// After a note is written: ask which pages it changes and which new pages
/// it calls for, propose each (a diff on Review), and propose a
/// decisions-log entry for each ruling said in the room. `generate` is asked
/// once for the plan, then once per page.
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

    let pages = page_list(db)?;
    let plan = generate(&plan_prompt(&pages, note)).map(|r| read_plan(&r, &pages, root)).unwrap_or_default();
    for page in &plan.update {
        let Ok(existing) = fs::read_to_string(root.join(page)) else { continue };
        let change = "work in what the note changes or adds to this page, citing the note as [[its name]]; keep every other line";
        let reply = generate(&crate::wikidraft::update_prompt(page, change, &existing, &source, date))
            .and_then(|r| crate::wikidraft::finish_update(&r, &existing));
        if let Ok(Some(proposed)) = reply {
            let p = crate::wikidraft::Proposal { page: page.clone(), base: existing, proposed };
            let title = format!("Proposed: {page} from {stem}");
            let body = format!(
                "The note {stem} changes what {page} says. Ken proposes this change; apply it to write it, or discard \
                 it. It applies only while the page still reads as it did."
            );
            crate::wikidraft::file_page_proposal(db, &p, &title, &body, now)?;
            out.updated.push(page.clone());
        }
    }
    for new in &plan.create {
        let reply = generate(&crate::wikidraft::prompt(&new.path, &new.purpose, None, &source, date))
            .and_then(|r| crate::wikidraft::finish(&r));
        if let Ok(proposed) = reply {
            let p = crate::wikidraft::Proposal { page: new.path.clone(), base: String::new(), proposed };
            let title = format!("New page: {} from {stem}", new.path);
            let body = format!(
                "The note {stem} covers something no page does yet: {}. Ken drafted {}; create it to write it, or \
                 discard it.",
                new.purpose, new.path
            );
            crate::wikidraft::file_page_proposal(db, &p, &title, &body, now)?;
            out.created.push(new.path.clone());
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
                 to the decisions log, or discards it. With several rulings from one note, apply them in order."
            );
            crate::wikidraft::file_page_proposal(db, &p, &title, &body, now)?;
            text = proposed;
            out.rulings += 1;
        }
    }
    Ok(out)
}

/// Undo one ingest: a filed source goes back to `Raw/`, and the note is
/// removed unless someone edited it since (then it is kept and said so).
/// Returns whether the note was removed.
pub fn undo(root: &Path, card: &Card) -> Result<bool> {
    let placement = &card.placement;
    let src = root.join(&placement.source);
    let raw = root.join(&placement.raw);
    if card.filed && src.exists() {
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

    const REPLY: &str = "```markdown\n---\ntitle: \"Standup - 2026-09-24\"\nstatus: evidence\nkind: meeting\nsource: wrong/path.txt\npresent: [Ana, Ben]\n---\n\n# Standup - 2026-09-24\n\n## What It Overturns\n\n**Ship date is Friday, not Monday.** Current/Project.md said Monday.\n\n## What Was Said\n\n- **Ship.** Ana: *\"we ship on Friday\"*.\n\n## Actions and Requests\n\n- Fix the save bug → Ben.\n```";

    #[test]
    fn processing_writes_the_note_in_its_home_and_leaves_the_source_for_review() {
        let d = library();
        let root = d.path();
        let raw = format!("{RAW}/Standup notes.txt");
        assert_eq!(waiting(root), vec![raw.clone()]);
        let mut db = Db::open_in_memory().unwrap();
        let mut seen_prompt = String::new();
        let p = ingest_one(root, &mut db, &raw, "2026-09-24", 100, |prompt| {
            seen_prompt = prompt.to_string();
            Ok(REPLY.to_string())
        })
        .unwrap();

        assert_eq!(p.note, format!("{INGESTED}/Meetings/2026-09/2026-09-24-standup-notes.md"));
        assert_eq!(p.source, format!("{INGESTED}/Meetings/2026-09/2026-09-24-standup-notes/Standup notes.txt"));
        assert!(seen_prompt.contains("we ship on Friday") && seen_prompt.contains("Set `kind:`"));
        assert!(root.join(&raw).exists(), "the source waits in Raw while the note is reviewed");
        let note = fs::read_to_string(root.join(&p.note)).unwrap();
        assert!(note.starts_with("---\n") && note.contains(&format!("source: {}", p.source)) && !note.contains("wrong/path"));
        assert_eq!(waiting(root), vec![raw.clone()]);
        assert!(waiting_new(root, &db).unwrap().is_empty(), "not read twice");
        assert_eq!(in_review(&db).unwrap(), vec![raw.clone()]);

        let (_, body) = db.open_review_item_of_kind(REVIEW_KIND).unwrap().unwrap();
        assert!(body.contains("Ship date is Friday") && body.contains("Fix the save bug") && body.contains("Done, file it"));
        let card = card_of(db.list_open_review_items().unwrap()[0].payload.as_deref()).unwrap();
        assert!(!card.filed);

        // Filing moves the source beside its note.
        let filed = file(root, &card).unwrap();
        assert!(filed.filed && root.join(&p.source).exists() && !root.join(&raw).exists());
        assert!(file(root, &filed).is_ok(), "filing twice is a no-op");

        // Undo takes both parts back.
        assert!(undo(root, &filed).unwrap());
        assert_eq!(waiting(root), vec![raw.clone()]);
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
        let card = Card { placement: p.clone(), note_hash: hash(&written), filed: false };
        assert!(!undo(root, &card).unwrap(), "edited: kept");
        assert!(root.join(&p.note).exists() && root.join(&raw).exists());

        let again = place(root, &raw, "2026-09-24", SourceKind::Meeting);
        assert_eq!(again.note, format!("{INGESTED}/Meetings/2026-09/2026-09-24-standup-notes-2.md"));
        let doc = place(root, &raw, "2026-10-02", SourceKind::Document);
        assert_eq!(doc.note, format!("{INGESTED}/Documents/2026-10/2026-10-02-standup-notes.md"));
    }

    #[test]
    fn a_reply_without_frontmatter_is_refused_and_nothing_is_written() {
        let d = library();
        let root = d.path();
        let mut db = Db::open_in_memory().unwrap();
        let raw = format!("{RAW}/Standup notes.txt");
        assert!(ingest_one(root, &mut db, &raw, "2026-09-24", 1, |_| Ok("Sure! Here is the note.".into())).is_err());
        assert!(root.join(&raw).exists());
        assert!(db.open_review_item_of_kind(REVIEW_KIND).unwrap().is_none());
    }

    #[test]
    fn the_kind_comes_from_the_note_else_from_the_source() {
        assert_eq!(kind_of("---\nkind: recording\npresent: [Ana]\n---\n", "Raw/x.txt"), SourceKind::Recording);
        assert_eq!(kind_of("---\ntitle: x\n---\n", "Raw/call.vtt"), SourceKind::Recording);
        assert_eq!(kind_of("---\npresent: [Ana, Ben]\n---\n", "Raw/x.txt"), SourceKind::Meeting);
        assert_eq!(kind_of("---\npresent: [{{who was there}}]\n---\n", "Raw/spec.pdf"), SourceKind::Document);
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
        assert!(out.contains("D-008 · 2026-09-24 ·"));
        assert!(out.contains("sources: [[2026-09-24-standup]]") && out.contains("decider: Ana"));
        assert!(decisions_entry("# Decisions\n", "x", None, "2026-09-24", "n").contains("D-001 ·"));
    }

    #[test]
    fn a_plan_keeps_real_pages_and_allowed_new_ones() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join("Platform")).unwrap();
        fs::write(d.path().join("Platform/Save.md"), "x").unwrap();
        let pages = vec![("Current/Project.md".to_string(), "Project".to_string())];
        let reply = "```json\n{\"update\": [\"Current/Project.md\", \"Nope/Missing.md\"], \"create\": [{\"path\": \"Platform/Release-dates.md\", \"purpose\": \"when we ship\"}, {\"path\": \"Ways-of-Working/New-rule.md\"}, {\"path\": \"Platform/Save.md\"}, {\"path\": \"../escape.md\"}]}\n```";
        let plan = read_plan(reply, &pages, d.path());
        assert_eq!(plan.update, vec!["Current/Project.md"]);
        assert_eq!(plan.create.iter().map(|n| n.path.as_str()).collect::<Vec<_>>(), vec!["Platform/Release-dates.md"]);
        assert_eq!(read_plan("no json here", &pages, d.path()), Plan::default());
    }

    #[test]
    fn a_note_proposes_page_changes_new_pages_and_rulings_never_writes_them() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::create_dir_all(root.join("Current")).unwrap();
        fs::create_dir_all(root.join("_meta")).unwrap();
        let project = "---\ntitle: Project\n---\nWe ship on Monday.\n";
        fs::write(root.join("Current/Project.md"), project).unwrap();
        let log = "# Decisions\n\nD-001 · 2026-09-01 · saves — Regions.\n";
        fs::write(root.join("_meta/DECISIONS.md"), log).unwrap();
        let mut db = Db::open_in_memory().unwrap();
        crate::scan::scan(&crate::project::Project::create(root, "Wiki").unwrap(), &mut db).unwrap();
        let placement = place(root, "Research/Ingestion/Raw/standup.txt", "2026-09-24", SourceKind::Meeting);

        let done = follow_ups(root, &mut db, &placement, NOTE, "2026-09-24", 9, |p| {
            Ok(if p.contains("PAGES IN THE WIKI") {
                assert!(p.contains("- Current/Project.md — Project") && !p.contains("_meta/DECISIONS.md"), "{p}");
                "{\"update\": [\"Current/Project.md\"], \"create\": [{\"path\": \"Platform/Release-dates.md\", \"purpose\": \"when we ship\"}]}".into()
            } else if p.contains("`Current/Project.md`") {
                "---\ntitle: Project\n---\nWe ship on Friday ([[2026-09-24-standup]]).\n".into()
            } else if p.contains("`Platform/Release-dates.md`") {
                "---\ntitle: Release dates\nsources:\n  - \"[[2026-09-24-standup]]\"\n---\n# Release dates\nFriday.\n".into()
            } else {
                "NO CHANGE".into()
            })
        })
        .unwrap();
        assert_eq!(
            done,
            FollowUps { updated: vec!["Current/Project.md".into()], created: vec!["Platform/Release-dates.md".into()], rulings: 1 }
        );
        assert_eq!(fs::read_to_string(root.join("Current/Project.md")).unwrap(), project, "proposed, not written");
        assert!(!root.join("Platform/Release-dates.md").exists());
        assert_eq!(fs::read_to_string(root.join("_meta/DECISIONS.md")).unwrap(), log);

        let props: Vec<crate::wikidraft::Proposal> = db
            .list_open_review_items()
            .unwrap()
            .into_iter()
            .filter(|i| i.kind == crate::wikidraft::PROPOSAL_KIND)
            .map(|i| serde_json::from_str(i.payload.as_deref().unwrap()).unwrap())
            .collect();
        let new_page = props.iter().find(|p| p.page == "Platform/Release-dates.md").unwrap();
        assert_eq!(new_page.base, "");
        assert!(new_page.proposed.contains("status: draft"));
        crate::wikidraft::apply(root, new_page).unwrap();
        assert!(root.join("Platform/Release-dates.md").exists(), "applying a new page creates it");
    }
}
