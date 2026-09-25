//! The first wiki, drafted from an analysis at set-up (knowledge-layer item
//! 4b). Offered, never forced: Claude reads the repos (READMEs, briefs, top
//! docs, layout, markers, who commits), and any folder of documents a person
//! adds (a Confluence export, say), and drafts the Current pages, the
//! architecture page and a first business doc, the release notes, for a
//! person to review.
//!
//! Three rules keep it honest:
//! - A page a person wrote is never touched. A page is drafted only when it
//!   is missing or is still an untouched template (it has `{{…}}` left).
//! - Every drafted page is `status: draft` with no `verified:` date, so the
//!   age rule keeps raising it until someone reads it against its sources.
//! - Every page names the sources it came from, in its frontmatter and its
//!   first line, as `repo:path` locators.
//!
//! The model call is passed in (`generate`), as for ingest.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::{Error, Result};

pub const REVIEW_KIND: &str = "wiki-draft";
/// Characters of source text given to the model per page, all sources
/// together, so a large folder cannot overflow the prompt.
const SOURCE_BUDGET: usize = 120_000;
const PER_FILE: usize = 8_000;

/// One thing read for the draft, labelled the way a page cites it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// `repo:path`, or `repo:(layout)` / `repo:(git authors)` for derived facts.
    pub label: String,
    pub text: String,
}

/// The pages drafted, in order: (path in the wiki, what the page is for).
pub const PAGES: &[(&str, &str)] = &[
    (
        "Current/Project.md",
        "what the project is and does, for someone who never reads code: the product, who it is for, the goal and the next milestone",
    ),
    ("Current/Team.md", "how the team is structured: roles, who decides what, how work is handed off"),
    ("Current/Who-Does-What.md", "who owns each area and repo, and who to ask, from who commits where and any CODEOWNERS"),
    (
        "Conventions/ARCHITECTURE.md",
        "the system's layers: what lives where, what may call what, the entry points, with a mermaid diagram",
    ),
    (
        "Work/Releases.md",
        "a business doc (item 2b): what changed in each release, newest first, in words for someone who never reads code, from changelogs and release tags",
    ),
];

fn clip(s: &str, n: usize) -> String {
    if s.len() <= n {
        return s.to_string();
    }
    let mut end = n;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[… clipped]", &s[..end])
}

fn read_text(path: &Path) -> Option<String> {
    let t = crate::extract::extract(path).ok()?.text;
    (!t.trim().is_empty()).then_some(t)
}

/// The repo's top two levels, folders first, skipping dotfiles and
/// dependency folders: enough to see its shape.
fn layout(root: &Path) -> String {
    let mut out = String::new();
    let list = |dir: &Path| -> Vec<(String, bool)> {
        let mut v: Vec<(String, bool)> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let is_dir = e.path().is_dir();
                (!name.starts_with('.') && !(is_dir && crate::scan::is_junk_dir_name(&name))).then_some((name, is_dir))
            })
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    };
    for (name, is_dir) in list(root).into_iter().take(60) {
        out.push_str(&format!("{name}{}\n", if is_dir { "/" } else { "" }));
        if is_dir {
            for (child, cdir) in list(&root.join(&name)).into_iter().take(15) {
                out.push_str(&format!("  {child}{}\n", if cdir { "/" } else { "" }));
            }
        }
    }
    out
}

fn git_tags(root: &Path) -> Option<String> {
    let mut cmd = Command::new("git");
    let out = crate::proc::quiet(&mut cmd)
        .args(["tag", "-n3", "--sort=-creatordate"])
        .current_dir(root)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let top: Vec<&str> = text.lines().take(40).collect();
    (out.status.success() && !top.is_empty()).then(|| top.join("\n"))
}

fn git_authors(root: &Path) -> Option<String> {
    let mut cmd = Command::new("git");
    let out = crate::proc::quiet(&mut cmd).args(["shortlog", "-sne", "--all", "--no-merges"]).current_dir(root).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let top: Vec<&str> = text.lines().take(12).collect();
    (out.status.success() && !top.is_empty()).then(|| top.join("\n"))
}

//// Characters of source text read from one repo for its Repo Map page, so
/// every repo gets its own budget however many the team has.
const REPO_BUDGET: usize = 60_000;

fn pusher(budget: usize) -> impl Fn(String, String, &mut Vec<Source>) {
    move |label: String, text: String, out: &mut Vec<Source>| {
        let used: usize = out.iter().map(|s| s.text.len()).sum();
        if used < budget {
            out.push(Source { label, text: clip(&text, PER_FILE.min(budget - used)) });
        }
    }
}

/// What one repo's page is made from: its description first, its READMEs
/// and briefs, up to ten top-level docs, its layout, CODEOWNERS, who
/// commits and its release tags. Clipped to [`REPO_BUDGET`].
pub fn gather_repo(name: &str, root: &Path) -> Vec<Source> {
    let mut out: Vec<Source> = Vec::new();
    let push = pusher(REPO_BUDGET);
    // What a person said this repo is for, at set-up: read first, so the
    // draft knows how to use each source.
    if let Some(d) = crate::registry::description_of(root) {
        push(format!("{name}:(what this repo is for, in the team's words)"), d, &mut out);
    }
    for f in ["README.md", "readme.md", "README", "CLAUDE.md", "START-HERE.md", "AGENTS.md", "CHANGELOG.md", "RELEASES.md"] {
        if let Some(t) = read_text(&root.join(f)) {
            push(format!("{name}:{f}"), t, &mut out);
        }
    }
    for dir in ["docs", "doc", "documentation"] {
        let mut docs: Vec<PathBuf> = fs::read_dir(root.join(dir))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect();
        docs.sort();
        for p in docs.into_iter().take(10) {
            if let Some(t) = read_text(&p) {
                let rel = format!("{dir}/{}", p.file_name().unwrap().to_string_lossy());
                push(format!("{name}:{rel}"), t, &mut out);
            }
        }
    }
    push(format!("{name}:(layout)"), layout(root), &mut out);
    for f in ["CODEOWNERS", ".github/CODEOWNERS", "docs/CODEOWNERS"] {
        if let Ok(t) = fs::read_to_string(root.join(f)) {
            push(format!("{name}:{f}"), t, &mut out);
        }
    }
    if let Some(a) = git_authors(root) {
        push(format!("{name}:(git authors, commits · name)"), a, &mut out);
    }
    if let Some(t) = git_tags(root) {
        push(format!("{name}:(release tags, newest first)"), t, &mut out);
    }
    out
}

