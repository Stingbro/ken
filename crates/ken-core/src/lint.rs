//! Lint (knowledge-core decisions 29 and 30): the checks that read the
//! knowledge base as a whole, every result a finding (`findings`).
//!
//! - **On every scan** ([`on_scan`]), no model: broken path links, missing
//!   pages (a `[[name]]` no page answers), names two pages claim, and
//!   orphans (a page nothing links to). One open finding per page and kind;
//!   a finding the scan no longer finds is resolved.
//! - **After each edit**: an ingest or a drift run that writes pages queues
//!   them ([`queue_after_edit`]); one Claude call compares them and their
//!   link neighbours for contradictions ([`after_edit_next`],
//!   [`after_edit_done`]).
//! - **Weekly**: the binding and current pages read together for
//!   contradictions, with pages they call for that do not exist yet
//!   ([`weekly_next`], [`weekly_done`]).
//! - **Rulings cited in code** ([`unknown_rulings`]): a code comment citing a
//!   ruling id the decisions log does not have.
//!
//! Claude is asked by the app, in the background, behind the shared pace,
//! the hourly cap (`summary::take_call`) and the `backgroundExtraction`
//! switch; this module only says what to ask and files what comes back.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use serde_json::Value;

use crate::db::Db;
use crate::findings::Finding;
use crate::links::Resolver;
use crate::Result;

/// The kinds the scan's lint owns: what it no longer finds, it resolves.
pub const SCAN_KINDS: [&str; 4] = ["broken-link", "missing-page", "ambiguous-name", "orphan"];
/// The kind the code-citation check owns.
pub const UNKNOWN_RULING: &str = "unknown-ruling";
/// What a lint finding names as its source.
pub const SOURCE: &str = "lint";
/// Most findings of one kind a check files: a library with hundreds of
/// broken links gets the first ones, by path, until they are fixed.
pub const MAX_PER_KIND: usize = 50;
/// How often the weekly lint runs, in seconds.
pub const WEEK: i64 = 7 * 86_400;

/// Longest stretch of one page a comparison quotes, in characters.
const PAGE_CHARS: usize = 6_000;
/// Most characters of pages in one comparison.
const PROMPT_CHARS: usize = 80_000;
/// Most pages one after-edit comparison reads: the touched pages first, then
/// their neighbours.
const AFTER_EDIT_PAGES: usize = 12;

const QUEUE: &str = "lint_queue";
const WEEKLY_AT: &str = "lint_weekly_at";

/// Pages that are not orphans when nothing links to them: indexes, the
/// entry point, the log and the brief, the decisions log, notes, tickets,
/// templates and Ken's own files.
fn may_be_orphan(path: &str) -> bool {
    if !crate::chunker::is_markdown(path) {
        return false;
    }
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    if ["index.md", "readme.md", "start-here.md", "log.md", "claude.md", "agents.md", "decisions.md"].contains(&name) {
        return false;
    }
    let folders = ["tickets/", "inbox/", "ingested/", "ingestion/", "notes/", "templates/", ".ken/", "_meta/"];
    !folders.iter().any(|f| lower.starts_with(f) || lower.contains(&format!("/{f}")))
}

/// The pages nothing links to, by path ([`may_be_orphan`] leaves the rest
/// out). A page's links to itself do not count.
pub fn orphans(db: &Db) -> Result<Vec<String>> {
    let resolver = Resolver::from_db(db)?;
    let mut reached: HashSet<String> = HashSet::new();
    for (from, link) in db.all_page_links()? {
        for to in resolver.resolve(&link) {
            if to != from {
                reached.insert(to);
            }
        }
    }
    let mut out: Vec<String> = db.page_paths()?.into_iter().filter(|p| may_be_orphan(p) && !reached.contains(p)).collect();
    out.sort();
    Ok(out)
}

