//! Drift (knowledge-core decision 28): the checks that read pages and the
//! decisions log against the code they cite (Ways of Working, drift). A
//! declaration goes stale without a test failing; this compares what a page
//! or a ruling declared with what is there now, says which of three things
//! each mismatch is, and acts on it.
//!
//! - **Docs check**: a page's `checked:` stamp pins a commit in each cited
//!   repo (its commit where that repo has it, else the last commit on its
//!   date); a source that moved between that pin and the default branch
//!   makes the page suspect. A page never checked is measured from the
//!   commit each citation names (`repo@sha:path`), else its `updated` date.
//! - **Decision check**: each decisions-log entry, measured from the date it
//!   declares for itself, against the code it cites. Superseded entries are
//!   skipped.
//! - **Age rule**: a page with a `sources:` list not checked in thirty days
//!   is raised even when nothing it cites moved. A page with no sources (an
//!   index, START-HERE) carries no `checked:` and is never raised.
//!
//! A sweep works from what changed: once per repo it asks git which files
//! moved since the last sweep's commit, re-measures only the citations of
//! those files, and carries every other result over (see `PinCache`).
//!
//! Only code citations (`repo:path[:line]`) are measured; a `[[Note]]` or a
//! `D-nnn` is a cross-reference, counted apart. Research pages report in
//! their own bucket and are never acted on. Every run can carry two controls
//! (a page known to have moved, one known clean) and a minimum page count; a
//! wrong control or too few pages voids the run, and a run with no controls
//! says so. The settings live in the knowledge base's `.ken/knowledge.json`
//! under `drift` ([`DriftConfig::for_repo`]).
//!
//! **When a run happens** ([`due`]): when a cited repo's branch has moved
//! since the last run, a week after the last run (the age rule's turn, for
//! pages whose sources do not move), and an hour after a run that left pages
//! waiting for a re-read.
//!
//! **What a run does** ([`act`]): a page whose sources changed only in
//! whitespace or comments has its `checked:` stamp advanced, silently. A
//! page with a real change, or past the age rule, is re-read by Claude (the
//! caller's `reread`, never the local model) against the diff or the code as
//! it is, and either stamped or fixed one section at a time through the
//! ingest edit rules: held for a person when ingest would hold it. A cited
//! file that was renamed is followed and the citation rewritten when git
//! names one new path. A ruling whose cited code changed is never edited: it
//! becomes a finding, as does a source that is gone. Each run that writes is
//! one commit in the knowledge base and one `log.md` line.
//!
//! The Scope and Answer checks read a ticket and its diff, so they stay with
//! Wright (decision 31), not here.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::pagemeta::{section_of, Section};
use crate::project::Project;
use crate::Result;

/// A page not checked for longer than this is raised by the age rule.
pub const AGE_DAYS: i64 = 30;
/// A run happens at least this often, for the age rule: pages whose
/// sources never move are still read again.
pub const AGE_RUN_DAYS: i64 = 7;
/// How long pages left waiting for a re-read (Claude not there, the hourly
/// cap reached) wait before a run tries them again, in seconds.
pub const RETRY_SECS: i64 = 3_600;
/// What a held edit from a drift run names as where it came from
/// (`Proposal::from`), so applying it is a commit of its own.
pub const HELD_FROM: &str = "drift";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    /// Only whitespace or comments changed: the pin can advance, nobody reads it.
    AutoRecleared,
    /// A real change under a cited source: is the page (or ruling) still true?
    Judgment,
    /// A cited path is gone, or a cited line is past the end of the file.
    Finding,
}

/// One cited source that did not hold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mismatch {
    /// The page, or `DECISIONS.md#D-012` for a ruling.
    pub subject: String,
    pub citation: String,
    pub severity: Severity,
    pub detail: String,
    /// Research: evidence, reported apart from live pages.
    pub research: bool,
    /// The commit it was measured from, and the branch it was measured
    /// against, when known: what a rename is looked for between.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// The knowledge base's drift settings: `drift` in its `.ken/knowledge.json`
/// (one config, decision 28). All optional. An older `intervalDays` is
/// ignored: a run follows the code now, not a schedule.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftConfig {
    /// A page whose sources are known to have moved since its pin.
    #[serde(default)]
    pub control_moved: Option<String>,
    /// A page known to be clean.
    #[serde(default)]
    pub control_clean: Option<String>,
    /// Fewer pages examined than this voids the run.
    #[serde(default)]
    pub min_pages: Option<usize>,
    /// `false` measures against the remote branches as last fetched; by
    /// default each sweep fetches them first.
    #[serde(default)]
    pub fetch: Option<bool>,
}

impl DriftConfig {
    /// The settings the repo at `root` reads (`knowledge_settings`), the
    /// defaults when it has none.
    pub fn for_repo(root: &Path) -> DriftConfig {
        crate::knowledge_settings::for_repo(root).map(|s| DriftConfig::from_settings(&s.value)).unwrap_or_default()
    }

    /// `drift` in a settings file's JSON.
    pub fn from_settings(value: &serde_json::Value) -> DriftConfig {
        value.get("drift").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
    }
}

/// What one sweep found, and whether it can be trusted.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftRun {
    pub at: i64,
    /// 0 clean · 1 a Finding, or the instrument broken · 2 drift only.
    pub exit_code: i32,
    pub pages_examined: usize,
    pub rulings_examined: usize,
    /// Code citations measured, and cross-references counted apart.
    pub code_citations: usize,
    pub cross_references: usize,
    pub mismatches: Vec<Mismatch>,
    /// Pages raised by the age rule: (page, checked date or none).
    pub aged: Vec<(String, Option<String>)>,
    /// Why the run is void, if it is: a wrong control, too few pages.
    pub void_reason: Option<String>,
    /// The run had no controls declared, so a clean result proves less.
    pub uncontrolled: bool,
    /// What the run could not measure, and why (a repo not found, no git).
    pub unmeasured: Vec<String>,
    /// Branches measured against, per repo, with a note when it fell back.
    pub branches: Vec<String>,
    /// Citations measured with git this sweep, and those carried from the
    /// last sweep because no commit since touched their file.
    #[serde(default)]
    pub measured: usize,
    #[serde(default)]
    pub reused: usize,
    /// What the run did ([`act`]): pages whose `checked` stamp advanced,
    /// pages fixed, edits held for a person, citations rewritten after a
    /// rename, findings filed, pages left waiting for a re-read, and the
    /// run's commit (or why there is none).
    #[serde(default)]
    pub stamped: Vec<String>,
    #[serde(default)]
    pub fixed: Vec<String>,
    #[serde(default)]
    pub held: Vec<String>,
    #[serde(default)]
    pub renamed: Vec<String>,
    #[serde(default)]
    pub findings: usize,
    #[serde(default)]
    pub waiting: Vec<String>,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub commit_error: Option<String>,
}

impl DriftRun {
    fn finish(&mut self) {
        let live = |m: &&Mismatch| !m.research && m.severity != Severity::AutoRecleared;
        self.exit_code = if self.void_reason.is_some() || self.mismatches.iter().filter(live).any(|m| m.severity == Severity::Finding) {
            1
        } else if self.mismatches.iter().any(|m| m.severity != Severity::AutoRecleared) || !self.aged.is_empty() {
            2
        } else {
            0
        };
    }

    /// The run as text: a Finding first, then judgments and aged pages as
    /// one line each, Research apart. Auto-recleared diffs are only counted.
    pub fn to_markdown(&self) -> String {
        let mut s = String::new();
        if let Some(v) = &self.void_reason {
            s.push_str(&format!("**This run is void:** {v}. Nothing below can be trusted as clean.\n\n"));
        }
        s.push_str(&format!(
            "{} pages and {} rulings examined; {} code citations measured, {} cross-references counted apart.\n",
            self.pages_examined, self.rulings_examined, self.code_citations, self.cross_references
        ));
        if self.uncontrolled {
            s.push_str("No controls are declared (`drift.controlMoved` and `drift.controlClean` in .ken/knowledge.json), so a clean result proves less.\n");
        }
        let auto = self.mismatches.iter().filter(|m| m.severity == Severity::AutoRecleared).count();
        if auto > 0 {
            s.push_str(&format!("{auto} source changes were whitespace or comments only and are cleared.\n"));
        }
        let section = |title: &str, sev: Severity, research: bool, s: &mut String| {
            let rows: Vec<&Mismatch> =
                self.mismatches.iter().filter(|m| m.severity == sev && m.research == research).collect();
            if !rows.is_empty() {
                s.push_str(&format!("\n**{title}**\n"));
                for m in rows {
                    s.push_str(&format!("- {} — {}: {}\n", m.subject, m.citation, m.detail));
                }
            }
        };
        section("Findings: a cited source is gone", Severity::Finding, false, &mut s);
        section("Is the page still true? A cited source changed", Severity::Judgment, false, &mut s);
        if !self.aged.is_empty() {
            s.push_str(&format!("\n**Not checked in {AGE_DAYS} days**\n"));
            for (page, v) in &self.aged {
                s.push_str(&format!("- {page} — {}\n", v.as_deref().map_or("never checked".to_string(), |d| format!("checked {d}"))));
            }
        }
        section("Research (evidence at the time, reported apart): source gone", Severity::Finding, true, &mut s);
        section("Research (evidence at the time, reported apart): source changed", Severity::Judgment, true, &mut s);
        if !self.unmeasured.is_empty() {
            s.push_str("\n**Not measured**\n");
            for u in &self.unmeasured {
                s.push_str(&format!("- {u}\n"));
            }
        }
        s
    }
}

/// A code citation: `repo:path` or `repo:path:line` (also `repo@sha:path`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeCitation {
    pub repo: String,
    /// The commit it was read at, when the citation says (`repo@sha:path`).
    pub at: Option<String>,
    pub path: String,
    pub line: Option<usize>,
}