/// Every readable document in a folder a person added (a Confluence
/// export, say), clipped to [`SOURCE_BUDGET`].
pub fn gather_extra(dir: &Path) -> Vec<Source> {
    let mut out: Vec<Source> = Vec::new();
    let push = pusher(SOURCE_BUDGET);
    let label = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "documents".into());
    let walker = ignore::WalkBuilder::new(dir).hidden(true).build().flatten();
    for e in walker.filter(|e| e.path().is_file()).take(200) {
        if let Some(t) = read_text(e.path()) {
            let rel = e.path().strip_prefix(dir).unwrap_or(e.path()).to_string_lossy().replace('\\', "/");
            push(format!("{label}:{rel}"), t, &mut out);
        }
    }
    out
}

/// Every repo's sources, each within its own budget, then `extra`.
pub fn gather(repos: &[(String, PathBuf)], extra: Option<&Path>) -> Vec<Source> {
    let mut out: Vec<Source> = repos.iter().flat_map(|(n, r)| gather_repo(n, r)).collect();
    if let Some(dir) = extra {
        out.extend(gather_extra(dir));
    }
    out
}

// A page Ken may write: missing, or a template nobody has filled in.
pub fn may_draft(existing: Option<&str>) -> bool {
    existing.is_none_or(|t| t.contains("{{"))
}

pub fn prompt(page: &str, purpose: &str, template: Option<&str>, sources: &[Source], today: &str) -> String {
    let mut s = format!(
        "You are drafting one page of a team's wiki, `{page}`, for a person to review. The page is: {purpose}.\n\n\
         Rules:\n\
         - Use only what the sources below say. Where they say nothing, write `(not in the sources yet)` rather than guess.\n\
         - Cite the source of every fact inline, as its label in backticks, e.g. `ken:README.md`.\n\
         - Frontmatter: keep the template's keys; set `status: draft`; set `updated: {today}`; do NOT write a `verified:` line (a person verifies it later); list every source you used under `sources:` as its label.\n\
         - The first line under the title says: Drafted by Ken on {today} from the sources listed; not yet verified by a person.\n\
         - Plain words for a reader who has not seen the code. Tables where the template has them.\n\
         - A source labelled `(what this repo is for, in the team's words)` is the team's own description of that repo: use it to know what each repo is and how it is used.\n\
         - Reply with the finished page only, in Markdown, starting with `---`. No preamble, no code fences around it.\n\n"
    );
    match template {
        Some(t) => s.push_str(&format!("TEMPLATE (fill it in, replacing every {{{{…}}}}):\n{t}\n\n")),
        None => s.push_str("There is no template; use frontmatter with title, status, updated and sources, then sections that fit the page.\n\n"),
    }
    s.push_str("SOURCES:\n");
    for src in sources {
        s.push_str(&format!("\n=== {} ===\n{}\n", src.label, src.text));
    }
    s
}