/// What the scan's lint finds now, at most [`MAX_PER_KIND`] of each kind.
pub fn scan_findings(db: &Db) -> Result<Vec<Finding>> {
    let report = crate::links::report(db)?;
    let base = |kind: &str| Finding { kind: kind.into(), source: SOURCE.into(), ..Default::default() };
    let mut out = Vec::new();
    for (name, askers) in report.missing_names.iter().take(MAX_PER_KIND) {
        let s = if askers.len() == 1 { "" } else { "s" };
        // It opens the first page that asks for it.
        out.push(Finding {
            subject: name.clone(),
            against: askers.first().cloned().unwrap_or_default(),
            against_kind: "page".into(),
            text: format!("[[{name}]] has no page: asked for by {} page{s}, {}.", askers.len(), askers.join(", ")),
            ..base("missing-page")
        });
    }
    let broken: Vec<&(String, String)> = report.broken_paths.values().flatten().collect();
    for (from, to) in broken.into_iter().take(MAX_PER_KIND) {
        out.push(Finding {
            subject: from.clone(),
            against: to.clone(),
            against_kind: "page".into(),
            text: format!("{from} links to {to}, which is not there."),
            ..base("broken-link")
        });
    }
    for (name, owners) in report.duplicate_names.iter().take(MAX_PER_KIND) {
        out.push(Finding {
            subject: name.clone(),
            text: format!("Two pages claim the name {name}, so a link to [[{name}]] is ambiguous: {}.", owners.join(", ")),
            ..base("ambiguous-name")
        });
    }
    for page in orphans(db)?.into_iter().take(MAX_PER_KIND) {
        out.push(Finding {
            text: format!("Nothing links to {page}: add it to its section's index, or link it from a page that covers it."),
            subject: page,
            ..base("orphan")
        });
    }
    Ok(out)
}

/// The lint on every scan: files what [`scan_findings`] finds and resolves
/// what it no longer finds. Returns (filed, resolved).
pub fn on_scan(db: &mut Db, now: i64) -> Result<(usize, usize)> {
    let found = scan_findings(db)?;
    crate::findings::sync(db, &SCAN_KINDS, &found, now)
}

fn read_list(db: &Db, name: &str) -> Result<Vec<String>> {
    Ok(db.check_state(name)?.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default())
}

fn store_list(db: &Db, name: &str, list: &[String]) -> Result<()> {
    db.store_check_state(name, &serde_json::to_string(list).unwrap_or_else(|_| "[]".into()))
}

/// Queue pages an ingest or a drift run just wrote, for the comparison
/// after an edit. Only pages; each once.
pub fn queue_after_edit(db: &Db, pages: &[String]) -> Result<()> {
    let mut queue = read_list(db, QUEUE)?;
    let before = queue.len();
    for p in pages.iter().filter(|p| crate::chunker::is_markdown(p)) {
        if !queue.contains(p) {
            queue.push(p.clone());
        }
    }
    if queue.len() != before {
        store_list(db, QUEUE, &queue)?;
    }
    Ok(())
}

/// The pages waiting for the comparison after an edit.
pub fn queued(db: &Db) -> Result<Vec<String>> {
    read_list(db, QUEUE)
}

/// One Claude call to make: the pages it is about and its prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    /// For the comparison after an edit, the queued pages it takes off the
    /// queue when it is done.
    pub pages: Vec<String>,
    pub prompt: String,
}

/// Pages as the prompt quotes them, each cut to [`PAGE_CHARS`], until
/// [`PROMPT_CHARS`]. Pages that cannot be read are left out.
fn quoted(root: &Path, pages: &[String]) -> (String, usize) {
    let mut out = String::new();
    let mut n = 0;
    for p in pages {
        let Ok(text) = std::fs::read_to_string(root.join(p)) else { continue };
        let cut: String = text.chars().take(PAGE_CHARS).collect();
        let more = if cut.len() < text.len() { "\n[…]" } else { "" };
        if !out.is_empty() && out.len() + cut.len() > PROMPT_CHARS {
            break;
        }
        out.push_str(&format!("\n--- {p}\n{cut}{more}\n"));
        n += 1;
    }
    (out, n)
}

const CONTRADICTION_RULES: &str = "Name every place where two of these pages contradict each other: they say different things \
about the same fact, rule or decision, so a reader of one would be wrong about the other. Do not name differences in wording, \
detail or emphasis, or a page that only says less. Use only the pages below; do not use any tools.";

const CONTRADICTION_SHAPE: &str = "{\"page\": \"<path of one page>\", \"against\": \"<path of the page it contradicts>\", \
\"text\": \"<how they differ, one sentence>\", \"quote\": \"<the words on the first page, at most 25 words>\"}";

