//! The Timeline as the project's raid log (knowledge-core decision 37): one
//! chronological, timestamped list of what happened in the project, each
//! entry linked to its source. Read from what is already there, no model:
//!
//! - **ingest, drift and lint** runs: the knowledge base's `log.md` lines
//!   (`## [YYYY-MM-DD] kind | title`, decision 2);
//! - **rulings**: the decisions log's dated entries;
//! - **ticket changes**: each ticket's `updated` date and status;
//! - **commits**: the knowledge base's git log, and the commits of the other
//!   repos that cite a ticket or a ruling (the graph's `commit` edges);
//! - **findings**: when each was filed, and when it was resolved;
//! - **releases**: each repo's tags.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::db::Db;
use crate::Result;

/// Most commits read from the knowledge base's log.
const KB_COMMITS: usize = 300;
/// Most tags read from one repo.
const TAGS: usize = 50;
/// Most findings read, filed or resolved.
const FINDINGS: usize = 2_000;
/// Longest entry text, in characters.
const TEXT_CHARS: usize = 240;

/// One entry of the raid log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// The day, `YYYY-MM-DD`.
    pub date: String,
    /// The moment in Unix seconds, when the source says it (a finding);
    /// None when only the day is known.
    pub at: Option<i64>,
    /// `ingest`, `drift`, `lint` (or another kind a `log.md` line names),
    /// `ruling`, `ticket`, `commit`, `finding` or `release`.
    pub kind: String,
    pub text: String,
    /// The repo it is in, by name, for a commit or a release of another
    /// repo; None for the knowledge base.
    pub repo: Option<String>,
    /// The file it comes from in the knowledge base, and the line.
    pub path: Option<String>,
    pub line: Option<i64>,
    /// A commit's sha or a release's tag.
    pub reference: Option<String>,
}

impl Entry {
    fn new(date: &str, kind: &str, text: &str) -> Entry {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let text: String = if text.chars().count() > TEXT_CHARS {
            text.chars().take(TEXT_CHARS - 1).chain(std::iter::once('…')).collect()
        } else {
            text
        };
        Entry { date: date.to_string(), at: None, kind: kind.to_string(), text, repo: None, path: None, line: None, reference: None }
    }

    fn at(mut self, path: &str, line: Option<i64>) -> Entry {
        self.path = Some(path.to_string());
        self.line = line;
        self
    }
}

fn is_date(s: &str) -> bool {
    s.len() == 10 && s.as_bytes().iter().enumerate().all(|(i, b)| if i == 4 || i == 7 { *b == b'-' } else { b.is_ascii_digit() })
}

/// The `log.md` lines: `## [YYYY-MM-DD] kind | title`, each with its line.
pub fn log_lines(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let Some(rest) = line.trim().strip_prefix("## [") else { continue };
        let Some((date, rest)) = rest.split_once(']') else { continue };
        if !is_date(date.trim()) {
            continue;
        }
        let (kind, title) = rest.split_once('|').unwrap_or((rest, ""));
        let kind = kind.trim().to_lowercase();
        if kind.is_empty() {
            continue;
        }
        out.push(Entry::new(date.trim(), &kind, title.trim()).at(crate::ingest::LOG_PAGE, Some(i as i64 + 1)));
    }
    out
}

/// A ruling entry's line: its id and its bold ruling, else its first line.
fn ruling_text(id: &str, text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let ruling = lines
        .iter()
        .find(|l| l.starts_with("**") && !l.starts_with(&format!("**{id}")))
        .or_else(|| lines.first())
        .map(|l| l.trim_matches('*').trim().to_string())
        .unwrap_or_default();
    let ruling = ruling.trim_start_matches('#').trim();
    if ruling.contains(id) {
        ruling.to_string()
    } else {
        format!("{id}: {ruling}")
    }
}