pub enum Citation {
    Code(CodeCitation),
    /// `[[Note]]` or `D-nnn`: a cross-reference, not measured.
    Cross,
    /// A placeholder, an elided path, or not a locator at all.
    Skip,
}

/// Classify one entry of a `sources:` list or a ruling's `sources:` line.
pub fn classify(raw: &str) -> Citation {
    let c = raw.trim().trim_matches(|ch| ch == '"' || ch == '\'' || ch == '`').trim();
    if c.is_empty() || c.contains("{{") || c.contains("...") || c.contains('…') {
        return Citation::Skip;
    }
    if c.starts_with("[[") || (c.starts_with("D-") && c[2..].chars().next().is_some_and(|ch| ch.is_ascii_digit())) {
        return Citation::Cross;
    }
    if c.contains("://") {
        return Citation::Skip;
    }
    let Some((repo, rest)) = c.split_once(':') else {
        return Citation::Skip;
    };
    let (repo, at) = match repo.split_once('@') {
        Some((r, sha)) => (r.trim(), Some(sha.trim()).filter(|h| h.len() >= 4 && h.chars().all(|ch| ch.is_ascii_hexdigit())).map(str::to_string)),
        None => (repo.trim(), None),
    };
    if repo.is_empty() || repo.contains(' ') || rest.is_empty() {
        return Citation::Skip;
    }
    let (path, line) = match rest.rsplit_once(':') {
        Some((p, l)) if l.chars().all(|ch| ch.is_ascii_digit()) && !l.is_empty() => (p, l.parse().ok()),
        _ => (rest, None),
    };
    let path = path.trim_start_matches("./").replace('\\', "/");
    // `repo:(layout)`, `repo:(git authors)`: a fact Ken derived, not a file.
    if path.is_empty() || path.starts_with('(') || path.contains(' ') && !path.contains('/') {
        return Citation::Skip;
    }
    Citation::Code(CodeCitation { repo: repo.to_string(), at, path, line })
}

/// The folder a citation's repo names: this project if it is named so,
/// a registered project of that name, or a sibling folder of that name.
pub fn resolve_repo(project_root: &Path, repo: &str) -> Option<PathBuf> {
    let own = project_root.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if own.eq_ignore_ascii_case(repo) {
        return Some(project_root.to_path_buf());
    }
    if let Ok(reg) = crate::registry::default_base_dir().and_then(|b| crate::registry::Registry::load(&b)) {
        if let Some(e) = reg.projects.iter().find(|e| {
            e.name.eq_ignore_ascii_case(repo) || e.path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(repo))
        }) {
            return Some(e.path.clone());
        }
    }
    let sibling = project_root.parent()?.join(repo);
    sibling.is_dir().then_some(sibling)
}

/// Git in `root`, through the one helper history shares (`crate::git`,
/// decision 27): its trimmed answer, None when it fails.
fn git(root: &Path, args: &[&str]) -> Option<String> {
    crate::git::output(root, args)
}

/// The branch to measure against: the remote's default, read from git, else
/// the checked-out branch, said so. A checkout on a branch the default does
/// not contain (a long-lived design branch, a worktree kept for one) is what
/// Ken read and the wiki was drafted from, so it is measured against that
/// branch's upstream, said so.
pub fn default_branch(root: &Path) -> (String, Option<String>) {
    let head = git(root, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "HEAD".into());
    let Some(default) = git(root, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]) else {
        let note = format!("no remote default branch found; measured against {head}, not the default");
        return (head, Some(note));
    };
    if git(root, &["merge-base", "--is-ancestor", "HEAD", &default]).is_some() {
        return (default, None);
    }
    let branch = git(root, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]).unwrap_or(head);
    let note = format!("checked out on {branch}, which {default} does not contain; measured against {branch}");
    (branch, Some(note))
}

/// The last commit on `branch` at or before the end of `date` (`YYYY-MM-DD`).
fn pin_at(root: &Path, branch: &str, date: &str) -> Option<String> {
    let before = format!("--before={date}T23:59:59");
    git(root, &["rev-list", "-1", &before, branch]).filter(|s| !s.is_empty())
}

fn is_comment_or_blank(line: &str) -> bool {
    let t = line.trim();
    t.is_empty()
        || ["//", "#", "--", "/*", "*", "*/", "<!--", "-->", ";", "%", "'"].iter().any(|p| t.starts_with(p))
}

/// Measure one code citation in `repo_root`: first whether it still points at
/// anything on `branch` (no pin needed), then, with a pin, whether it moved.
/// `Err` carries why it could not be measured.
fn measure(repo_root: &Path, pin: Option<&str>, branch: &str, c: &CodeCitation) -> std::result::Result<Option<(Severity, String)>, String> {
    if let Some(found) = still_there(repo_root, branch, c) {
        return Ok(Some(found));
    }
    let Some(pin) = pin else {
        return Err("no commit on or before its date to pin to".into());
    };
    Ok(moved(repo_root, pin, branch, c))
}

/// A Finding when the cited path is gone or the cited line is past the end.
fn still_there(repo_root: &Path, branch: &str, c: &CodeCitation) -> Option<(Severity, String)> {
    let file = repo_root.join(&c.path);
    let at_head = git(repo_root, &["cat-file", "-e", &format!("{branch}:{}", c.path)]).is_some();
    if !at_head {
        let detail = if file.exists() { format!("not on {branch}") } else { "the path no longer exists".to_string() };
        return Some((Severity::Finding, detail));
    }
    if let Some(line) = c.line {
        let text = git(repo_root, &["show", &format!("{branch}:{}", c.path)]).unwrap_or_default();
        let lines = text.lines().count();
        if line > lines {
            return Some((Severity::Finding, format!("line {line} is past the end of the file ({lines} lines): it was rewritten under the citation")));
        }
    }
    None
}

/// Judgment or auto-recleared when the cited file changed from `pin`.
fn moved(repo_root: &Path, pin: &str, branch: &str, c: &CodeCitation) -> Option<(Severity, String)> {
    let changed = git(repo_root, &["diff", "--name-only", pin, branch, "--", &c.path]).unwrap_or_default();
    if changed.is_empty() {
        return None;
    }
    let diff = git(repo_root, &["diff", "-w", "--ignore-blank-lines", "-U0", pin, branch, "--", &c.path]).unwrap_or_default();
    let substantive = diff
        .lines()
        .filter(|l| (l.starts_with('+') || l.starts_with('-')) && !l.starts_with("+++") && !l.starts_with("---"))
        .any(|l| !is_comment_or_blank(&l[1..]));
    if !substantive {
        return Some((Severity::AutoRecleared, "whitespace or comments only".into()));
    }
    let short = &pin[..pin.len().min(7)];
    Some((Severity::Judgment, format!("changed since {short} (git diff {short} {branch} -- {})", c.path)))
}

/// A decisions-log entry: id, its own date, its sources, whether superseded.
#[derive(Debug, Clone, PartialEq)]
pub struct Ruling {
    pub id: String,
    pub date: Option<String>,
    pub sources: Vec<String>,
    pub superseded: bool,
}

/// Entries of a decisions log (the fenced format example skipped).
pub fn parse_rulings(text: &str) -> Vec<Ruling> {
    let mut out: Vec<Ruling> = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        // `**D-410** · …` reads as `D-410 · …`.
        let unbold = crate::chunker::unbold_id(t);
        let t = unbold.trim();
        if t.starts_with("D-") && t[2..].starts_with(|c: char| c.is_ascii_digit()) {
            let sep = if t.contains(" · ") { " · " } else { " - " };
            let mut fields = t.split(sep);
            let id = fields.next().unwrap_or("").trim().to_string();
            let date = fields.next().map(str::trim).filter(|d| d.len() >= 10).map(|d| d[..10].to_string());
            out.push(Ruling { id, date, sources: Vec::new(), superseded: t.contains("[SUPERSEDED BY") });
            continue;
        }
        let Some(r) = out.last_mut() else { continue };
        if let Some(rest) = t.strip_prefix("sources:") {
            r.sources.extend(rest.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
        }
        if t.contains("[SUPERSEDED BY") {
            r.superseded = true;
        }
    }
    out
}

pub(crate) fn days_between(earlier: &str, now_epoch: i64) -> Option<i64> {
    let d = chrono_free_epoch(earlier)?;
    Some((now_epoch - d) / 86_400)
}

/// `YYYY-MM-DD` to Unix seconds at midnight UTC, without a date crate.
fn chrono_free_epoch(date: &str) -> Option<i64> {
    let y: i64 = date.get(0..4)?.parse().ok()?;
    let m: i64 = date.get(5..7)?.parse().ok()?;
    let d: i64 = date.get(8..10)?.parse().ok()?;
    // Days from civil (Howard Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400)
}

/// What one citation measured to, kept between sweeps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Measured {
    Clean,
    Hit(Severity, String),
    Unmeasured(String),
}

/// The sweep's memory: per repo, the commit the last sweep measured
/// against, and each citation's result there. A result holds until a
/// commit touches its file, so a sweep asks git once per repo which files
/// changed since (`git diff --name-only <last> <now>`) and re-measures only
/// the citations of those files. The result is what a full sweep gives:
/// a file untouched since the last sweep reads the same at both commits.
#[derive(Debug, Default, Serialize, Deserialize)]
struct PinCache {
    /// How results were measured; a cache from another way is dropped.
    #[serde(default)]
    version: u32,
    /// Repo folder → the branch head the results were measured at.
    heads: std::collections::HashMap<String, String>,
    /// Citation key → result. Keyed by repo, branch, pin, path and line, so a
    /// page checked again (a new pin) measures afresh.
    results: std::collections::HashMap<String, Measured>,
}

impl PinCache {
    /// Bumped whenever measuring changes (what counts as a citation, how a
    /// path resolves), so an upgrade never shows results measured the old way.
    const VERSION: u32 = 2;