/// The reply as a page: fences stripped, `status: draft` forced, any
/// `verified:` line dropped (only a person sets it).
pub fn finish(reply: &str) -> Result<String> {
    let mut t = reply.trim();
    if let Some(rest) = t.strip_prefix("```markdown").or_else(|| t.strip_prefix("```md")).or_else(|| t.strip_prefix("```")) {
        t = rest.trim_start().strip_suffix("```").unwrap_or(rest).trim();
    }
    if !t.starts_with("---") {
        return Err(Error::Other("the draft did not start with its frontmatter".into()));
    }
    let mut out = String::new();
    let (mut in_fm, mut closed, mut has_status) = (false, false, false);
    for (i, line) in t.lines().enumerate() {
        let trimmed = line.trim_start();
        if line.trim_end() == "---" {
            if i == 0 {
                in_fm = true;
            } else if in_fm && !closed {
                if !has_status {
                    out.push_str("status: draft\n");
                }
                in_fm = false;
                closed = true;
            }
            out.push_str("---\n");
            continue;
        }
        if in_fm && trimmed.starts_with("verified:") {
            continue;
        }
        if in_fm && trimmed.starts_with("status:") {
            out.push_str("status: draft\n");
            has_status = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if !closed {
        return Err(Error::Other("the draft's frontmatter was never closed".into()));
    }
    Ok(out)
}

//// What a draft run did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftReport {
    pub drafted: Vec<String>,
    /// Pages a person already wrote, left alone.
    pub kept: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub sources: Vec<String>,
    /// Changes to pages a person keeps, filed for them to apply or discard.
    #[serde(default)]
    pub proposed: Vec<String>,
}

fn write_page(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    fs::write(path, text).map_err(|e| Error::io(path, e))
}

/// Draft each of `pages` the wiki does not already have into `report`.
fn draft_pages(
    wiki: &Path,
    pages: &[(String, String)],
    sources: &[Source],
    today: &str,
    report: &mut DraftReport,
    generate: &mut impl FnMut(&str) -> Result<String>,
) -> Result<()> {
    for (page, purpose) in pages {
        let path = wiki.join(page);
        let existing = fs::read_to_string(&path).ok();
        if !may_draft(existing.as_deref()) {
            report.kept.push(page.clone());
            continue;
        }
        match generate(&prompt(page, purpose, existing.as_deref(), sources, today)).and_then(|r| finish(&r)) {
            Ok(text) => {
                write_page(&path, &text)?;
                report.drafted.push(page.clone());
            }
            Err(e) => report.failed.push((page.clone(), e.to_string())),
        }
    }
    Ok(())
}

fn file_card(db: &mut Db, report: &DraftReport, title: &str, now: i64) -> Result<()> {
    let mut body = String::new();
    if !report.drafted.is_empty() {
        body.push_str("**Drafted for you to read against their sources** (each is `status: draft`, not verified):\n");
        for p in &report.drafted {
            body.push_str(&format!("- {p}\n"));
        }
    }
    if !report.proposed.is_empty() {
        body.push_str("\n**Changes proposed** to pages a person keeps, each on its own card to apply or discard:\n");
        for p in &report.proposed {
            body.push_str(&format!("- {p}\n"));
        }
    }
    if !report.kept.is_empty() {
        body.push_str("\n**Left alone** (a person already wrote them):\n");
        for p in &report.kept {
            body.push_str(&format!("- {p}\n"));
        }
    }
    if !report.failed.is_empty() {
        body.push_str("\n**Could not draft:**\n");
        for (p, e) in &report.failed {
            body.push_str(&format!("- {p}: {e}\n"));
        }
    }
    body.push_str(&format!("\nRead from {} sources: {}\n", report.sources.len(), report.sources.join(", ")));
    let first = report.drafted.first().cloned().unwrap_or_default();
    db.insert_review_item(REVIEW_KIND, title, &body, &first, None, now)?;
    Ok(())
}

fn team_pages() -> Vec<(String, String)> {
    PAGES.iter().map(|(p, u)| (p.to_string(), u.to_string())).collect()
}

/// Draft every page of [`PAGES`] the wiki does not already have, then file
/// one Review card listing what was drafted and kept.
pub fn draft(
    wiki: &Path,
    db: &mut Db,
    sources: &[Source],
    today: &str,
    now: i64,
    mut generate: impl FnMut(&str) -> Result<String>,
) -> Result<DraftReport> {
    let mut report = DraftReport { sources: sources.iter().map(|s| s.label.clone()).collect(), ..Default::default() };
    draft_pages(wiki, &team_pages(), sources, today, &mut report, &mut generate)?;
    let title = format!("First wiki drafted: {} pages to read", report.drafted.len());
    file_card(db, &report, &title, now)?;
    Ok(report)
}

// --- The wiki at team scale: a Repo Map page per repo, then the team pages
// drafted from those pages, so twenty repos never overflow one prompt. ----

/// The section for what lives where in the code (the template's `Repo-Map/`).
pub const REPO_MAP: &str = "Repo-Map";

const REPO_PURPOSE: &str = "one repo's page in the Repo Map: what the repo is for (the team's own description first), \
    what lives where in it (its layout and entry points), who owns it and who commits, how it is built and released, \
    and how it connects to the team's other repos";

/// A repo's page in the wiki: `Repo-Map/<name>.md`.
pub fn repo_page(name: &str) -> String {
    format!("{REPO_MAP}/{}.md", crate::workspace::member_leaf(name))
}

/// The Repo Map's index: a table of every repo page with what it is for.
/// Written by Ken, not the model: it is a list, and a list has no facts to
/// get wrong.
pub fn repo_map_index(repos: &[(String, String)], today: &str) -> String {
    let mut s = format!(
        "---\ntitle: \"Repo Map\"\naliases: [\"Repo-Map\", \"what lives where\"]\ntags: [moc]\nupdated: {today}\n---\n\n\
         # Repo Map\n\nWhat lives where in the code, one page per repo. Open the repo's page before searching its files.\n\n\
         | repo | what it is for |\n|---|---|\n"
    );
    for (name, description) in repos {
        s.push_str(&index_row(name, description));
    }
    s
}

fn index_row(name: &str, description: &str) -> String {
    let leaf = crate::workspace::member_leaf(name);
    let d = description.split_whitespace().collect::<Vec<_>>().join(" ").replace('|', "/");
    format!("| [[{leaf}]] | {} |\n", if d.is_empty() { "(not said yet)" } else { &d })
}

/// The repo pages and descriptions the team pages are drafted from, each
/// repo's share of [`SOURCE_BUDGET`] equal, then `extra`.
pub fn team_sources(wiki: &Path, wiki_name: &str, repos: &[(String, PathBuf)], extra: &[Source]) -> Vec<Source> {
    let share = (SOURCE_BUDGET * 2 / 3) / repos.len().max(1);
    let mut out = Vec::new();
    for (name, root) in repos {
        if let Some(d) = crate::registry::description_of(root) {
            out.push(Source { label: format!("{name}:(what this repo is for, in the team's words)"), text: clip(&d, 2_000) });
        }
        let page = repo_page(name);
        if let Some(t) = read_text(&wiki.join(&page)) {
            out.push(Source { label: format!("{wiki_name}:{page}"), text: clip(&t, share) });
        }
        // The release notes read changelogs and tags, which a repo page
        // summarises away.
        for f in ["CHANGELOG.md", "RELEASES.md"] {
            if let Some(t) = read_text(&root.join(f)) {
                out.push(Source { label: format!("{name}:{f}"), text: clip(&t, 3_000) });
            }
        }
        if let Some(t) = git_tags(root) {
            out.push(Source { label: format!("{name}:(release tags, newest first)"), text: clip(&t, 2_000) });
        }
    }
    let used: usize = out.iter().map(|s| s.text.len()).sum();
    let mut left = SOURCE_BUDGET.saturating_sub(used);
    for s in extra {
        if left == 0 {
            break;
        }
        let text = clip(&s.text, left.min(PER_FILE));
        left = left.saturating_sub(text.len());
        out.push(Source { label: s.label.clone(), text });
    }
    out
}

/// Pass one: each repo's Repo Map page, from that repo alone.
fn draft_repo_pages(
    wiki: &Path,
    repos: &[(String, PathBuf)],
    today: &str,
    report: &mut DraftReport,
    generate: &mut impl FnMut(&str) -> Result<String>,
) -> Result<()> {
    for (name, root) in repos {
        let sources = gather_repo(name, root);
        report.sources.extend(sources.iter().map(|s| s.label.clone()));
        draft_pages(wiki, &[(repo_page(name), REPO_PURPOSE.to_string())], &sources, today, report, generate)?;
    }
    Ok(())
}

/// Draft a team's wiki: a Repo Map page per repo (each from that repo
/// alone), the Repo Map index, then the team pages of [`PAGES`] from the
/// repo pages and descriptions. `repos` are the team's repos, the wiki
/// itself left out. One Review card lists it all.
#[allow(clippy::too_many_arguments)]
pub fn draft_team(
    wiki: &Path,
    wiki_name: &str,
    db: &mut Db,
    repos: &[(String, PathBuf)],
    extra: Option<&Path>,
    today: &str,
    now: i64,
    mut generate: impl FnMut(&str) -> Result<String>,
) -> Result<DraftReport> {
    let mut report = DraftReport::default();
    draft_repo_pages(wiki, repos, today, &mut report, &mut generate)?;
    let index = format!("{REPO_MAP}/Index.md");
    let index_path = wiki.join(&index);
    if may_draft(fs::read_to_string(&index_path).ok().as_deref()) {
        let listed: Vec<(String, String)> =
            repos.iter().map(|(n, r)| (n.clone(), crate::registry::description_of(r).unwrap_or_default())).collect();
        write_page(&index_path, &repo_map_index(&listed, today))?;
        report.drafted.push(index);
    }
    let extra = extra.map(gather_extra).unwrap_or_default();
    let sources = team_sources(wiki, wiki_name, repos, &extra);
    report.sources.extend(sources.iter().map(|s| s.label.clone()));
    report.sources.dedup();
    draft_pages(wiki, &team_pages(), &sources, today, &mut report, &mut generate)?;
    let title = format!("First wiki drafted: {} pages to read", report.drafted.len());
    file_card(db, &report, &title, now)?;
    Ok(report)
}

// --- A repo added later: its page is new, so Ken drafts it; the pages a
// person already keeps get a proposed change, never a write. -------------

/// Review kind of a proposed change to one page.
pub const PROPOSAL_KIND: &str = "page-proposal";

/// A change to one page, held until a person applies it. `base` is the page
/// as Ken read it; the change applies only while the page still reads so.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub page: String,
    pub base: String,
    pub proposed: String,
    /// The repo the page is in, when not the one whose Review holds the card
    /// (a ticket in the team repo, proposed from a note in the wiki).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// A decisions-log entry to append, worked out against the log as it is
    /// when applied: rulings from one note then apply in any order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub append: Option<RulingEntry>,
    /// The ingested note this came from, so undoing the ingest withdraws it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

/// A ruling to append to a decisions log.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulingEntry {
    pub ruling: String,
    pub decider: Option<String>,
    pub date: String,
    pub note: String,
}