/// The raid log of the knowledge base `db` at `root`: everything above,
/// newest first (a day's timed entries before its untimed ones), at most
/// `limit`. `repos` are the workspace's other repos (name and checkout),
/// for their releases; `offset_secs` is the local time's offset from UTC,
/// so a finding falls on the person's own day.
pub fn raid_log(db: &Db, root: &Path, repos: &[(String, PathBuf)], offset_secs: i64, limit: usize) -> Result<Vec<Entry>> {
    let mut out: Vec<Entry> = Vec::new();

    // Runs: ingest, drift, lint.
    if let Ok(text) = std::fs::read_to_string(root.join(crate::ingest::LOG_PAGE)) {
        out.extend(log_lines(&text));
    }

    // Rulings.
    let roles = crate::authority::Roles::of_repo(root);
    for log in crate::supersede::log_paths(db, &roles)? {
        for e in crate::supersede::entries(db, &log)? {
            if is_date(&e.date) {
                out.push(Entry::new(&e.date, "ruling", &ruling_text(&e.id, &e.text)).at(&log, Some(e.line)));
            }
        }
    }

    // Ticket changes: the day it was last updated, and where it stands.
    for path in db.page_paths()?.into_iter().filter(|p| crate::day::is_ticket_path(p)) {
        let text = db.get_text(&path)?.unwrap_or_default();
        let t = crate::day::parse_ticket(&path, &text);
        let updated = db.page_meta(&path)?.and_then(|m| m.updated).map(|u| u.chars().take(10).collect::<String>());
        let date = match updated.filter(|d| is_date(d)) {
            Some(d) => d,
            None => match db.get_file(&path)? {
                Some(f) => crate::day::date_of_epoch(f.mtime + offset_secs),
                None => continue,
            },
        };
        let state = if t.state.is_empty() { String::new() } else { format!(" · {}", t.state) };
        out.push(Entry::new(&date, "ticket", &format!("{}{state}: {}", t.id, t.title)).at(&path, None));
    }

    // Commits: the knowledge base's own, then the other repos' that cite a
    // ticket or a ruling.
    let mut own_shas: std::collections::HashSet<String> = Default::default();
    for c in crate::git::log(root, KB_COMMITS, None, &[]).unwrap_or_default() {
        let mut e = Entry::new(&c.date, "commit", &format!("{} ({})", c.subject, c.author));
        own_shas.insert(c.sha.clone());
        e.reference = Some(c.sha);
        out.push(e);
    }
    // One entry a commit, however many ids it cites.
    let mut cited: std::collections::BTreeMap<String, (String, String)> = Default::default();
    for edge in db.graph_edges_of(crate::graph::ORIGIN_COMMITS)? {
        let Some(node) = crate::graph::Node::parse(&edge.src) else { continue };
        let (date, subject) = edge.detail.split_once(' ').unwrap_or((edge.detail.as_str(), ""));
        cited.entry(node.id).or_insert_with(|| (date.to_string(), subject.to_string()));
    }
    for (key, (date, subject)) in cited {
        let Some((repo, sha)) = key.rsplit_once('@') else { continue };
        if !is_date(&date) || own_shas.contains(sha) {
            continue;
        }
        let mut e = Entry::new(&date, "commit", &subject);
        e.repo = Some(repo.to_string());
        e.reference = Some(sha.to_string());
        out.push(e);
    }

    // Findings: filed, and resolved.
    for item in db.review_items_of_kind(crate::findings::KIND, FINDINGS)? {
        let Some(f) = item.payload.as_deref().and_then(|p| serde_json::from_str::<crate::findings::Finding>(p).ok()) else {
            continue;
        };
        let page = f.page();
        let mut moments = vec![("filed", item.created_at)];
        if let Some(at) = item.resolved_at {
            moments.push(("resolved", at));
        }
        for (what, at) in moments {
            let mut e = Entry::new(&crate::day::date_of_epoch(at + offset_secs), "finding", &format!("{} {what}: {}", f.kind, f.text));
            e.at = Some(at);
            if !page.is_empty() {
                e.path = Some(page.clone());
            }
            out.push(e);
        }
    }

    // Releases: the knowledge base's tags and each repo's.
    let own = (String::new(), root.to_path_buf());
    for (name, dir) in std::iter::once(&own).chain(repos.iter()) {
        for t in crate::git::tags(dir, TAGS).unwrap_or_default() {
            if !is_date(&t.date) {
                continue;
            }
            let text = if t.subject.is_empty() || t.subject == t.name { t.name.clone() } else { format!("{}: {}", t.name, t.subject) };
            let mut e = Entry::new(&t.date, "release", &text);
            e.repo = (!name.is_empty()).then(|| name.clone());
            e.reference = Some(t.name);
            out.push(e);
        }
    }

    sort(&mut out);
    out.truncate(limit);
    Ok(out)
}