    fn load(db: &Db) -> PinCache {
        db.drift_cache()
            .ok()
            .flatten()
            .and_then(|j| serde_json::from_str::<PinCache>(&j).ok())
            .filter(|c| c.version == Self::VERSION)
            .unwrap_or(PinCache { version: Self::VERSION, ..Default::default() })
    }

    fn key(repo: &Path, branch: &str, pin: Option<&str>, c: &CodeCitation) -> String {
        format!("{}\u{1f}{branch}\u{1f}{}\u{1f}{}\u{1f}{:?}", repo.display(), pin.unwrap_or("-"), c.path, c.line)
    }

    /// Bring one repo's results up to `head`: drop those whose file changed
    /// since the last sweep, or all of them when git cannot say.
    fn advance(&mut self, repo: &Path, head: Option<&str>) {
        let r = repo.display().to_string();
        let prefix = format!("{r}\u{1f}");
        let old = self.heads.get(&r).cloned();
        match (old.as_deref(), head) {
            (Some(o), Some(h)) if o == h => {}
            (Some(o), Some(h)) => match git(repo, &["diff", "--name-only", o, h]) {
                Some(changed) => {
                    let changed: std::collections::HashSet<&str> = changed.lines().map(str::trim).collect();
                    self.results.retain(|k, _| {
                        !k.starts_with(&prefix) || k.split('\u{1f}').nth(3).is_none_or(|p| !changed.contains(p))
                    });
                }
                // The old head is gone (a force push): nothing carries over.
                None => self.results.retain(|k, _| !k.starts_with(&prefix)),
            },
            _ => self.results.retain(|k, _| !k.starts_with(&prefix)),
        }
        match head {
            Some(h) => {
                self.heads.insert(r, h.to_string());
            }
            None => {
                self.heads.remove(&r);
            }
        }
    }
}

/// How long a fetch may take before the sweep goes on without it.
const FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// `git fetch <remote>`: remote-tracking branches only; the working tree,
/// local branches and the remote are untouched. Never prompts: a remote that
/// needs a password a person has not stored fails, and is said so.
fn fetch(root: &Path, remote: &str) -> std::result::Result<(), String> {
    let mut cmd = crate::git::command(root);
    cmd.args(["fetch", "--quiet", "--no-tags", remote])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + FETCH_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => {
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    let _ = std::io::Read::read_to_string(&mut e, &mut err);
                }
                return Err(err.lines().last().unwrap_or("git fetch failed").trim().to_string());
            }
            Ok(None) if std::time::Instant::now() > deadline => {
                crate::proc::kill_tree(&mut child);
                return Err(format!("no answer in {}s", FETCH_TIMEOUT.as_secs()));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(200)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// A page whose frontmatter title is still a `{{placeholder}}`.
fn is_template(text: &str) -> bool {
    text.lines()
        .take_while(|l| !l.trim_start().starts_with("# "))
        .any(|l| l.trim_start().starts_with("title:") && l.contains("{{"))
}

/// Run the sweep over one knowledge base.
pub fn sweep(project: &Project, db: &Db, now: i64) -> Result<DriftRun> {
    let cfg = DriftConfig::for_repo(&project.root);
    let mut run = DriftRun { at: now, ..Default::default() };
    let mut branches: std::collections::HashMap<PathBuf, (String, Option<String>)> = Default::default();
    let mut cache = PinCache::load(db);
    let mut advanced: std::collections::HashSet<PathBuf> = Default::default();
    let mut pins: std::collections::HashMap<(PathBuf, String), Option<String>> = Default::default();
    let mut used: std::collections::HashSet<String> = Default::default();
    let mut repos_in_git: std::collections::HashMap<PathBuf, bool> = Default::default();
    let mut tracked: std::collections::HashMap<(PathBuf, String), Vec<String>> = Default::default();
    // `exact`: a citation pinned to its commit (`repo@sha:path`) is measured
    // from that commit. A draft pins what it read; once the check has re-read
    // the page, its `checked` stamp is the pin instead: `checked_sha` where
    // the cited repo has that commit, else the last commit on `since`.
    let mut measure_one = |subject: &str, raw: &str, since: &str, exact: bool, checked_sha: Option<&str>, research: bool, run: &mut DriftRun| {
        let c = match classify(raw) {
            Citation::Code(c) => c,
            Citation::Cross => {
                run.cross_references += 1;
                return;
            }
            Citation::Skip => return,
        };
        let Some(repo_root) = resolve_repo(&project.root, &c.repo) else {
            run.code_citations += 1;
            let u = format!("repo `{}` (cited by {subject}) was not found beside this one or in Ken", c.repo);
            if !run.unmeasured.contains(&u) {
                run.unmeasured.push(u);
            }
            return;
        };
        // A page citing another page of this wiki is a cross-reference, like
        // `[[Note]]`: measuring it against git would flag every page not yet
        // committed (a fresh draft) as missing.
        if repo_root == project.root && c.path.to_ascii_lowercase().ends_with(".md") {
            run.cross_references += 1;
            return;
        }
        run.code_citations += 1;
        // A folder without git can be read but not measured.
        let in_git = *repos_in_git
            .entry(repo_root.clone())
            .or_insert_with(|| git(&repo_root, &["rev-parse", "--is-inside-work-tree"]).is_some_and(|o| o == "true"));
        if !in_git {
            let u = format!("`{}` is not a git repository, so its citations cannot be measured", c.repo);
            if !run.unmeasured.contains(&u) {
                run.unmeasured.push(u);
            }
            return;
        }
        let (branch, note) = branches.entry(repo_root.clone()).or_insert_with(|| default_branch(&repo_root)).clone();
        if let Some(n) = note {
            let n = format!("{}: {n}", c.repo);
            if !run.branches.contains(&n) {
                run.branches.push(n);
            }
        }
        // Once per repo per sweep: bring its remote branches up to date, so
        // drift is measured against what is merged, not what was there at the
        // last fetch (nothing else fetches a code repo); then which files
        // changed since the last sweep.
        if advanced.insert(repo_root.clone()) {
            if let Some(remote) = branch.split_once('/').map(|(r, _)| r).filter(|_| cfg.fetch != Some(false)) {
                if let Err(why) = fetch(&repo_root, remote) {
                    let n = format!("{}: could not fetch {remote} ({why}); measured against its last fetch", c.repo);
                    if !run.branches.contains(&n) {
                        run.branches.push(n);
                    }
                }
            }
            let head = git(&repo_root, &["rev-parse", &branch]);
            cache.advance(&repo_root, head.as_deref());
        }
        let exact_pin = c
            .at
            .as_deref()
            .filter(|_| exact)
            .or(checked_sha)
            .and_then(|sha| git(&repo_root, &["rev-parse", "--verify", "--quiet", &format!("{sha}^{{commit}}")]));
        let pin = match exact_pin {
            Some(sha) => Some(sha),
            None => pins
                .entry((repo_root.clone(), since.to_string()))
                .or_insert_with(|| pin_at(&repo_root, &branch, since))
                .clone(),
        };
        let key = PinCache::key(&repo_root, &branch, pin.as_deref(), &c);
        used.insert(key.clone());
        let result = match cache.results.get(&key) {
            Some(m) => {
                run.reused += 1;
                m.clone()
            }
            None => {
                run.measured += 1;
                // Written by hand (or by a model), a path can differ from the
                // file in case alone: `readme.md` for `README.md`. Git is
                // case-sensitive, so measure the tracked path it means.
                let mut c = c.clone();
                if git(&repo_root, &["cat-file", "-e", &format!("{branch}:{}", c.path)]).is_none() {
                    let files = tracked.entry((repo_root.clone(), branch.clone())).or_insert_with(|| {
                        git(&repo_root, &["ls-tree", "-r", "--name-only", &branch])
                            .map(|out| out.lines().map(str::to_string).collect())
                            .unwrap_or_default()
                    });
                    if let Some(real) = files.iter().find(|f| f.eq_ignore_ascii_case(&c.path)) {
                        c.path = real.clone();
                    }
                }
                let m = match measure(&repo_root, pin.as_deref(), &branch, &c) {
                    Ok(None) => Measured::Clean,
                    Ok(Some((severity, detail))) => Measured::Hit(severity, detail),
                    Err(why) => Measured::Unmeasured(why),
                };
                cache.results.insert(key, m.clone());
                m
            }
        };
        match result {
            Measured::Hit(severity, detail) => run.mismatches.push(Mismatch {
                subject: subject.to_string(),
                citation: raw.trim().to_string(),
                severity,
                detail,
                research,
                pin: pin.clone(),
                branch: Some(branch.clone()),
            }),
            Measured::Clean => {}
            Measured::Unmeasured(why) => run.unmeasured.push(format!("{subject} — {}: {why} ({since})", raw.trim())),
        }
    };

    // Docs check and age rule, over every page with frontmatter.
    for page in db.page_paths()?.into_iter().filter(|p| p.ends_with(".md")) {
        let Some(meta) = db.page_meta(&page)? else { continue };
        if meta.retired() || meta.generated {
            continue; // evidence, or rebuilt from its generator: not a claim to re-verify
        }
        // A template to copy (`title: "{{Area}} — conventions"`) is not a
        // claim either. Page meta drops placeholder values, so a page with
        // no title is checked in its text.
        if meta.title.is_none() && db.get_text(&page)?.is_some_and(|t| is_template(&t)) {
            continue;
        }
        run.pages_examined += 1;
        let research = section_of(&page) == Some(Section::Research);
        let checked = meta.checked.as_ref().map(|c| c.date.clone());
        let checked_sha = meta.checked.as_ref().and_then(|c| c.sha.clone());
        let date = checked.clone().or_else(|| meta.updated.clone());
        // Only a page with a `sources:` list is checked (Chris's ruling): it
        // says something the check re-reads against its sources. A
        // navigation page (an index, START-HERE) has none and never ages.
        if !research && !meta.sources.is_empty() {
            // Never checked: aged once it has gone unchecked for as long
            // since it was written. A draft from today is waiting for its
            // first read, not stale; a new wiki is all such pages.
            let old = date.as_deref().and_then(|v| days_between(v, now)).is_none_or(|d| d > AGE_DAYS);
            if old {
                run.aged.push((page.clone(), checked.clone()));
            }
        }
        if let Some(since) = date {
            for src in &meta.sources {
                measure_one(&page, src, &since, checked.is_none(), checked_sha.as_deref(), research, &mut run);
            }
        }
    }

    // Decision check, per entry, from each entry's own date.
    for log in db.paths_named(&["decisions.md"])? {
        let Some(text) = db.get_text(&log)? else { continue };
        for r in parse_rulings(&text).into_iter().filter(|r| !r.superseded) {
            run.rulings_examined += 1;
            let Some(since) = r.date.clone() else { continue };
            let subject = format!("{log}#{}", r.id);
            for src in &r.sources {
                measure_one(&subject, src, &since, true, None, false, &mut run);
            }
        }
    }

    // Keep only what this sweep cited, so the memory stays the size of the wiki.
    cache.results.retain(|k, _| used.contains(k));
    if let Ok(json) = serde_json::to_string(&cache) {
        let _ = db.store_drift_cache(&json);
    }

    // Controls and the minimum count.
    match (&cfg.control_moved, &cfg.control_clean) {
        (Some(moved), Some(clean)) => {
            let flagged = |p: &str| run.mismatches.iter().any(|m| m.subject == p && m.severity != Severity::AutoRecleared);
            if !flagged(moved) {
                run.void_reason = Some(format!("the known-moved control {moved} read clean"));
            } else if flagged(clean) {
                run.void_reason = Some(format!("the known-clean control {clean} was flagged"));
            }
        }
        _ => run.uncontrolled = true,
    }
    if run.void_reason.is_none() {
        if let Some(min) = cfg.min_pages.filter(|m| run.pages_examined < *m) {
            run.void_reason = Some(format!("only {} pages examined, under the declared minimum of {min}", run.pages_examined));
        }
    }
    run.finish();
    Ok(run)
}

/// Whether a run is due ([`AGE_RUN_DAYS`], [`RETRY_SECS`]): none recorded
/// yet; a week since the last, for the age rule; an hour since a run that
/// left pages waiting for a re-read; or a repo the last run measured has a
/// new head on its branch ([`branch_moved`]).
pub fn due(db: &Db, now: i64) -> Result<bool> {
    let Some(last) = db.last_drift_run()? else { return Ok(true) };
    if now - last.at >= AGE_RUN_DAYS * 86_400 {
        return Ok(true);
    }
    if !last.waiting.is_empty() && now - last.at >= RETRY_SECS {
        return Ok(true);
    }
    Ok(branch_moved(db))
}

/// Whether a repo the last run measured has moved on: its branch's head,
/// as git has it here, is not the one the run measured at. Ken sees a move
/// when that branch moves locally (a pull, or any fetch; each run fetches
/// first); it does not fetch only to look.
pub fn branch_moved(db: &Db) -> bool {
    let cache = PinCache::load(db);
    cache.heads.iter().any(|(repo, head)| {
        let root = Path::new(repo);
        let (branch, _) = default_branch(root);
        git(root, &["rev-parse", &branch]).is_some_and(|now| &now != head)
    })
}

/// The kind of the Review items earlier versions filed for a run.
pub const REVIEW_KIND: &str = "drift";

/// Record the run; an item an earlier version filed for a run is resolved.
/// What the run found is in the findings list ([`act`]).
pub fn file_run(db: &mut Db, run: &DriftRun) -> Result<()> {
    db.insert_drift_run(run)?;
    while let Some((id, _)) = db.open_review_item_of_kind(REVIEW_KIND)? {
        db.resolve_review_item(id, run.at)?;
    }
    Ok(())
}

/// One Claude call for a re-read: a prompt in, the answer's text out. An
/// error (no Claude, or asked to stop while the throttle held it) leaves the page
/// waiting for a later run.
pub type Reread<'a> = dyn FnMut(&str) -> Result<String> + 'a;

/// One drift run: the sweep, what it found acted on ([`act`]), the run
/// recorded. `today` is the local date the stamps and the log line carry.
pub fn run(project: &Project, db: &mut Db, now: i64, today: &str, reread: Option<&mut Reread<'_>>) -> Result<DriftRun> {
    let mut run = sweep(project, db, now)?;
    act(project, db, &mut run, today, reread)?;
    file_run(db, &run)?;
    Ok(run)
}

/// What a finding from a run says it came from.
const SOURCE: &str = "drift";
/// What a void run's finding points at, so the next run that holds resolves it.
const THE_RUN: &str = "the run";

fn finding(subject: &str, against: &str, against_kind: &str, text: String) -> crate::findings::Finding {
    crate::findings::Finding {
        kind: "drift".into(),
        source: SOURCE.into(),
        subject: subject.into(),
        against: against.into(),
        against_kind: against_kind.into(),
        text,
        ..Default::default()
    }
}

/// `text` with its frontmatter's `checked:` set to `today` and `sha`.
pub fn stamp_checked(text: &str, today: &str, sha: Option<&str>) -> String {
    let checked = crate::pagemeta::Checked { date: today.to_string(), sha: sha.map(str::to_string) };
    crate::day::set_frontmatter(text, "checked", &checked.to_string())
}

/// The commit a page's check is stamped with: the head of the branch its
/// first code citation's repo is measured against (one sha per page, as
/// slice 3 chose).
fn head_for(project_root: &Path, sources: &[String]) -> Option<String> {
    sources.iter().find_map(|raw| {
        let Citation::Code(c) = classify(raw) else { return None };
        let repo = resolve_repo(project_root, &c.repo)?;
        let (branch, _) = default_branch(&repo);
        git(&repo, &["rev-parse", "--short=12", &branch]).filter(|s| !s.is_empty())
    })
}

/// The one new path git names for `path` renamed between `pin` and
/// `branch`; None when it was not renamed, or git names more than one.
fn renamed_to(repo_root: &Path, pin: &str, branch: &str, path: &str) -> Option<String> {
    let out = git(repo_root, &["diff", "-M", "--name-status", "--diff-filter=R", pin, branch])?;
    let found: Vec<String> = out
        .lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            let (status, from, to) = (f.next()?, f.next()?, f.next()?);
            (status.starts_with('R') && from.eq_ignore_ascii_case(path)).then(|| to.to_string())
        })
        .collect();
    (found.len() == 1).then(|| found[0].clone())
}

