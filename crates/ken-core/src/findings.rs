//! The one findings list (knowledge-core decisions 10, 14, 25, 28 to 30):
//! what a check found that a person should look at, kept as Review items of
//! kind [`KIND`] in the knowledge base's index. Ingest files each
//! contradiction a source shows and each question it leaves open; drift
//! files a ruling whose code changed and a source that is gone; lint files
//! broken links, missing pages, ambiguous names, orphans, contradictions
//! between pages and code that cites a ruling the log lacks; the
//! supersession pass files each pair it finds. A finding changes nothing by
//! itself: a person reads it, fixes what needs fixing and marks it done (a
//! supersession is confirmed instead, and Ken writes it into the log).
//!
//! Repeats fold: a finding with the same kind, page and target as an open
//! one is not filed again ([`file`]). A check that owns its kinds (lint on a
//! scan, the code citations) files what it finds now and resolves what it
//! no longer finds ([`sync`]).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::db::{Db, ReviewItemRow};
use crate::{Error, Result};

/// The Review kind of a finding.
pub const KIND: &str = "finding";

/// What a finding is about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// `contradiction` | `question` (ingest, and lint after an edit or
    /// weekly) · `drift` (a ruling whose code changed, a cited source that is
    /// gone, a void run) · `broken-link` | `missing-page` | `ambiguous-name` |
    /// `orphan` (lint on every scan) · `suggested-page` (the weekly lint) ·
    /// `unknown-ruling` (code cites an id the log does not have) ·
    /// `supersession`
    pub kind: String,
    /// Where it was found: the ingested note it came from, or the check that
    /// found it (`drift`, `lint`, `supersession`).
    pub source: String,
    /// What it is about, when that is not the source: a page, the page a
    /// link is on, a missing page's name, the later ruling of a
    /// supersession, a code line (`repo:path:line`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subject: String,
    /// What it contradicts or points at: a page or ticket path, a ruling id
    /// (`D-012`), a code file, a link's target. Empty for a question.
    #[serde(default)]
    pub against: String,
    /// `page` | `ruling` | `ticket` | `code` | `source` (the source disagrees
    /// with itself), empty when it points at nothing.
    #[serde(default)]
    pub against_kind: String,
    /// How the two differ, the question, or what is wrong.
    pub text: String,
    /// The source's own words, when the note quotes them.
    #[serde(default)]
    pub quote: String,
    /// For a supersession: `supersedes` or `refines`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub relation: String,
}

impl Finding {
    /// A contradiction with a ruling or with the code: never edited away,
    /// always for a person (decision 10).
    pub fn binds(&self) -> bool {
        self.kind == "contradiction" && matches!(self.against_kind.as_str(), "ruling" | "code")
    }

    /// The page to open for it: its subject or target when that is a page,
    /// else the note it came from. Empty when it is about no page.
    pub fn page(&self) -> String {
        let md = |s: &str| crate::chunker::is_markdown(s).then(|| s.to_string());
        let target = matches!(self.against_kind.as_str(), "page" | "ticket").then(|| md(&self.against)).flatten();
        md(&self.subject).or(target).or_else(|| md(&self.source)).unwrap_or_default()
    }

    /// What makes two findings one: the kind, the page (or the source), and
    /// the target; the text when there is no target (two questions from one
    /// note are two findings).
    pub fn key(&self) -> (String, String, String) {
        let page = if self.subject.is_empty() { &self.source } else { &self.subject };
        let target = if self.against.is_empty() { self.text.trim() } else { self.against.trim() };
        (self.kind.clone(), page.clone(), target.to_string())
    }

    fn title(&self) -> String {
        let short: String = self.text.chars().take(80).collect();
        match self.kind.as_str() {
            "question" => format!("Open question: {short}"),
            "contradiction" if self.against.is_empty() => format!("Contradiction: {short}"),
            "contradiction" => format!("Contradicts {}: {short}", self.against),
            _ => short,
        }
    }

    fn body(&self) -> String {
        let mut s = self.text.clone();
        if !self.quote.is_empty() {
            s.push_str(&format!("\n\n> {}", self.quote));
        }
        if crate::chunker::is_markdown(&self.source) {
            let stem = self.source.rsplit('/').next().unwrap_or(&self.source).trim_end_matches(".md");
            s.push_str(&format!("\n\nFrom [[{stem}]]."));
        }
        if self.binds() {
            s.push_str(" A contradiction with a ruling or the code waits for a person: Ken changed nothing for it.");
        }
        s
    }
}

/// File one finding, unless an open one has the same kind, page and target
/// ([`Finding::key`]): repeats fold into it. Returns the Review item's id,
/// the open one's when it folded.
pub fn file(db: &mut Db, f: &Finding, now: i64) -> Result<i64> {
    if let Some((it, _)) = open(db)?.into_iter().find(|(_, o)| o.key() == f.key()) {
        return Ok(it.id);
    }
    let payload = serde_json::to_string(f).map_err(|e| Error::Other(e.to_string()))?;
    let source_ref = if f.subject.is_empty() { &f.source } else { &f.subject };
    db.insert_review_item(KIND, &f.title(), &f.body(), source_ref, Some(&payload), now)
}