impl Proposal {
    pub fn new(page: impl Into<String>, base: impl Into<String>, proposed: impl Into<String>) -> Proposal {
        Proposal { page: page.into(), base: base.into(), proposed: proposed.into(), ..Default::default() }
    }
}

/// Pages a new repo changes, and what each should gain.
const ON_ADD: &[(&str, &str)] = &[
    ("Conventions/ARCHITECTURE.md", "work the new repos into the layers, what may call what, and the diagram"),
    ("Current/Who-Does-What.md", "add who owns each new repo and who to ask"),
    ("Current/Project.md", "say what the new repos add to the project, only if they change what it is or does"),
    (
        "Vocabulary.md",
        "add a row to the word table for each term the new repos use for something the team already names differently",
    ),
];

pub fn update_prompt(page: &str, change: &str, existing: &str, sources: &[Source], today: &str) -> String {
    let mut s = format!(
        "You are proposing a change to one page of a team's wiki, `{page}`, because repos were added to the team. \
         A person keeps this page; they will see your change as a diff and apply it or not. The change: {change}.\n\n\
         Rules:\n\
         - Change only what the new repos change. Keep every other line exactly as it is, in the same order.\n\
         - Use only what the sources below say, and cite each new fact inline as its label in backticks.\n\
         - Set `updated: {today}` in the frontmatter; leave any `verified:` line as it is.\n\
         - If the new repos change nothing on this page, reply exactly NO CHANGE.\n\
         - Otherwise reply with the whole page, starting with `---`. No preamble, no code fences around it.\n\n\
         THE PAGE NOW:\n{existing}\n\nSOURCES (the new repos):\n"
    );
    for src in sources {
        s.push_str(&format!("\n=== {} ===\n{}\n", src.label, src.text));
    }
    s
}

/// The model's reply as a proposed page, or `None` for no change.
pub fn finish_update(reply: &str, existing: &str) -> Result<Option<String>> {
    let mut t = reply.trim();
    if t.eq_ignore_ascii_case("NO CHANGE") {
        return Ok(None);
    }
    if let Some(rest) = t.strip_prefix("```markdown").or_else(|| t.strip_prefix("```md")).or_else(|| t.strip_prefix("```")) {
        t = rest.trim_start().strip_suffix("```").unwrap_or(rest).trim();
    }
    if existing.starts_with("---") && !t.starts_with("---") {
        return Err(Error::Other("the proposed page lost its frontmatter".into()));
    }
    let page = format!("{t}\n");
    Ok((page.trim() != existing.trim()).then_some(page))
}