/// The comparison after an edit: the queued pages and their link
/// neighbours, one call. None when nothing is queued; a queue of pages that
/// are all gone is cleared.
pub fn after_edit_next(db: &Db, root: &Path) -> Result<Option<Pending>> {
    let queue = queued(db)?;
    if queue.is_empty() {
        return Ok(None);
    }
    let mut pages: Vec<String> = queue.iter().filter(|p| root.join(p).is_file()).cloned().collect();
    if pages.is_empty() {
        store_list(db, QUEUE, &[])?;
        return Ok(None);
    }
    let edited = pages.len();
    for p in queue.iter() {
        let n = crate::links::page_links(db, p)?;
        for q in n.outgoing.into_iter().chain(n.incoming) {
            if pages.len() >= AFTER_EDIT_PAGES {
                break;
            }
            if may_be_compared(&q) && !pages.contains(&q) {
                pages.push(q);
            }
        }
    }
    let (text, n) = quoted(root, &pages);
    if n < 2 {
        // One page alone contradicts nothing.
        store_list(db, QUEUE, &[])?;
        return Ok(None);
    }
    let prompt = format!(
        "These are pages of a team's knowledge base. The first {} were just edited; the rest link to them or are linked \
         from them. {CONTRADICTION_RULES} Only name contradictions that involve an edited page.\n\n\
         Answer with JSON only: [{CONTRADICTION_SHAPE}]\nAnswer [] when there is none.\n\nPages:\n{text}",
        edited.min(n)
    );
    Ok(Some(Pending { pages: queue, prompt }))
}

/// Pages worth comparing: not an index, the log, a template or a note.
fn may_be_compared(path: &str) -> bool {
    may_be_orphan(path)
}

/// Contradictions in a reply, as findings: each names two pages, the first
/// is the finding's subject.
fn contradictions(items: &[Value]) -> Vec<Finding> {
    items
        .iter()
        .filter_map(|item| {
            let s = |k: &str| item.get(k).and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
            let (page, against, text) = (s("page"), s("against"), s("text"));
            (!page.is_empty() && !text.is_empty() && page != against).then(|| Finding {
                kind: "contradiction".into(),
                source: SOURCE.into(),
                subject: page,
                against_kind: if against.is_empty() { String::new() } else { "page".into() },
                against,
                text,
                quote: s("quote"),
                ..Default::default()
            })
        })
        .collect()
}