/// Newest first: by day, then a timed entry by its moment before the day's
/// untimed ones, then in the order they were read.
pub fn sort(entries: &mut [Entry]) {
    entries.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| b.at.unwrap_or(i64::MIN).cmp(&a.at.unwrap_or(i64::MIN))));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Project;
    use std::fs;

    #[test]
    fn log_lines_read_each_run_with_its_line() {
        let text = "# Log\n\n## [2026-10-01] ingest | Standup\n\nnot a line\n## [2026-10-02] drift | 2 checked, 1 fixed\n## [someday] lint | x\n";
        let got = log_lines(text);
        assert_eq!(got.len(), 2);
        assert_eq!((got[0].date.as_str(), got[0].kind.as_str(), got[0].text.as_str()), ("2026-10-01", "ingest", "Standup"));
        assert_eq!((got[0].path.as_deref(), got[0].line), (Some("log.md"), Some(3)));
        assert_eq!((got[1].kind.as_str(), got[1].text.as_str()), ("drift", "2 checked, 1 fixed"));
    }

    /// One list, newest first, each entry linked to its source: runs from
    /// `log.md`, rulings, ticket changes, findings filed and resolved, and
    /// the other repos' commits that cite a ticket or a ruling.
    #[test]
    fn the_raid_log_is_one_list_newest_first_with_links() {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in [
            ("log.md", "# Log\n\n## [2026-10-01] ingest | Standup\n"),
            ("Decisions/DECISIONS.md", "# Decisions\n\n## D-012 · 2026-10-02 · saves\n\n**Regions save apart.**\n"),
            ("tickets/SR-101.md", "---\nid: SR-101\nstatus: done\nupdated: 2026-10-04\n---\n# Fix saves\n"),
        ] {
            let p = dir.path().join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        let project = Project::create(dir.path(), "Lib").unwrap();
        let mut db = Db::open_in_memory().unwrap();
        crate::scan::scan(&project, &mut db).unwrap();
        // A finding filed on 2026-10-05 (UTC noon) and resolved a day later.
        let filed = 1_791_201_600;
        let f = crate::findings::Finding {
            kind: "orphan".into(),
            source: "lint".into(),
            subject: "Design/Lonely.md".into(),
            text: "Nothing links to Design/Lonely.md.".into(),
            ..Default::default()
        };
        let id = crate::findings::file(&mut db, &f, filed).unwrap();
        db.resolve_review_item(id, filed + 86_400).unwrap();
        // A commit in another repo citing the ticket.
        db.add_graph_edges(crate::graph::ORIGIN_COMMITS, &[crate::graph::Edge {
            src: "commit:game@1a2b3c4".into(),
            dst: "ticket:tickets/SR-101.md".into(),
            kind: "commit".into(),
            detail: "2026-10-03 Fix SR-101".into(),
        }])
        .unwrap();

        let log = raid_log(&db, dir.path(), &[], 0, 100).unwrap();
        let kinds: Vec<(&str, &str)> = log.iter().map(|e| (e.date.as_str(), e.kind.as_str())).collect();
        assert_eq!(
            kinds,
            vec![
                ("2026-10-06", "finding"),
                ("2026-10-05", "finding"),
                ("2026-10-04", "ticket"),
                ("2026-10-03", "commit"),
                ("2026-10-02", "ruling"),
                ("2026-10-01", "ingest"),
            ],
            "{log:#?}"
        );
        assert!(log[0].text.starts_with("orphan resolved") && log[1].text.starts_with("orphan filed"), "{log:#?}");
        assert_eq!(log[0].path.as_deref(), Some("Design/Lonely.md"));
        assert_eq!(log[0].at, Some(filed + 86_400));
        assert_eq!((log[2].text.as_str(), log[2].path.as_deref()), ("SR-101 · done: Fix saves", Some("tickets/SR-101.md")));
        assert_eq!((log[3].repo.as_deref(), log[3].reference.as_deref(), log[3].text.as_str()), (Some("game"), Some("1a2b3c4"), "Fix SR-101"));
        assert_eq!(log[4].text, "D-012: Regions save apart.");
        assert_eq!(log[4].path.as_deref(), Some("Decisions/DECISIONS.md"));
        assert!(log[4].line.is_some_and(|l| l > 1));
        assert_eq!((log[5].path.as_deref(), log[5].line), (Some("log.md"), Some(3)));
        assert_eq!(raid_log(&db, dir.path(), &[], 0, 2).unwrap().len(), 2);
    }
}