fn file_proposal(db: &mut Db, p: &Proposal, added: &[String], now: i64) -> Result<()> {
    let names = added.join(", ");
    let title = format!("Proposed: {} with {names}", p.page);
    let body = format!(
        "Ken read {names} and proposes this change to {}, a page a person keeps. Apply it to write it, or discard it. \
         It applies only while the page still reads as it did when Ken proposed it.",
        p.page
    );
    file_page_proposal(db, p, &title, &body, now)
}

/// File one proposed page change as a Review card, to apply or discard.
pub fn file_page_proposal(db: &mut Db, p: &Proposal, title: &str, body: &str, now: i64) -> Result<()> {
    let payload = serde_json::to_string(p).map_err(|e| Error::Other(e.to_string()))?;
    db.insert_review_item(PROPOSAL_KIND, title, body, &p.page, Some(&payload), now)?;
    Ok(())
}

/// Repos joined the team: draft their Repo Map pages, then for each page of
/// [`ON_ADD`] and the Repo Map index, draft it when it is still missing or a
/// template, or file a proposed change when a person keeps it. `all` is
/// every repo of the team now, the added ones included.
#[allow(clippy::too_many_arguments)]
pub fn draft_added(
    wiki: &Path,
    wiki_name: &str,
    db: &mut Db,
    added: &[(String, PathBuf)],
    all: &[(String, PathBuf)],
    today: &str,
    now: i64,
    mut generate: impl FnMut(&str) -> Result<String>,
) -> Result<DraftReport> {
    let mut report = DraftReport::default();
    let names: Vec<String> = added.iter().map(|(n, _)| n.clone()).collect();
    draft_repo_pages(wiki, added, today, &mut report, &mut generate)?;

    // The index: a new one lists every repo; a kept one gains a row per new repo.
    let index = format!("{REPO_MAP}/Index.md");
    let existing = fs::read_to_string(wiki.join(&index)).ok();
    let describe = |r: &Path| crate::registry::description_of(r).unwrap_or_default();
    match existing.as_deref().filter(|t| !may_draft(Some(t))) {
        None => {
            let listed: Vec<(String, String)> = all.iter().map(|(n, r)| (n.clone(), describe(r))).collect();
            write_page(&wiki.join(&index), &repo_map_index(&listed, today))?;
            report.drafted.push(index);
        }
        Some(text) => {
            let mut proposed = text.trim_end().to_string();
            proposed.push('\n');
            let mut changed = false;
            for (n, r) in added {
                if !text.contains(&format!("[[{}]]", crate::workspace::member_leaf(n))) {
                    proposed.push_str(&index_row(n, &describe(r)));
                    changed = true;
                }
            }
            if changed {
                let p = Proposal::new(index.clone(), text.to_string(), proposed);
                file_proposal(db, &p, &names, now)?;
                report.proposed.push(index);
            }
        }
    }

    let sources = team_sources(wiki, wiki_name, added, &[]);
    report.sources.extend(sources.iter().map(|s| s.label.clone()));
    for (page, change) in ON_ADD {
        let existing = fs::read_to_string(wiki.join(page)).ok();
        if may_draft(existing.as_deref()) {
            // Still missing or a template: draft it whole, from every repo.
            let purpose =
                PAGES.iter().find(|(p, _)| p == page).map(|(_, u)| u.to_string()).unwrap_or_else(|| change.to_string());
            let everything = team_sources(wiki, wiki_name, all, &[]);
            draft_pages(wiki, &[(page.to_string(), purpose)], &everything, today, &mut report, &mut generate)?;
            continue;
        }
        let existing = existing.unwrap_or_default();
        match generate(&update_prompt(page, change, &existing, &sources, today)).and_then(|r| finish_update(&r, &existing)) {
            Ok(Some(proposed)) => {
                file_proposal(db, &Proposal::new(page.to_string(), existing, proposed), &names, now)?;
                report.proposed.push(page.to_string());
            }
            Ok(None) => report.kept.push(page.to_string()),
            Err(e) => report.failed.push((page.to_string(), e.to_string())),
        }
    }
    report.sources.dedup();
    let title =
        format!("{} added to the wiki: {} drafted, {} proposed", names.join(", "), report.drafted.len(), report.proposed.len());
    file_card(db, &report, &title, now)?;
    Ok(report)
}

// --- Which repos a wiki covers: a wiki belongs to a team, and covers the
// team's repos. -----------------------------------------------------------

/// A workspace member as the wiki sees it: its name, folder, kinds and team.
#[derive(Debug, Clone, PartialEq)]
pub struct TeamMember {
    pub name: String,
    pub root: PathBuf,
    pub kind: Vec<crate::registry::RepoKind>,
    pub team: Option<String>,
}

impl TeamMember {
    fn is_wiki(&self) -> bool {
        self.kind.contains(&crate::registry::RepoKind::Wiki)
    }
}

/// The repos a wiki drafts from: the other members on its team, wikis left
/// out. A wiki with no team covers every member that has no team.
pub fn team_repos(wiki: &str, members: &[TeamMember]) -> Vec<(String, PathBuf)> {
    let Some(w) = members.iter().find(|m| m.name == wiki) else { return Vec::new() };
    members
        .iter()
        .filter(|m| m.name != wiki && !m.is_wiki() && m.team == w.team)
        .map(|m| (m.name.clone(), m.root.clone()))
        .collect()
}

/// The wiki that covers `member`: the wiki on its team (the first by name
/// when a team has two).
pub fn wiki_for<'a>(member: &str, members: &'a [TeamMember]) -> Option<&'a TeamMember> {
    let m = members.iter().find(|m| m.name == member)?;
    let mut wikis: Vec<&TeamMember> = members.iter().filter(|w| w.is_wiki() && w.name != member && w.team == m.team).collect();
    wikis.sort_by(|a, b| a.name.cmp(&b.name));
    wikis.into_iter().next()
}