/// What a check that owns `kinds` finds now: each one not open yet is
/// filed, and each open one of those kinds that is no longer found is
/// resolved. A finding a person marked done and the check finds again is
/// filed again. Returns (filed, resolved).
pub fn sync(db: &mut Db, kinds: &[&str], found: &[Finding], now: i64) -> Result<(usize, usize)> {
    let open: Vec<(ReviewItemRow, Finding)> = open(db)?.into_iter().filter(|(_, f)| kinds.contains(&f.kind.as_str())).collect();
    let now_found: HashSet<(String, String, String)> = found.iter().map(Finding::key).collect();
    let mut resolved = 0;
    for (it, f) in &open {
        if !now_found.contains(&f.key()) {
            db.resolve_review_item(it.id, now)?;
            resolved += 1;
        }
    }
    let was_open: HashSet<(String, String, String)> = open.iter().map(|(_, f)| f.key()).collect();
    let mut filed = 0;
    let mut seen = HashSet::new();
    for f in found {
        let key = f.key();
        if was_open.contains(&key) || !seen.insert(key) {
            continue;
        }
        file(db, f, now)?;
        filed += 1;
    }
    Ok((filed, resolved))
}

/// Every open finding, newest first, with what it is about.
pub fn open(db: &Db) -> Result<Vec<(ReviewItemRow, Finding)>> {
    Ok(db
        .list_open_review_items()?
        .into_iter()
        .filter(|it| it.kind == KIND)
        .filter_map(|it| {
            let f = it.payload.as_deref().and_then(|p| serde_json::from_str::<Finding>(p).ok())?;
            Some((it, f))
        })
        .collect())
}

/// Every finding ever filed of `kind`, open or resolved: what a check has
/// already said, so it never says it twice (a supersession a person
/// dismissed stays dismissed).
pub fn ever(db: &Db, kind: &str) -> Result<Vec<Finding>> {
    Ok(db
        .review_items_of_kind(KIND, 10_000)?
        .into_iter()
        .filter_map(|it| it.payload.as_deref().and_then(|p| serde_json::from_str::<Finding>(p).ok()))
        .filter(|f| f.kind == kind)
        .collect())
}

/// How many open findings came from `source`.
pub fn open_from(db: &Db, source: &str) -> Result<usize> {
    Ok(open(db)?.into_iter().filter(|(_, f)| f.source == source).count())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contradiction() -> Finding {
        Finding {
            kind: "contradiction".into(),
            source: "Research/Ingestion/Ingested/Meetings/2026-10/2026-10-13-standup.md".into(),
            against: "D-012".into(),
            against_kind: "ruling".into(),
            text: "The note says saves are global; the ruling says per region".into(),
            quote: "we save it all in one file".into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_finding_is_filed_and_read_back() {
        let mut db = Db::open_in_memory().unwrap();
        let f = contradiction();
        let id = file(&mut db, &f, 5).unwrap();
        let open = open(&db).unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].0.id, id);
        assert_eq!(open[0].1, f);
        assert!(open[0].0.title.starts_with("Contradicts D-012: "));
        assert!(open[0].0.body.contains("> we save it all in one file") && open[0].0.body.contains("[[2026-10-13-standup]]"));
        assert!(open[0].0.body.contains("waits for a person"), "a ruling binds");
        assert_eq!(open_from(&db, &f.source).unwrap(), 1);
        let q = Finding { kind: "question".into(), source: f.source.clone(), text: "Who owns saves?".into(), ..Default::default() };
        assert!(!q.binds() && q.title() == "Open question: Who owns saves?");
        assert_eq!(f.page(), f.source, "a ruling is no page: the note opens");
    }

    #[test]
    fn repeats_fold_and_two_questions_stay_two() {
        let mut db = Db::open_in_memory().unwrap();
        let f = contradiction();
        let first = file(&mut db, &f, 5).unwrap();
        let again = Finding { text: "said another way".into(), ..f.clone() };
        assert_eq!(file(&mut db, &again, 6).unwrap(), first, "same kind, page and target: one finding");
        let q = |t: &str| Finding { kind: "question".into(), source: f.source.clone(), text: t.into(), ..Default::default() };
        file(&mut db, &q("Who owns saves?"), 7).unwrap();
        file(&mut db, &q("When do we ship?"), 7).unwrap();
        assert_eq!(open(&db).unwrap().len(), 3);
    }

    #[test]
    fn a_check_files_what_it_finds_and_resolves_what_it_no_longer_finds() {
        let mut db = Db::open_in_memory().unwrap();
        let orphan = |p: &str| Finding {
            kind: "orphan".into(),
            source: "lint".into(),
            subject: p.into(),
            text: format!("Nothing links to {p}."),
            ..Default::default()
        };
        file(&mut db, &contradiction(), 1).unwrap();
        assert_eq!(sync(&mut db, &["orphan"], &[orphan("Design/A.md"), orphan("Design/B.md")], 2).unwrap(), (2, 0));
        assert_eq!(sync(&mut db, &["orphan"], &[orphan("Design/A.md"), orphan("Design/A.md")], 3).unwrap(), (0, 1));
        let kinds: Vec<String> = open(&db).unwrap().into_iter().map(|(_, f)| format!("{} {}", f.kind, f.subject)).collect();
        assert_eq!(kinds.len(), 2, "{kinds:?}");
        assert!(kinds.contains(&"orphan Design/A.md".to_string()), "{kinds:?}");
        assert!(kinds.iter().any(|k| k.starts_with("contradiction")), "another kind is left alone: {kinds:?}");
        assert_eq!(orphan("Design/A.md").page(), "Design/A.md");
        assert_eq!(ever(&db, "orphan").unwrap().len(), 2, "the resolved one is still on record");
    }
}