/// A reply's list: the array itself, or `key`'s array in an object.
fn list_in(reply: &str, key: &str) -> Vec<Value> {
    match crate::local_llm::parse_json_lenient(reply) {
        Ok(Value::Array(a)) => a,
        Ok(Value::Object(o)) => o.get(key).and_then(|v| v.as_array()).cloned().unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// File what the comparison after an edit found, and take its pages off the
/// queue (pages queued since stay). Returns how many findings it filed.
pub fn after_edit_done(db: &mut Db, pending: &Pending, reply: &str, now: i64) -> Result<usize> {
    let found = contradictions(&list_in(reply, "contradictions"));
    for f in &found {
        crate::findings::file(db, f, now)?;
    }
    let rest: Vec<String> = queued(db)?.into_iter().filter(|p| !pending.pages.contains(p)).collect();
    store_list(db, QUEUE, &rest)?;
    Ok(found.len())
}

/// When the weekly lint last ran, if it has.
pub fn weekly_at(db: &Db) -> Result<Option<i64>> {
    Ok(db.check_state(WEEKLY_AT)?.and_then(|v| v.trim().parse().ok()))
}

/// Whether the weekly lint is due: never run, or a week since.
pub fn weekly_due(db: &Db, now: i64) -> Result<bool> {
    Ok(weekly_at(db)?.is_none_or(|at| now - at >= WEEK))
}

/// The binding and current pages (decision 23's top two tiers), the
/// decisions log left out: its rulings are the supersession pass's.
pub fn binding_and_current(db: &Db, root: &Path) -> Result<Vec<String>> {
    let roles = crate::authority::Roles::of_repo(root);
    let mut out = Vec::new();
    for p in db.page_paths()?.into_iter().filter(|p| crate::chunker::is_markdown(p) && !roles.is_decisions_log(p)) {
        let tier = db.authority_of(&p)?.and_then(|(t, _)| crate::authority::Tier::parse(&t));
        if matches!(tier, Some(crate::authority::Tier::Binding | crate::authority::Tier::Current)) {
            out.push(p);
        }
    }
    Ok(out)
}

/// The weekly lint's one call: the binding and current pages read together,
/// for contradictions and for pages they call for that are not there. None
/// when there are fewer than two such pages; it is marked done at `now`
/// then.
pub fn weekly_next(db: &Db, root: &Path, now: i64) -> Result<Option<Pending>> {
    let pages = binding_and_current(db, root)?;
    let (text, n) = quoted(root, &pages);
    if n < 2 {
        db.store_check_state(WEEKLY_AT, &now.to_string())?;
        return Ok(None);
    }
    let left = if n < pages.len() { format!(" ({} more were left out for length)", pages.len() - n) } else { String::new() };
    let prompt = format!(
        "These are the binding and current pages of a team's knowledge base: its rules, and what it says is true now{left}. \
         {CONTRADICTION_RULES}\n\n\
         Also name up to five pages the knowledge base lacks: a subject these pages keep relying on that no page here \
         covers. Not a page that would only be nice to have.\n\n\
         Answer with JSON only: {{\"contradictions\": [{CONTRADICTION_SHAPE}], \"missing\": [{{\"name\": \"<the page's \
         title>\", \"why\": \"<which pages need it and for what, one sentence>\"}}]}}\n\
         Answer with empty lists when there is nothing.\n\nPages:\n{text}"
    );
    Ok(Some(Pending { pages, prompt }))
}

/// File what the weekly lint found and mark it done. Returns how many
/// findings it filed.
pub fn weekly_done(db: &mut Db, reply: &str, now: i64) -> Result<usize> {
    let mut found = contradictions(&list_in(reply, "contradictions"));
    for item in list_in(reply, "missing").iter().take(5) {
        let s = |k: &str| item.get(k).and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        let name = s("name");
        if name.is_empty() {
            continue;
        }
        let why = s("why");
        found.push(Finding {
            kind: "suggested-page".into(),
            source: SOURCE.into(),
            text: if why.is_empty() { format!("No page covers {name}.") } else { format!("No page covers {name}: {why}") },
            subject: name,
            ..Default::default()
        });
    }
    for f in &found {
        crate::findings::file(db, f, now)?;
    }
    db.store_check_state(WEEKLY_AT, &now.to_string())?;
    Ok(found.len())
}

/// Whether a code line has a comment before byte `at`: `//`, `#`, `/*`,
/// `--`, `<!--`, `;;`, or a line that starts with `*` (inside a block
/// comment).
fn in_comment(line: &str, at: usize) -> bool {
    if line.trim_start().starts_with('*') {
        return true;
    }
    ["//", "#", "/*", "--", "<!--", ";;"].iter().any(|m| line.find(m).is_some_and(|i| i < at))
}

/// The ruling ids each comment in a code file cites, as (line, id), for
/// ids in the shape of the log's (one of `prefixes`, a dash, digits).
pub fn code_citations(text: &str, prefixes: &BTreeSet<String>) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        for id in crate::git::cited_ids(line) {
            let Some((prefix, _)) = id.rsplit_once('-') else { continue };
            if !prefixes.contains(prefix) {
                continue;
            }
            if line.find(&id).is_some_and(|at| in_comment(line, at)) {
                out.push((i + 1, id));
            }
        }
    }
    out
}

/// One code comment citing a ruling id ([`rulings_in_code`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeRuling {
    /// The repo, by its name in the workspace.
    pub repo: String,
    pub path: String,
    pub line: usize,
    pub id: String,
    /// Whether the decisions log has the id.
    pub known: bool,
}

/// Every code comment citing a ruling (decision 30), in each of `code` (the
/// workspace's other repos), checked against the logs of the knowledge base
/// `db` at `root`. The ids' shape is the log's own (its ids' prefixes), so a
/// ticket id or `UTF-8` is never read as a ruling. Nothing when the
/// knowledge base has no log yet. The known ones are graph edges
/// (`graph::store_code_citations`); the rest are findings
/// ([`unknown_rulings`]).
pub fn rulings_in_code(db: &Db, root: &Path, code: &[crate::ingest::CodeSource]) -> Result<Vec<CodeRuling>> {
    let roles = crate::authority::Roles::of_repo(root);
    let mut ids: HashSet<String> = HashSet::new();
    for log in crate::supersede::log_paths(db, &roles)? {
        if let Some(text) = db.get_text(&log)? {
            ids.extend(crate::drift::parse_rulings(&text).into_iter().map(|r| r.id));
        }
    }
    let prefixes: BTreeSet<String> = ids.iter().filter_map(|id| id.rsplit_once('-').map(|(p, _)| p.to_string())).collect();
    let mut out: Vec<CodeRuling> = Vec::new();
    for source in code {
        for prefix in &prefixes {
            for (path, text) in source.db.code_texts_containing(&format!("{prefix}-"))? {
                for (line, id) in code_citations(&text, &prefixes) {
                    if out.iter().any(|c| c.repo == source.name && c.path == path && c.line == line && c.id == id) {
                        continue;
                    }
                    out.push(CodeRuling { repo: source.name.clone(), path: path.clone(), line, known: ids.contains(&id), id });
                }
            }
        }
    }
    Ok(out)
}