// --- A repo removed from the team: the pages that still cite it. -----------

/// The wiki pages that cite `repo`: each page with the citations that name
/// it (its `sources:`), and the repo's own Repo Map page. Read from the index.
pub fn citing_pages(db: &Db, repo: &str) -> Result<Vec<(String, Vec<String>)>> {
    let leaf = crate::workspace::member_leaf(repo);
    let own = repo_page(repo);
    let mut out = Vec::new();
    for page in db.page_paths()?.into_iter().filter(|p| p.ends_with(".md")) {
        let Some(meta) = db.page_meta(&page)? else { continue };
        let cites: Vec<String> = meta
            .sources
            .iter()
            .filter(|s| {
                matches!(crate::drift::classify(s), crate::drift::Citation::Code(c) if c.repo.eq_ignore_ascii_case(leaf))
            })
            .map(|s| s.trim().trim_matches('"').to_string())
            .collect();
        if !cites.is_empty() || page.eq_ignore_ascii_case(&own) {
            out.push((page, cites));
        }
    }
    Ok(out)
}

/// File one Review card on the wiki listing the pages that still cite a repo
/// that left the team, so a person rewrites or retires them. Nothing is
/// edited. `None` when no page cites it.
pub fn file_removed_card(db: &mut Db, repo: &str, now: i64) -> Result<Option<usize>> {
    let pages = citing_pages(db, repo)?;
    if pages.is_empty() {
        return Ok(None);
    }
    let own = repo_page(repo);
    let mut body = format!(
        "**{repo}** was removed from the team. These pages still cite it; each needs rewriting without it, or retiring \
         (`status: retired` with what replaced it). Nothing was changed.\n\n"
    );
    for (page, cites) in &pages {
        if page.eq_ignore_ascii_case(&own) {
            body.push_str(&format!("- {page} — its Repo Map page\n"));
        } else {
            body.push_str(&format!("- {page} — cites {}\n", cites.join(", ")));
        }
    }
    let title = format!("{repo} left the team: {} pages cite it", pages.len());
    let first = pages.first().map(|(p, _)| p.clone()).unwrap_or_default();
    db.insert_review_item(REVIEW_KIND, &title, &body, &first, None, now)?;
    Ok(Some(pages.len()))
}

/// Why a proposal could not be applied.
#[derive(Debug, PartialEq)]
pub enum ApplyError {
    /// The page changed after Ken proposed; the proposal is out of date.
    Changed,
    Io(String),
}