/// A file name without its folder and extension: the name a rename row
/// records when it changed (`SaveSystem` → `WorldSaver`).
fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(s, _)| s)
}

/// Longest diff or file a re-read quotes, in characters, and the most of
/// them in one prompt.
const QUOTE_CHARS: usize = 8_000;
const QUOTES_CHARS: usize = 24_000;

fn clip(s: &str, max: usize) -> String {
    let cut: String = s.chars().take(max).collect();
    if cut.len() < s.len() { format!("{cut}\n[…]") } else { cut }
}

/// What changed under one page's citations: the diff of each changed file
/// from its pin, or for a page past the age rule the cited files as they
/// are now.
fn evidence(project_root: &Path, sources: &[String], changed: &[&Mismatch]) -> String {
    let mut out = String::new();
    let add = |head: String, body: String, out: &mut String| {
        if out.len() < QUOTES_CHARS && !body.trim().is_empty() {
            out.push_str(&format!("\n--- {head}\n{}\n", clip(&body, QUOTE_CHARS)));
        }
    };
    if !changed.is_empty() {
        for m in changed {
            let Citation::Code(c) = classify(&m.citation) else { continue };
            let (Some(repo), Some(pin), Some(branch)) = (resolve_repo(project_root, &c.repo), m.pin.as_deref(), m.branch.as_deref()) else {
                continue;
            };
            let diff = git(&repo, &["diff", "-U3", pin, branch, "--", &c.path]).unwrap_or_default();
            add(format!("{} (git diff {} {branch})", m.citation, &pin[..pin.len().min(7)]), diff, &mut out);
        }
        return out;
    }
    for raw in sources {
        let Citation::Code(c) = classify(raw) else { continue };
        let Some(repo) = resolve_repo(project_root, &c.repo) else { continue };
        let (branch, _) = default_branch(&repo);
        let text = git(&repo, &["show", &format!("{branch}:{}", c.path)]).unwrap_or_default();
        add(format!("{raw} (as it is on {branch})"), text, &mut out);
    }
    out
}

/// The re-read of one page (decision 28): the page and what changed under
/// it; Claude answers CHECKED, or the one section to put in its place.
pub fn reread_prompt(page: &str, text: &str, evidence: &str, aged: bool) -> String {
    let why = if aged {
        format!("It has not been checked in {AGE_DAYS} days; the code it cites is below as it is now")
    } else {
        "Code it cites has changed; the changes are below".to_string()
    };
    format!(
        "You are checking one page of a team's knowledge base, `{page}`, against the code it cites. {why}.\n\n\
         Rules:\n\
         - If everything the page says still holds, reply exactly CHECKED.\n\
         - If part of it no longer holds, reply with the one section that holds it, corrected: its heading line, then \
           everything down to the next heading of the same or a higher level. Change only what the code changed; keep \
           every other line of the section exactly as it is. The section you send replaces that section of the page.\n\
         - Use only the page and the code below; do not use any tools.\n\
         - Leave the frontmatter alone; Ken stamps it.\n\
         - No preamble, no code fences.\n\n\
         THE PAGE NOW:\n{text}\n\nTHE CODE:\n{evidence}\n"
    )
}