/// Code comments citing a ruling the decisions log does not have (decision
/// 30), at most [`MAX_PER_KIND`], from [`rulings_in_code`].
pub fn unknown_rulings(db: &Db, root: &Path, code: &[crate::ingest::CodeSource]) -> Result<Vec<Finding>> {
    Ok(unknown_of(&rulings_in_code(db, root, code)?))
}

/// The findings for the citations the log does not have.
pub fn unknown_of(citations: &[CodeRuling]) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    for c in citations.iter().filter(|c| !c.known) {
        let at = format!("{}:{}:{}", c.repo, c.path, c.line);
        if out.len() >= MAX_PER_KIND || out.iter().any(|f| f.subject == at && f.against == c.id) {
            continue;
        }
        out.push(Finding {
            kind: UNKNOWN_RULING.into(),
            source: SOURCE.into(),
            text: format!("`{at}` cites {}, which the decisions log does not have.", c.id),
            subject: at,
            against: c.id.clone(),
            against_kind: "ruling".into(),
            ..Default::default()
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Project;
    use std::fs;

    fn library(files: &[(&str, &str)]) -> (tempfile::TempDir, Project, Db) {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let p = dir.path().join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        let project = Project::create(dir.path(), "Lib").unwrap();
        let mut db = Db::open_in_memory().unwrap();
        crate::scan::scan(&project, &mut db).unwrap();
        (dir, project, db)
    }

    #[test]
    fn orphans_are_pages_nothing_links_to_and_never_an_index_a_note_or_a_ticket() {
        let (_d, _p, db) = library(&[
            ("START-HERE.md", "# Start\nSee [[Payments]].\n"),
            ("Design/Index.md", "# Design\n"),
            ("Design/Payments.md", "# Payments\nLinks to [[Payments]] itself.\n"),
            ("Design/Lonely.md", "# Lonely\nSee [[Payments]].\n"),
            ("Research/Ingestion/Ingested/Meetings/2026-10/n.md", "# A note\n"),
            ("tickets/SR-001.md", "# A ticket\n"),
            ("log.md", "# Log\n"),
        ]);
        assert_eq!(orphans(&db).unwrap(), vec!["Design/Lonely.md".to_string()]);
    }

    #[test]
    fn lint_findings_are_filed_once_and_resolved_when_fixed() {
        let (dir, project, mut db) = library(&[
            ("START-HERE.md", "# Start\nSee [[Payments]], [[Ghost]] and [the map](Design/Map.md).\n"),
            ("Design/Payments.md", "# Payments\n"),
            ("Design/Lonely.md", "# Lonely\n"),
        ]);
        on_scan(&mut db, 1).unwrap();
        let open = crate::findings::open(&db).unwrap();
        let kinds: BTreeSet<(String, String)> = open.iter().map(|(_, f)| (f.kind.clone(), f.subject.clone())).collect();
        assert!(kinds.contains(&("missing-page".into(), "Ghost".into())), "{kinds:?}");
        assert!(kinds.contains(&("broken-link".into(), "START-HERE.md".into())), "{kinds:?}");
        assert!(kinds.contains(&("orphan".into(), "Design/Lonely.md".into())), "{kinds:?}");
        assert_eq!(on_scan(&mut db, 2).unwrap(), (0, 0), "found again: nothing new, nothing resolved");

        fs::write(dir.path().join("START-HERE.md"), "# Start\nSee [[Payments]] and [[Lonely]].\n").unwrap();
        crate::scan::scan(&project, &mut db).unwrap();
        on_scan(&mut db, 3).unwrap();
        assert!(crate::findings::open(&db).unwrap().is_empty(), "every one fixed, every one resolved");
    }

    #[test]
    fn edited_pages_queue_for_one_comparison_with_their_neighbours() {
        let (dir, _p, mut db) = library(&[
            ("START-HERE.md", "# Start\nSee [[Payments]].\n"),
            ("Design/Payments.md", "# Payments\nRefunds take a week. See [[Refunds]].\n"),
            ("Design/Refunds.md", "# Refunds\nRefunds take a day.\n"),
        ]);
        assert!(after_edit_next(&db, dir.path()).unwrap().is_none(), "nothing queued, nothing asked");
        queue_after_edit(&db, &["Design/Payments.md".to_string(), "src/x.rs".to_string()]).unwrap();
        assert_eq!(queued(&db).unwrap(), vec!["Design/Payments.md".to_string()], "only pages");
        let pending = after_edit_next(&db, dir.path()).unwrap().unwrap();
        assert!(pending.prompt.contains("--- Design/Payments.md") && pending.prompt.contains("--- Design/Refunds.md"), "{}", pending.prompt);
        queue_after_edit(&db, &["Design/Refunds.md".to_string()]).unwrap();
        let reply = r#"[{"page": "Design/Payments.md", "against": "Design/Refunds.md", "text": "A week against a day", "quote": "Refunds take a week."}]"#;
        assert_eq!(after_edit_done(&mut db, &pending, reply, 5).unwrap(), 1);
        let open = crate::findings::open(&db).unwrap();
        assert_eq!(open[0].1.kind, "contradiction");
        assert_eq!((open[0].1.subject.as_str(), open[0].1.against.as_str()), ("Design/Payments.md", "Design/Refunds.md"));
        assert_eq!(queued(&db).unwrap(), vec!["Design/Refunds.md".to_string()], "queued since: still waits");
    }

    #[test]
    fn the_weekly_lint_reads_binding_and_current_pages_and_files_suggestions() {
        let (dir, _p, mut db) = library(&[
            ("Rules/Reviews.md", "---\nstatus: current\n---\n# Reviews\nTwo approvals.\n"),
            ("Current/Project.md", "---\nstatus: current\n---\n# Project\nOne approval is enough.\n"),
            ("Reference/Glossary.md", "# Glossary\n"),
        ]);
        assert!(weekly_due(&db, 100).unwrap());
        let pending = weekly_next(&db, dir.path(), 100).unwrap().expect("two pages to compare");
        assert!(pending.prompt.contains("--- Rules/Reviews.md") && pending.prompt.contains("--- Current/Project.md"));
        assert!(!pending.prompt.contains("Glossary"), "reference pages wait for their own checks");
        let reply = r#"{"contradictions": [{"page": "Current/Project.md", "against": "Rules/Reviews.md", "text": "One approval against two"}],
                        "missing": [{"name": "Release process", "why": "Reviews and Project both rely on it."}]}"#;
        assert_eq!(weekly_done(&mut db, reply, 100).unwrap(), 2);
        let kinds: Vec<String> = crate::findings::open(&db).unwrap().into_iter().map(|(_, f)| f.kind).collect();
        assert!(kinds.contains(&"suggested-page".to_string()) && kinds.contains(&"contradiction".to_string()), "{kinds:?}");
        assert!(!weekly_due(&db, 100 + WEEK - 1).unwrap());
        assert!(weekly_due(&db, 100 + WEEK).unwrap());
    }

    #[test]
    fn a_comment_citing_a_ruling_the_log_lacks_is_a_finding() {
        let prefixes: BTreeSet<String> = ["D".to_string()].into();
        let text = "// Per D-012 regions save apart.\nlet s = \"D-999\"; // see D-404\n# D-7 in a script\nlet x = UTF-8;\n/* SR-12 */\n";
        assert_eq!(
            code_citations(text, &prefixes),
            vec![(1, "D-012".to_string()), (2, "D-404".to_string()), (3, "D-7".to_string())],
            "a string is no comment; another series is not the log's"
        );

        let (dir, _p, db) = library(&[(
            "Decisions/DECISIONS.md",
            "# Decisions\n\n## D-012 · 2026-10-01 · saves\n\n**Regions save apart.**\n\nD-013 - 2026-10-02 - saves - Older form.\n",
        )]);
        let mut code = Db::open_in_memory().unwrap();
        code.upsert_file("src/save.rs", "code", 1, 1, "indexed", None, "// D-012 and D-013 hold\n// but D-404 is made up\nfn save() {}\n")
            .unwrap();
        let found = unknown_rulings(&db, dir.path(), &[crate::ingest::CodeSource { name: "game".into(), db: code }]).unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].subject.as_str(), found[0].against.as_str()), ("game:src/save.rs:2", "D-404"));
    }
}