/// Write a proposal, only while the page still reads as `base`: a change
/// made against an older page would undo whatever a person wrote since. A
/// ruling to append goes onto the log as it is now, whatever was added since.
/// `wiki` is the repo whose Review holds the card; `p.root` wins when set.
pub fn apply(wiki: &Path, p: &Proposal) -> std::result::Result<(), ApplyError> {
    let root = p.root.as_deref().map(Path::new).unwrap_or(wiki);
    let path = root.join(&p.page);
    let now = fs::read_to_string(&path).unwrap_or_default();
    if let Some(r) = &p.append {
        let log = if now.trim().is_empty() { "# Decisions\n".to_string() } else { now };
        let text = crate::ingest::decisions_entry(&log, &r.ruling, r.decider.as_deref(), &r.date, &r.note);
        return write_page(&path, &text).map_err(|e| ApplyError::Io(e.to_string()));
    }
    if now.replace("\r\n", "\n") != p.base.replace("\r\n", "\n") {
        return Err(ApplyError::Changed);
    }
    write_page(&path, &p.proposed).map_err(|e| ApplyError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gather_reads_readmes_docs_layout_and_an_extra_folder_within_budget() {
        let repo = tempfile::tempdir().unwrap();
        fs::write(repo.path().join("README.md"), "# Game\nA tactics game.\n").unwrap();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(repo.path().join("docs/save.md"), "Saves are region files.\n").unwrap();
        fs::create_dir_all(repo.path().join("src/world")).unwrap();
        fs::create_dir_all(repo.path().join("node_modules/x")).unwrap();
        let extra = tempfile::tempdir().unwrap();
        fs::write(extra.path().join("Confluence-Home.txt"), "Our roadmap: ship in Q4.\n").unwrap();

        let s = gather(&[("game".into(), repo.path().to_path_buf())], Some(extra.path()));
        let labels: Vec<&str> = s.iter().map(|s| s.label.as_str()).collect();
        assert!(labels.contains(&"game:README.md"), "{labels:?}");
        assert!(labels.contains(&"game:docs/save.md"));
        let layout = s.iter().find(|s| s.label == "game:(layout)").unwrap();
        assert!(layout.text.contains("src/") && layout.text.contains("  world/") && !layout.text.contains("node_modules"));
        assert!(s.iter().any(|s| s.label.ends_with(":Confluence-Home.txt") && s.text.contains("Q4")));
    }

    #[test]
    fn only_missing_or_untouched_template_pages_are_drafted() {
        assert!(may_draft(None));
        assert!(may_draft(Some("---\ntitle: \"Team\"\n---\n| {{role}} | {{name}} |\n")));
        assert!(!may_draft(Some("---\ntitle: Team\n---\nAna leads.\n")));
    }

    #[test]
    fn a_draft_is_marked_draft_and_never_verified() {
        let page = finish("```\n---\ntitle: Project\nstatus: current\nverified: 2026-09-24\nsources:\n  - game:README.md\n---\n# Project\n```").unwrap();
        assert!(page.contains("status: draft") && !page.contains("verified:") && page.contains("game:README.md"));
        assert!(finish("Here you go").is_err());
    }

    #[test]
    fn a_wiki_is_drafted_around_what_a_person_wrote() {
        let wiki = tempfile::tempdir().unwrap();
        fs::create_dir_all(wiki.path().join("Current")).unwrap();
        fs::write(wiki.path().join("Current/Team.md"), "---\ntitle: Team\n---\nAna leads; Ben reviews.\n").unwrap();
        fs::write(wiki.path().join("Current/Project.md"), "---\ntitle: \"Project\"\n---\n{{one paragraph}}\n").unwrap();
        let sources = vec![Source { label: "game:README.md".into(), text: "A tactics game.".into() }];
        let mut db = Db::open_in_memory().unwrap();
        let mut prompts: Vec<String> = Vec::new();
        let report = draft(wiki.path(), &mut db, &sources, "2026-09-24", 5, |p| {
            prompts.push(p.to_string());
            Ok("---\ntitle: Drafted\nsources:\n  - game:README.md\n---\n# Drafted\nDrafted by Ken.\n".into())
        })
        .unwrap();
        assert_eq!(report.kept, vec!["Current/Team.md"]);
        assert_eq!(
            report.drafted,
            vec!["Current/Project.md", "Current/Who-Does-What.md", "Conventions/ARCHITECTURE.md", "Work/Releases.md"]
        );
        assert!(prompts[0].contains("{{one paragraph}}"), "the template page's own text is the template");
        assert!(prompts.iter().all(|p| p.contains("=== game:README.md ===")));
        assert_eq!(fs::read_to_string(wiki.path().join("Current/Team.md")).unwrap(), "---\ntitle: Team\n---\nAna leads; Ben reviews.\n");
        assert!(fs::read_to_string(wiki.path().join("Conventions/ARCHITECTURE.md")).unwrap().contains("status: draft"));
        let (_, body) = db.open_review_item_of_kind(REVIEW_KIND).unwrap().unwrap();
        assert!(body.contains("Left alone") && body.contains("Current/Team.md"));
    }
    fn page(title: &str) -> String {
        format!("---\ntitle: {title}\nsources:\n  - x\n---\n# {title}\nDrafted by Ken.\n")
    }

    fn repo(dir: &Path, name: &str, readme: &str) -> (String, PathBuf) {
        let root = dir.join(name);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("README.md"), readme).unwrap();
        (name.to_string(), root)
    }

    #[test]
    fn a_team_wiki_is_drafted_repo_by_repo_then_from_the_repo_pages() {
        let d = tempfile::tempdir().unwrap();
        let wiki = d.path().join("Wiki");
        fs::create_dir_all(&wiki).unwrap();
        let repos = vec![repo(d.path(), "Game", "# Game\nThe client.\n"), repo(d.path(), "Tools", "# Tools\nThe editor.\n")];
        let mut db = Db::open_in_memory().unwrap();
        let mut prompts: Vec<String> = Vec::new();
        let report = draft_team(&wiki, "Wiki", &mut db, &repos, None, "2026-09-25", 5, |p| {
            prompts.push(p.to_string());
            Ok(page("Drafted"))
        })
        .unwrap();
        assert_eq!(&report.drafted[..3], &["Repo-Map/Game.md", "Repo-Map/Tools.md", "Repo-Map/Index.md"]);
        assert!(report.drafted.contains(&"Conventions/ARCHITECTURE.md".to_string()));
        // Pass one reads one repo each.
        assert!(prompts[0].contains("=== Game:README.md ===") && !prompts[0].contains("Tools:README.md"));
        assert!(prompts[1].contains("=== Tools:README.md ===") && !prompts[1].contains("Game:README.md"));
        // Pass two reads the repo pages, not the raw repos.
        let team = &prompts[2];
        assert!(team.contains("=== Wiki:Repo-Map/Game.md ===") && team.contains("=== Wiki:Repo-Map/Tools.md ==="), "{team}");
        assert!(!team.contains("=== Game:README.md ==="));
        let index = fs::read_to_string(wiki.join("Repo-Map/Index.md")).unwrap();
        assert!(index.contains("| [[Game]] | (not said yet) |") && index.contains("| [[Tools]] |"));
    }

    #[test]
    fn every_repo_gets_its_own_budget_however_many_there_are() {
        let d = tempfile::tempdir().unwrap();
        let big = "word ".repeat(40_000);
        let repos: Vec<(String, PathBuf)> = (0..20).map(|i| repo(d.path(), &format!("r{i}"), &big)).collect();
        let s = gather(&repos, None);
        assert!(s.iter().any(|s| s.label == "r19:README.md"), "the last repo is still read");
    }

    #[test]
    fn an_added_repo_gets_its_page_and_kept_pages_get_proposals() {
        let d = tempfile::tempdir().unwrap();
        let wiki = d.path().join("Wiki");
        fs::create_dir_all(wiki.join("Repo-Map")).unwrap();
        fs::create_dir_all(wiki.join("Current")).unwrap();
        fs::create_dir_all(wiki.join("Conventions")).unwrap();
        let index = "---\ntitle: Repo Map\n---\n| repo | what it is for |\n|---|---|\n| [[Game]] | The client. |\n";
        fs::write(wiki.join("Repo-Map/Index.md"), index).unwrap();
        let arch = "---\ntitle: Architecture\n---\nGame calls the server.\n";
        fs::write(wiki.join("Conventions/ARCHITECTURE.md"), arch).unwrap();
        fs::write(wiki.join("Current/Who-Does-What.md"), "---\ntitle: Who\n---\nAna owns Game.\n").unwrap();
        fs::write(wiki.join("Current/Project.md"), "---\ntitle: Project\n---\nA game.\n").unwrap();
        let game = repo(d.path(), "Game", "# Game\n");
        let tools = repo(d.path(), "Tools", "# Tools\nThe level editor.\n");
        let mut db = Db::open_in_memory().unwrap();
        let report = draft_added(&wiki, "Wiki", &mut db, &[tools.clone()], &[game, tools], "2026-09-25", 9, |p| {
            Ok(if p.contains("`Repo-Map/Tools.md`") {
                page("Tools")
            } else if p.contains("`Conventions/ARCHITECTURE.md`") {
                "---\ntitle: Architecture\n---\nGame calls the server. Tools writes levels the Game loads.\n".into()
            } else if p.contains("`Vocabulary.md`") {
                page("Vocabulary")
            } else {
                "NO CHANGE".into()
            })
        })
        .unwrap();
        assert!(report.drafted.contains(&"Repo-Map/Tools.md".to_string()), "{report:?}");
        assert!(report.drafted.contains(&"Vocabulary.md".to_string()), "a missing page is drafted whole");
        assert_eq!(report.proposed, vec!["Repo-Map/Index.md", "Conventions/ARCHITECTURE.md"]);
        assert_eq!(report.kept, vec!["Current/Who-Does-What.md", "Current/Project.md"]);
        assert_eq!(fs::read_to_string(wiki.join("Conventions/ARCHITECTURE.md")).unwrap(), arch, "a kept page is never written");

        let items = db.list_open_review_items().unwrap();
        let mut props: Vec<Proposal> = items
            .iter()
            .filter(|i| i.kind == PROPOSAL_KIND)
            .map(|i| serde_json::from_str(i.payload.as_deref().unwrap()).unwrap())
            .collect();
        props.sort_by(|a, b| b.page.cmp(&a.page)); // Repo-Map/Index.md, then Conventions/…
        assert_eq!(props.len(), 2);
        assert!(props[0].proposed.ends_with("| [[Game]] | The client. |\n| [[Tools]] | (not said yet) |\n"));

        // Applied while the page reads as it did; refused once it changed.
        apply(&wiki, &props[1]).unwrap();
        assert!(fs::read_to_string(wiki.join("Conventions/ARCHITECTURE.md")).unwrap().contains("Tools writes levels"));
        fs::write(wiki.join("Repo-Map/Index.md"), format!("{index}| [[Server]] | added by hand |\n")).unwrap();
        assert_eq!(apply(&wiki, &props[0]), Err(ApplyError::Changed));
    }

    #[test]
    fn an_update_reply_of_no_change_or_the_same_page_proposes_nothing() {
        let page = "---\ntitle: A\n---\nText.\n";
        assert_eq!(finish_update("NO CHANGE", page).unwrap(), None);
        assert_eq!(finish_update(page, page).unwrap(), None);
        assert!(finish_update("Sure, here it is", page).is_err());
        assert!(finish_update("---\ntitle: A\n---\nText and more.", page).unwrap().is_some());
    }
    #[test]
    fn a_removed_repo_lists_the_pages_that_cite_it() {
        let d = tempfile::tempdir().unwrap();
        let wiki = d.path().join("Wiki");
        fs::create_dir_all(wiki.join("Platform")).unwrap();
        fs::create_dir_all(wiki.join("Repo-Map")).unwrap();
        fs::write(wiki.join("Platform/Save.md"), "---\nsources:\n  - Tools:src/save.rs:3\n  - Game:src/x.rs\n---\n# Save\n").unwrap();
        fs::write(wiki.join("Platform/Combat.md"), "---\nsources:\n  - Game:src/hit.rs\n---\n# Combat\n").unwrap();
        fs::write(wiki.join("Repo-Map/Tools.md"), "---\ntitle: Tools\n---\n# Tools\n").unwrap();
        let project = crate::project::Project::create(&wiki, "Wiki").unwrap();
        let mut db = Db::open_in_memory().unwrap();
        crate::scan::scan(&project, &mut db).unwrap();

        let mut pages = citing_pages(&db, "Tools").unwrap();
        pages.sort();
        assert_eq!(
            pages,
            vec![
                ("Platform/Save.md".to_string(), vec!["Tools:src/save.rs:3".to_string()]),
                ("Repo-Map/Tools.md".to_string(), vec![]),
            ]
        );
        assert_eq!(file_removed_card(&mut db, "Tools", 7).unwrap(), Some(2));
        let (_, body) = db.open_review_item_of_kind(REVIEW_KIND).unwrap().unwrap();
        assert!(body.contains("Platform/Save.md — cites Tools:src/save.rs:3") && body.contains("its Repo Map page"));
        assert_eq!(file_removed_card(&mut db, "Nobody", 7).unwrap(), None);
    }

    #[test]
    fn a_wiki_covers_its_teams_repos_and_a_repo_finds_its_teams_wiki() {
        use crate::registry::RepoKind;
        let m = |name: &str, kind: RepoKind, team: Option<&str>| TeamMember {
            name: name.into(),
            root: PathBuf::from(name),
            kind: vec![kind],
            team: team.map(String::from),
        };
        let members = vec![
            m("Realms-Wiki", RepoKind::Wiki, Some("Realms")),
            m("Game", RepoKind::Code, Some("Realms")),
            m("Server", RepoKind::Code, Some("Realms")),
            m("Ops-Wiki", RepoKind::Wiki, Some("Ops")),
            m("Infra", RepoKind::Code, Some("Ops")),
            m("Scratch", RepoKind::Code, None),
        ];
        let names = |v: Vec<(String, PathBuf)>| v.into_iter().map(|(n, _)| n).collect::<Vec<_>>();
        assert_eq!(names(team_repos("Realms-Wiki", &members)), vec!["Game", "Server"]);
        assert_eq!(names(team_repos("Ops-Wiki", &members)), vec!["Infra"]);
        assert_eq!(wiki_for("Server", &members).map(|w| w.name.as_str()), Some("Realms-Wiki"));
        assert_eq!(wiki_for("Scratch", &members), None, "no team wiki, no update");
    }
}