/// Act on what a sweep found (decision 28), in the knowledge base at
/// `project.root`, recording what was done in `run`:
///
/// - A void run does nothing but say so, as a finding.
/// - A ruling whose cited code changed, or whose source is gone, becomes a
///   finding against the ruling. A ruling is never edited.
/// - A page's cited file that git says was renamed to one new path is
///   followed: the citation is rewritten, and a rename row is added to the
///   Vocabulary page when the file's name changed. A source that is gone
///   otherwise becomes a finding.
/// - A page whose sources changed only in whitespace or comments has its
///   `checked:` stamp advanced.
/// - A page with a real change, or past the age rule, is re-read with
///   `reread`: CHECKED (or no change) stamps it; a section replaces its
///   section through the ingest rules (`ingest::hold`), written or held for
///   a person as a page proposal. Without `reread`, or when a call fails,
///   the page waits for a later run.
///
/// A page holding a person's uncommitted work is left alone this run. What
/// was written is one commit (`git::commit_paths`, never pushed) with one
/// `log.md` line; the pages fixed wait for the lint after an edit.
pub fn act(project: &Project, db: &mut Db, run: &mut DriftRun, today: &str, mut reread: Option<&mut Reread<'_>>) -> Result<()> {
    let root = project.root.as_path();
    let now = run.at;
    if let Some(why) = &run.void_reason {
        let f = finding("", THE_RUN, "", format!("The last drift run is void: {why}. Nothing was stamped or fixed."));
        crate::findings::file(db, &f, now)?;
        run.findings += 1;
        return Ok(());
    }
    // A run that holds clears the void one before it.
    for (it, f) in crate::findings::open(db)? {
        if f.kind == "drift" && f.against == THE_RUN {
            db.resolve_review_item(it.id, now)?;
        }
    }

    let dirty = crate::git::uncommitted(root);
    let mut touched: Vec<String> = Vec::new();
    let mut by_page: std::collections::BTreeMap<String, Vec<Mismatch>> = Default::default();
    for m in run.mismatches.iter().filter(|m| !m.research) {
        match m.subject.split_once('#') {
            // A ruling: never edited, always for a person.
            Some((log, id)) => {
                if m.severity == Severity::AutoRecleared {
                    continue;
                }
                let what = if m.severity == Severity::Finding { "is gone" } else { "changed" };
                let text = format!(
                    "{id} cites {}, which {what}: {}. A ruling is never edited: read it against the code, and supersede it if it no longer holds.",
                    m.citation, m.detail
                );
                crate::findings::file(db, &finding(log, id, "ruling", text), now)?;
                run.findings += 1;
            }
            None => by_page.entry(m.subject.clone()).or_default().push(m.clone()),
        }
    }
    let aged: std::collections::HashSet<String> = run.aged.iter().map(|(p, _)| p.clone()).collect();
    let mut pages: Vec<String> = by_page.keys().cloned().chain(aged.iter().cloned()).collect();
    pages.sort();
    pages.dedup();

    for page in pages {
        if dirty.contains(&page) {
            continue;
        }
        let Ok(mut text) = std::fs::read_to_string(root.join(&page)) else { continue };
        let found = by_page.remove(&page).unwrap_or_default();

        // Gone: followed when git names the one new path, else a finding.
        for m in found.iter().filter(|m| m.severity == Severity::Finding) {
            let followed = match classify(&m.citation) {
                Citation::Code(c) => resolve_repo(root, &c.repo).and_then(|repo| {
                    let (pin, branch) = (m.pin.as_deref()?, m.branch.as_deref()?);
                    renamed_to(&repo, pin, branch, &c.path).map(|new| (c.path.clone(), new))
                }),
                _ => None,
            };
            match followed {
                Some((old, new)) if text.contains(m.citation.as_str()) => {
                    let cited = m.citation.replacen(old.as_str(), &new, 1);
                    text = text.replacen(m.citation.as_str(), &cited, 1);
                    write_page(root, &page, &text)?;
                    push_once(&mut touched, &page);
                    run.renamed.push(format!("{page}: {} → {cited}", m.citation));
                    if stem(&old) != stem(&new) && crate::vocab::add_rename(root, stem(&old), stem(&new), today)? {
                        push_once(&mut touched, crate::vocab::VOCABULARY_PAGE);
                    }
                }
                _ => {
                    let text = format!("{page} cites {}: {}.", m.citation, m.detail);
                    crate::findings::file(db, &finding(&page, &m.citation, "code", text), now)?;
                    run.findings += 1;
                }
            }
        }

        let changed: Vec<&Mismatch> = found.iter().filter(|m| m.severity == Severity::Judgment).collect();
        let is_aged = aged.contains(&page);
        let sources = db.page_meta(&page)?.map(|m| m.sources).unwrap_or_default();
        let sha = head_for(root, &sources);
        if changed.is_empty() && !is_aged {
            // Whitespace or comments only: the stamp advances, silently.
            if found.iter().any(|m| m.severity == Severity::AutoRecleared) {
                write_page(root, &page, &stamp_checked(&text, today, sha.as_deref()))?;
                push_once(&mut touched, &page);
                run.stamped.push(page.clone());
            }
            continue;
        }

        // A real change, or the age rule: Claude re-reads the page, unless
        // an edit a run held for it still waits for a person.
        if held_open(db, &page)? {
            continue;
        }
        let Some(call) = reread.as_deref_mut() else {
            run.waiting.push(page.clone());
            continue;
        };
        let prompt = reread_prompt(&page, &text, &evidence(root, &sources, &changed), changed.is_empty());
        let section = match call(&prompt) {
            Ok(reply) if reply.trim().trim_end_matches('.').eq_ignore_ascii_case("CHECKED") => None,
            Ok(reply) => match crate::ingest::finish_section(&reply) {
                Ok(s) => s,
                Err(_) => {
                    run.waiting.push(page.clone());
                    continue;
                }
            },
            Err(_) => {
                run.waiting.push(page.clone());
                continue;
            }
        };
        let stamped = stamp_checked(&text, today, sha.as_deref());
        let Some(section) = section else {
            write_page(root, &page, &stamped)?;
            push_once(&mut touched, &page);
            run.stamped.push(page.clone());
            continue;
        };
        let proposed = stamp_checked(
            &crate::ingest::stamp_updated(&crate::ingest::replace_section(&text, &section), today),
            today,
            sha.as_deref(),
        );
        if proposed.replace("\r\n", "\n") == stamped.replace("\r\n", "\n") {
            write_page(root, &page, &stamped)?;
            push_once(&mut touched, &page);
            run.stamped.push(page.clone());
            continue;
        }
        // The stamps are Ken's own: the size rule measures the section alone.
        let on_disk = std::fs::read_to_string(root.join(&page)).unwrap_or_default();
        let base = crate::ingest::stamp_updated(&stamped, today);
        let held = if on_disk.replace("\r\n", "\n") != text.replace("\r\n", "\n") {
            Some(crate::ingest::Held::Changed)
        } else {
            crate::ingest::hold(&page, &base, &base, &proposed, "")
        };
        match held {
            None => {
                write_page(root, &page, &proposed)?;
                push_once(&mut touched, &page);
                run.fixed.push(page.clone());
            }
            Some(why) => {
                // Held against the page as it is now: the same section, put
                // into a person's newer page, applies without undoing them.
                let proposed = match why {
                    crate::ingest::Held::Changed => stamp_checked(
                        &crate::ingest::stamp_updated(&crate::ingest::replace_section(&on_disk, &section), today),
                        today,
                        sha.as_deref(),
                    ),
                    _ => proposed,
                };
                let p = crate::proposal::Proposal {
                    from: Some(HELD_FROM.to_string()),
                    ..crate::proposal::Proposal::new(page.clone(), on_disk, proposed)
                };
                let body = format!(
                    "Held from the drift run of {today}: {}. The code it cites changed; read the change and apply it, or \
                     discard it. It applies only while the page still reads as it does now.",
                    why.reason()
                );
                crate::proposal::file_page_proposal(db, &p, &format!("Held: {page} from drift"), &body, now)?;
                run.held.push(page.clone());
            }
        }
    }

    // One commit and one log line for what the run wrote (decisions 28, 36).
    if !touched.is_empty() {
        let title = format!(
            "{} checked, {} fixed, {} held, {} renames followed",
            run.stamped.len(),
            run.fixed.len(),
            run.held.len(),
            run.renamed.len()
        );
        let log = crate::ingest::append_log(root, today, "drift", &title)?;
        let mut paths: Vec<String> = touched.iter().filter(|p| !dirty.contains(p)).cloned().collect();
        if !dirty.contains(&log) {
            paths.push(log);
        }
        match crate::git::commit_paths(root, &paths, &commit_message(run, today)) {
            Ok(c) => run.commit = c,
            Err(e) => run.commit_error = Some(e),
        }
        crate::lint::queue_after_edit(db, &run.fixed)?;
        crate::graph::queue_topics(db, &run.fixed)?;
    }
    Ok(())
}

/// Whether an edit a drift run held for `page` still waits for a person:
/// the page is not read again until it is applied or discarded.
fn held_open(db: &Db, page: &str) -> Result<bool> {
    Ok(db.list_open_review_items()?.into_iter().any(|it| {
        it.kind == crate::proposal::PROPOSAL_KIND
            && it.source_ref == page
            && it
                .payload
                .as_deref()
                .and_then(|p| serde_json::from_str::<crate::proposal::Proposal>(p).ok())
                .is_some_and(|p| p.from.as_deref() == Some(HELD_FROM))
    }))
}

fn push_once(list: &mut Vec<String>, item: &str) {
    if !list.iter().any(|p| p == item) {
        list.push(item.to_string());
    }
}

fn write_page(root: &Path, page: &str, text: &str) -> Result<()> {
    let path = root.join(page);
    std::fs::write(&path, text).map_err(|e| crate::Error::io(&path, e))
}

/// A run's commit message: what it checked, fixed, held and followed, and
/// what waits.
pub fn commit_message(run: &DriftRun, today: &str) -> String {
    let mut s = format!("drift: the run of {today}\n");
    let mut list = |title: &str, items: &[String]| {
        if !items.is_empty() {
            s.push_str(&format!("\n{title}:\n"));
            for i in items {
                s.push_str(&format!("- {i}\n"));
            }
        }
    };
    list("Checked", &run.stamped);
    list("Fixed", &run.fixed);
    list("Held for a person", &run.held);
    list("Renames followed", &run.renamed);
    list("Waiting for a re-read", &run.waiting);
    if run.findings > 0 {
        s.push_str(&format!("\n{} findings filed.\n", run.findings));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn citations_are_code_cross_references_or_skipped() {
        assert!(matches!(classify("ken:src/scan.rs:42"), Citation::Code(CodeCitation { ref repo, ref path, line: Some(42), at: None }) if repo == "ken" && path == "src/scan.rs"));
        assert!(matches!(classify("ken@abc1234:src/lib.rs"), Citation::Code(CodeCitation { ref repo, line: None, ref at, .. }) if repo == "ken" && at.as_deref() == Some("abc1234")));
        assert!(matches!(classify("\"[[Standup - 2026-09-19]]\""), Citation::Cross));
        assert!(matches!(classify("D-012"), Citation::Cross));
        for skip in ["{{repo:path}}", "repo:path/.../x.rs", "https://example.com/a", "", "just words"] {
            assert!(matches!(classify(skip), Citation::Skip), "{skip}");
        }
    }

    #[test]
    fn rulings_parse_with_their_own_date_and_skip_the_format_example() {
        let text = "```\nD-001 · 2026-01-01 · topic — X\n  sources: repo:path:1\n```\nD-012 · 2026-09-01 · save — Worlds save as regions.\n  sources: game:src/save.rs:10, [[Save format]]\nD-013 · 2026-09-02 · old — gone [SUPERSEDED BY D-014]\n  sources: game:src/old.rs\n";
        let r = parse_rulings(text);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].id, "D-012");
        assert_eq!(r[0].date.as_deref(), Some("2026-09-01"));
        assert_eq!(r[0].sources, vec!["game:src/save.rs:10", "[[Save format]]"]);
        assert!(r[1].superseded);
    }

    #[test]
    fn dates_convert_without_a_date_crate() {
        assert_eq!(chrono_free_epoch("1970-01-01"), Some(0));
        assert_eq!(chrono_free_epoch("2026-09-24"), Some(1_790_208_000));
        assert_eq!(days_between("2026-08-24", 1_790_208_000), Some(31));
    }

    use std::process::Command;

    fn git_ok(dir: &Path, args: &[&str]) {
        let out = Command::new("git").args(args).current_dir(dir).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn commit_at(dir: &Path, file: &str, text: &str, date: &str) {
        fs::create_dir_all(dir.join(file).parent().unwrap()).unwrap();
        fs::write(dir.join(file), text).unwrap();
        git_ok(dir, &["add", "-A"]);
        let when = format!("{date}T12:00:00");
        let out = Command::new("git")
            .args(["commit", "-q", "-m", file])
            .env("GIT_AUTHOR_DATE", &when)
            .env("GIT_COMMITTER_DATE", &when)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    }

    /// A wiki drafted today: nothing in it may read as drift. Derived facts
    /// are not files, its own pages are cross-references, a path in the
    /// wrong case is the file it means, a folder without git is unmeasured,
    /// and an unchecked draft is not aged.
    #[test]
    fn a_fresh_draft_is_clean() {
        let parent = tempfile::tempdir().unwrap();
        let code = parent.path().join("app");
        fs::create_dir_all(&code).unwrap();
        git_ok(&code, &["init", "-q", "-b", "main"]);
        git_ok(&code, &["config", "user.email", "t@t"]);
        git_ok(&code, &["config", "user.name", "t"]);
        commit_at(&code, "README.md", "# App\n", "2026-09-01");
        let docs = parent.path().join("designs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("README.md"), "# Designs\n").unwrap();

        let wiki = parent.path().join("wiki");
        fs::create_dir_all(wiki.join("Repo-Map")).unwrap();
        fs::write(
            wiki.join("Repo-Map/app.md"),
            "---\nupdated: 2026-09-24\nsources:\n  - app:(layout)\n  - app:readme.md\n  - wiki:Repo-Map/designs.md\n  - designs:README.md\n---\n# App\n",
        )
        .unwrap();
        // A template to copy, with no date: not a page to age.
        fs::create_dir_all(wiki.join("Templates")).unwrap();
        fs::write(wiki.join("Templates/How-to.md"), "---
title: \"{{How to do the thing}}\"
updated: {{date}}
---
# {{How}}
").unwrap();
        let project = Project::create(&wiki, "wiki").unwrap();
        let mut db = Db::open_in_memory().unwrap();
        crate::scan::scan(&project, &mut db).unwrap();

        let run = sweep(&project, &db, chrono_free_epoch("2026-09-24").unwrap()).unwrap();
        assert!(run.mismatches.is_empty(), "{:?}", run.mismatches);
        assert!(run.aged.is_empty(), "{:?}", run.aged);
        assert_eq!(run.cross_references, 1, "the wiki's own page");
        assert!(run.unmeasured.iter().any(|u| u.contains("designs") && u.contains("not a git repository")), "{:?}", run.unmeasured);
        assert_eq!(run.exit_code, 0);

        // A draft pinned to the commit it read sees a change made the same
        // day; the date alone would take that commit in as already read.
        let read_at = {
            let out = Command::new("git").args(["rev-parse", "HEAD"]).current_dir(&code).output().unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        fs::write(
            wiki.join("Repo-Map/pinned.md"),
            format!("---\ntitle: Pinned\nupdated: 2026-09-24\nsources:\n  - app@{}:README.md\n---\n# P\n", &read_at[..12]),
        )
        .unwrap();
        // A page the check re-read at that commit pins to it in the same
        // way, through its `checked` stamp.
        fs::write(
            wiki.join("Repo-Map/checked.md"),
            format!("---\ntitle: Checked\nupdated: 2026-09-20\nchecked: 2026-09-24 {}\nsources:\n  - app:README.md\n---\n# C\n", &read_at[..12]),
        )
        .unwrap();
        commit_at(&code, "README.md", "# App\nNow with two approvals.\n", "2026-09-24");
        crate::scan::scan(&project, &mut db).unwrap();
        let run = sweep(&project, &db, chrono_free_epoch("2026-09-24").unwrap()).unwrap();
        let mut hits: Vec<_> = run.mismatches.iter().map(|m| (m.subject.as_str(), m.severity)).collect();
        hits.sort();
        assert_eq!(
            hits,
            vec![("Repo-Map/checked.md", Severity::Judgment), ("Repo-Map/pinned.md", Severity::Judgment)],
            "only the pinned pages: the dated one took today's commit in"
        );

        // Thirty-odd days on, still unchecked: now it is aged. A navigation
        // page with no sources carries no `checked:` and never is.
        fs::write(wiki.join("Repo-Map/Index.md"), "---\ntitle: Repo Map\nupdated: 2026-09-24\n---\n# Repo Map\n| [[app]] | the app |\n").unwrap();
        crate::scan::scan(&project, &mut db).unwrap();
        let later = sweep(&project, &db, chrono_free_epoch("2026-10-30").unwrap()).unwrap();
        assert_eq!(
            later.aged,
            vec![
                ("Repo-Map/app.md".to_string(), None),
                ("Repo-Map/checked.md".to_string(), Some("2026-09-24".to_string())),
                ("Repo-Map/pinned.md".to_string(), None),
            ]
        );
    }

    /// A code repo nobody fetches (Ken never syncs code): a change merged
    /// upstream after the draft is still found, because the sweep fetches.
    #[test]
    fn a_change_merged_upstream_is_found_without_a_manual_fetch() {
        let parent = tempfile::tempdir().unwrap();
        let upstream = parent.path().join("upstream.git");
        git_ok(parent.path(), &["init", "-q", "--bare", "-b", "main", upstream.to_str().unwrap()]);
        let seed = parent.path().join("seed");
        git_ok(parent.path(), &["clone", "-q", upstream.to_str().unwrap(), seed.to_str().unwrap()]);
        for (k, v) in [("user.email", "t@t"), ("user.name", "t")] {
            git_ok(&seed, &["config", k, v]);
        }
        commit_at(&seed, "README.md", "# App\n", "2026-09-01");
        git_ok(&seed, &["push", "-q", "origin", "main"]);
        let code = parent.path().join("app");
        git_ok(parent.path(), &["clone", "-q", upstream.to_str().unwrap(), code.to_str().unwrap()]);
        let read_at = {
            let out = Command::new("git").args(["rev-parse", "--short=12", "HEAD"]).current_dir(&code).output().unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        // Someone else merges a change; this clone never pulls or fetches.
        commit_at(&seed, "README.md", "# App\nNow with two approvals.\n", "2026-09-02");
        git_ok(&seed, &["push", "-q", "origin", "main"]);

        let wiki = parent.path().join("wiki");
        fs::create_dir_all(wiki.join("Repo-Map")).unwrap();
        fs::write(
            wiki.join("Repo-Map/app.md"),
            format!("---\ntitle: App\nupdated: 2026-09-01\nsources:\n  - app@{read_at}:README.md\n---\n# App\n"),
        )
        .unwrap();
        let project = Project::create(&wiki, "wiki").unwrap();
        let mut db = Db::open_in_memory().unwrap();
        crate::scan::scan(&project, &mut db).unwrap();
        let run = sweep(&project, &db, chrono_free_epoch("2026-09-03").unwrap()).unwrap();
        let hits: Vec<_> = run.mismatches.iter().map(|m| (m.subject.as_str(), m.severity)).collect();
        assert_eq!(hits, vec![("Repo-Map/app.md", Severity::Judgment)], "{:?} {:?}", run.mismatches, run.branches);
    }

    /// A repo checked out on a long-lived branch the default does not hold
    /// (a design branch): the wiki was read from it, so it is measured
    /// against it, and a change pushed to it is still found.
    #[test]
    fn a_checkout_on_another_branch_is_measured_against_that_branch() {
        let parent = tempfile::tempdir().unwrap();
        let upstream = parent.path().join("upstream.git");
        git_ok(parent.path(), &["init", "-q", "--bare", "-b", "main", upstream.to_str().unwrap()]);
        let seed = parent.path().join("seed");
        git_ok(parent.path(), &["clone", "-q", upstream.to_str().unwrap(), seed.to_str().unwrap()]);
        for (k, v) in [("user.email", "t@t"), ("user.name", "t")] {
            git_ok(&seed, &["config", k, v]);
        }
        commit_at(&seed, "README.md", "# App\n", "2026-09-01");
        git_ok(&seed, &["push", "-q", "origin", "main"]);
        git_ok(&seed, &["checkout", "-q", "-b", "design"]);
        commit_at(&seed, "docs/DEMO.md", "# Demo\nChicago only.\n", "2026-09-02");
        git_ok(&seed, &["push", "-q", "origin", "design"]);
        let code = parent.path().join("app");
        git_ok(parent.path(), &["clone", "-q", "-b", "design", upstream.to_str().unwrap(), code.to_str().unwrap()]);
        let read_at = {
            let out = Command::new("git").args(["rev-parse", "--short=12", "HEAD"]).current_dir(&code).output().unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };

        let wiki = parent.path().join("wiki");
        fs::create_dir_all(wiki.join("Repo-Map")).unwrap();
        fs::write(
            wiki.join("Repo-Map/app.md"),
            format!("---\ntitle: App\nupdated: 2026-09-02\nsources:\n  - app@{read_at}:docs/DEMO.md\n---\n# App\n"),
        )
        .unwrap();
        let project = Project::create(&wiki, "wiki").unwrap();
        let mut db = Db::open_in_memory().unwrap();
        crate::scan::scan(&project, &mut db).unwrap();
        let run = sweep(&project, &db, chrono_free_epoch("2026-09-03").unwrap()).unwrap();
        assert!(run.mismatches.is_empty(), "{:?}", run.mismatches);
        assert!(run.branches.iter().any(|b| b.contains("measured against origin/design")), "{:?}", run.branches);

        commit_at(&seed, "docs/DEMO.md", "# Demo\nChicago and Dallas.\n", "2026-09-03");
        git_ok(&seed, &["push", "-q", "origin", "design"]);
        crate::scan::scan(&project, &mut db).unwrap();
        let run = sweep(&project, &db, chrono_free_epoch("2026-09-04").unwrap()).unwrap();
        let hits: Vec<_> = run.mismatches.iter().map(|m| (m.subject.as_str(), m.severity)).collect();
        assert_eq!(hits, vec![("Repo-Map/app.md", Severity::Judgment)], "{:?} {:?}", run.mismatches, run.branches);
    }

    /// A wiki beside a code repo: a page whose cited code changed is a
    /// Judgment, a comment-only change clears itself, a deleted file and a
    /// line past the end are Findings, an old page is aged, Research is
    /// apart, every page with sources is measured alike, and the controls
    /// decide whether the run is trusted.
    #[test]
    fn a_sweep_sorts_what_moved_and_checks_its_controls() {
        let parent = tempfile::tempdir().unwrap();
        let code = parent.path().join("game");
        fs::create_dir_all(&code).unwrap();
        git_ok(&code, &["init", "-q", "-b", "main"]);
        git_ok(&code, &["config", "user.email", "t@t"]);
        git_ok(&code, &["config", "user.name", "t"]);
        git_ok(&code, &["config", "core.autocrlf", "false"]);
        commit_at(&code, "src/save.rs", "fn save() {\n    write();\n}\n", "2026-09-01");
        commit_at(&code, "src/combat.rs", "fn hit() {}\n", "2026-09-01");
        commit_at(&code, "src/gone.rs", "fn x() {}\n", "2026-09-01");
        // After the pins: a real change, a comment-only change, a deletion.
        commit_at(&code, "src/save.rs", "fn save() {\n    write_all();\n}\n", "2026-09-10");
        commit_at(&code, "src/combat.rs", "// hits\nfn hit() {}\n", "2026-09-10");
        fs::remove_file(code.join("src/gone.rs")).unwrap();
        git_ok(&code, &["add", "-A"]);
        git_ok(&code, &["commit", "-q", "-m", "rm"]);

        let wiki = parent.path().join("wiki");
        fs::create_dir_all(wiki.join("Platform")).unwrap();
        fs::create_dir_all(wiki.join("Research")).unwrap();
        let page = |sources: &str, checked: &str| format!("---\nchecked: {checked}\nsources:\n{sources}---\n# P\n");
        fs::write(wiki.join("Platform/Save.md"), page("  - game:src/save.rs:2\n", "2026-09-05")).unwrap();
        // A Current page citing the same changed code is measured like any
        // other (decision 28 drops the audience split).
        fs::create_dir_all(wiki.join("Current")).unwrap();
        fs::write(wiki.join("Current/Project.md"), page("  - game:src/save.rs:2\n", "2026-09-05")).unwrap();
        fs::write(wiki.join("Platform/Combat.md"), page("  - game:src/combat.rs\n  - \"[[Save]]\"\n", "2026-09-05")).unwrap();
        fs::write(wiki.join("Platform/Old.md"), page("  - game:src/gone.rs\n  - game:src/save.rs:99\n", "2026-08-01")).unwrap();
        fs::write(wiki.join("Research/Spike.md"), page("  - game:src/save.rs\n", "2026-09-05")).unwrap();
        let project = Project::create(&wiki, "wiki").unwrap();
        settings(&wiki, serde_json::json!({"controlMoved": "Platform/Save.md", "controlClean": "Platform/Combat.md", "minPages": 2}));
        let mut db = Db::open_in_memory().unwrap();
        crate::scan::scan(&project, &mut db).unwrap();

        let now = chrono_free_epoch("2026-09-24").unwrap();
        let run = sweep(&project, &db, now).unwrap();
        let sev = |subject: &str| {
            let mut v: Vec<Severity> = run.mismatches.iter().filter(|m| m.subject == subject).map(|m| m.severity).collect();
            v.sort();
            v
        };
        assert_eq!(sev("Platform/Save.md"), vec![Severity::Judgment]);
        assert_eq!(sev("Platform/Combat.md"), vec![Severity::AutoRecleared]);
        assert_eq!(sev("Platform/Old.md"), vec![Severity::Finding, Severity::Finding], "{:?}", run.mismatches);
        assert!(run.mismatches.iter().any(|m| m.subject == "Research/Spike.md" && m.research));
        assert_eq!(run.aged, vec![("Platform/Old.md".to_string(), Some("2026-08-01".to_string()))]);
        assert_eq!(run.cross_references, 1);
        assert_eq!(sev("Current/Project.md"), vec![Severity::Judgment], "no page is spared its code");
        assert_eq!(run.void_reason, None, "controls held: {:?}", run.void_reason);
        assert!(!run.uncontrolled);
        assert_eq!(run.exit_code, 1, "a live Finding");
        assert!(run.branches.iter().any(|b| b.contains("measured against main")), "{:?}", run.branches);
        assert_eq!((run.measured, run.reused), (5, 1), "the first sweep measures every code citation, a repeated one once");

        // Nothing committed since: every result carries over, the same as a
        // full sweep, with one git call per repo to see nothing moved.
        let again = sweep(&project, &db, now).unwrap();
        assert_eq!(again.mismatches, run.mismatches);
        assert_eq!(again.unmeasured, run.unmeasured);
        assert_eq!((again.measured, again.reused), (0, 6));

        // A commit touching one file re-measures only the citations of it.
        commit_at(&code, "src/combat.rs", "// hits\nfn hit() { crit(); }\n", "2026-09-20");
        let after = sweep(&project, &db, now).unwrap();
        assert_eq!((after.measured, after.reused), (1, 5));
        assert!(after
            .mismatches
            .iter()
            .any(|m| m.subject == "Platform/Combat.md" && m.severity == Severity::Judgment));
        assert!(after.void_reason.as_deref().is_some_and(|v| v.contains("known-clean")), "the clean control moved");

        // A known-clean control that is flagged voids the run.
        settings(&wiki, serde_json::json!({"controlMoved": "Platform/Save.md", "controlClean": "Platform/Old.md"}));
        let void = sweep(&project, &db, now).unwrap();
        assert!(void.void_reason.as_deref().unwrap().contains("known-clean"));

        // No controls: says so. Filing keeps a record, and no Review item.
        settings(&wiki, serde_json::json!({}));
        let plain = sweep(&project, &db, now).unwrap();
        assert!(plain.uncontrolled);
        db.insert_review_item(REVIEW_KIND, "Drift: from before", "body", "", None, now - 1).unwrap();
        file_run(&mut db, &plain).unwrap();
        file_run(&mut db, &plain).unwrap();
        assert!(db.open_review_item_of_kind(REVIEW_KIND).unwrap().is_none(), "an old item is resolved");
        assert_eq!(db.last_drift_run_at().unwrap(), Some(now));

        // The next run: a week on for the age rule, else when the code moves.
        assert!(!due(&db, now + 86_400).unwrap(), "nothing moved");
        assert!(due(&db, now + 8 * 86_400).unwrap(), "the age rule's week");
        commit_at(&code, "src/save.rs", "fn save() {\n    write_some();\n}\n", "2026-09-21");
        assert!(due(&db, now + 60).unwrap(), "the branch moved");
    }

    /// The settings a knowledge base keeps in `.ken/knowledge.json`, with
    /// `drift` set (one config, decision 28).
    fn settings(wiki: &Path, drift: serde_json::Value) {
        let all = serde_json::json!({ "drift": drift });
        fs::create_dir_all(wiki.join(".ken")).unwrap();
        fs::write(wiki.join(crate::wikinew::KNOWLEDGE_FILE), all.to_string()).unwrap();
        assert_eq!(DriftConfig::for_repo(wiki), DriftConfig::from_settings(&all));
    }

    #[test]
    fn the_config_is_read_from_the_knowledge_settings() {
        let cfg = DriftConfig::from_settings(&serde_json::json!({
            "drift": { "controlMoved": "A.md", "controlClean": "B.md", "minPages": 3, "fetch": false, "intervalDays": 3 }
        }));
        assert_eq!(cfg.control_moved.as_deref(), Some("A.md"));
        assert_eq!(cfg.control_clean.as_deref(), Some("B.md"));
        assert_eq!((cfg.min_pages, cfg.fetch), (Some(3), Some(false)), "an old intervalDays is ignored");
        assert_eq!(DriftConfig::from_settings(&serde_json::json!({})), DriftConfig::default());
    }

    /// A run acts on what it found (decision 28): a comment-only change is
    /// stamped silently, a real change is re-read and fixed, a Rules page's
    /// fix is held, a ruling whose code changed is a finding, a renamed file
    /// is followed, and what was written is one commit with one log line.
    /// Without Claude the re-reads wait.
    #[test]
    fn a_run_stamps_fixes_holds_follows_and_commits() {
        let parent = tempfile::tempdir().unwrap();
        let code = parent.path().join("game");
        fs::create_dir_all(&code).unwrap();
        git_ok(&code, &["init", "-q", "-b", "main"]);
        for (k, v) in [("user.email", "t@t"), ("user.name", "t"), ("core.autocrlf", "false")] {
            git_ok(&code, &["config", k, v]);
        }
        commit_at(&code, "src/save.rs", "fn save() {\n    write();\n}\n", "2026-09-01");
        commit_at(&code, "src/combat.rs", "fn hit() {}\n", "2026-09-01");
        commit_at(&code, "src/old_name.rs", "fn x() {}\n", "2026-09-01");
        commit_at(&code, "src/rules.rs", "fn rule() {}\n", "2026-09-01");
        commit_at(&code, "src/save.rs", "fn save() {\n    write_all();\n}\n", "2026-09-10");
        commit_at(&code, "src/combat.rs", "// hits\nfn hit() {}\n", "2026-09-10");
        commit_at(&code, "src/rules.rs", "fn rule() { strict(); }\n", "2026-09-10");
        git_ok(&code, &["mv", "src/old_name.rs", "src/new_name.rs"]);
        commit_at(&code, "src/other.rs", "fn other() {}\n", "2026-09-11");

        let wiki = parent.path().join("wiki");
        let page = |source: &str, body: &str| {
            format!(
                "---\ntitle: P\nchecked: 2026-09-05\nsources:\n  - {source}\n---\n# P\n\nWhat this covers.\n\n## Saving\n\n{body}\n\n\
                 ## More\n\nOne.\nTwo.\nThree.\nFour.\n"
            )
        };
        let log = "# Decisions\n\n## D-012 · 2026-09-05 · rules\n\n**Rules stay loose.**\n\nsources: game:src/rules.rs\n";
        for (path, text) in [
            ("Platform/Save.md", page("game:src/save.rs:2", "Saves call write().")),
            ("Platform/Combat.md", page("game:src/combat.rs", "Hits land.")),
            ("Platform/Moved.md", page("game:src/old_name.rs", "X lives in old_name.")),
            ("Rules/Strict.md", page("game:src/rules.rs", "Rules are loose.")),
            ("Decisions/DECISIONS.md", log.to_string()),
        ] {
            fs::create_dir_all(wiki.join(path).parent().unwrap()).unwrap();
            fs::write(wiki.join(path), text).unwrap();
        }
        let project = Project::create(&wiki, "wiki").unwrap();
        settings(&wiki, serde_json::json!({ "fetch": false }));
        git_ok(&wiki, &["init", "-q", "-b", "main"]);
        for (k, v) in [("user.email", "t@t"), ("user.name", "t"), ("core.autocrlf", "false")] {
            git_ok(&wiki, &["config", k, v]);
        }
        git_ok(&wiki, &["add", "-A"]);
        git_ok(&wiki, &["commit", "-q", "-m", "wiki"]);
        let mut db = Db::open_in_memory().unwrap();
        crate::scan::scan(&project, &mut db).unwrap();

        let mut asked: Vec<String> = Vec::new();
        let mut reread = |p: &str| -> Result<String> {
            asked.push(p.lines().next().unwrap_or("").to_string());
            Ok(if p.contains("`Platform/Save.md`") {
                assert!(p.contains("write_all"), "the diff is in the prompt");
                "## Saving\n\nSaves call write_all().\n".into()
            } else if p.contains("`Rules/Strict.md`") {
                "## Saving\n\nRules are strict.\n".into()
            } else {
                "CHECKED".into()
            })
        };
        let now = chrono_free_epoch("2026-09-24").unwrap();
        let r = run(&project, &mut db, now, "2026-09-24", Some(&mut reread as &mut Reread<'_>)).unwrap();
        assert_eq!(asked.len(), 2, "only the real changes are read: {asked:?}");
        assert_eq!(r.stamped, vec!["Platform/Combat.md".to_string()], "the comment-only change, silently");
        assert_eq!(r.fixed, vec!["Platform/Save.md".to_string()]);
        assert_eq!(r.held, vec!["Rules/Strict.md".to_string()], "a Rules page waits for a person");
        assert_eq!(r.renamed.len(), 1, "{:?}", r.renamed);
        assert!(r.waiting.is_empty(), "{:?}", r.waiting);

        let read = |p: &str| fs::read_to_string(wiki.join(p)).unwrap();
        assert!(read("Platform/Save.md").contains("Saves call write_all()") && read("Platform/Save.md").contains("checked: 2026-09-24 "));
        assert!(read("Platform/Save.md").contains("## More\n\nOne."), "only the one section changed");
        assert!(read("Platform/Combat.md").contains("checked: 2026-09-24 ") && read("Platform/Combat.md").contains("Hits land."));
        assert!(read("Platform/Moved.md").contains("game:src/new_name.rs"), "the citation follows the rename");
        assert!(read("Reference/Vocabulary.md").contains("old_name") && read("Reference/Vocabulary.md").contains("new_name"));
        assert!(read("Rules/Strict.md").contains("Rules are loose."), "held, not written");
        assert!(db.open_review_item_of_kind(crate::proposal::PROPOSAL_KIND).unwrap().is_some(), "the held edit waits");
        let found = crate::findings::open(&db).unwrap();
        assert!(
            found.iter().any(|(_, f)| f.kind == "drift" && f.against == "D-012" && f.against_kind == "ruling"),
            "the ruling is a finding: {found:?}"
        );
        assert!(read("Decisions/DECISIONS.md").contains("**Rules stay loose.**"), "a ruling is never edited");

        // One commit, with one log line: only what the run wrote.
        assert!(r.commit.is_some(), "{:?}", r.commit_error);
        let shown = Command::new("git").args(["show", "--name-only", "--format=%s", "HEAD"]).current_dir(&wiki).output().unwrap();
        let shown = String::from_utf8_lossy(&shown.stdout).to_string();
        for f in ["log.md", "Platform/Save.md", "Platform/Combat.md", "Platform/Moved.md", "Reference/Vocabulary.md"] {
            assert!(shown.contains(f), "{f} in {shown}");
        }
        assert!(shown.starts_with("drift: the run of 2026-09-24") && !shown.contains("Rules/Strict.md"), "{shown}");
        assert!(read("log.md").contains("## [2026-09-24] drift | 1 checked, 1 fixed, 1 held, 1 renames followed"));
        assert_eq!(crate::lint::queued(&db).unwrap(), vec!["Platform/Save.md".to_string()], "the fix waits for the lint");

        // The code moves again; without Claude the re-read waits, and the
        // next run is due an hour on.
        commit_at(&code, "src/save.rs", "fn save() {\n    write_some();\n}\n", "2026-09-25");
        crate::scan::scan(&project, &mut db).unwrap();
        let later = chrono_free_epoch("2026-09-26").unwrap();
        assert!(due(&db, later).unwrap(), "the branch moved");
        let r = run(&project, &mut db, later, "2026-09-26", None).unwrap();
        assert!(r.waiting.contains(&"Platform/Save.md".to_string()), "{:?}", r.waiting);
        assert!(!r.waiting.contains(&"Rules/Strict.md".to_string()), "its held edit still waits: not read again");
        assert!(r.commit.is_none(), "nothing written, nothing committed");
        assert!(!due(&db, later + 60).unwrap(), "waits for the hour");
        assert!(due(&db, later + RETRY_SECS).unwrap());
    }

    #[test]
    fn a_bold_log_reads_like_a_plain_one() {
        let log = "**D-410** · 2026-10-06 · anchors — **WRITTEN BY THE TOOLS.**\n  sources: game:src/a.rs:3\n\n\
                   - **D-379** · 2026-10-05 · anchors — **THE SERVER NEVER OVERWRITES.** [SUPERSEDED BY D-410]\n";
        let rs = parse_rulings(log);
        assert_eq!(rs.len(), 2, "{rs:?}");
        assert_eq!((rs[0].id.as_str(), rs[0].date.as_deref()), ("D-410", Some("2026-10-06")));
        assert_eq!(rs[0].sources, vec!["game:src/a.rs:3".to_string()]);
        assert!(!rs[0].superseded && rs[1].superseded && rs[1].id == "D-379");
    }
}
