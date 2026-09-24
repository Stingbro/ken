//! The first wiki, drafted from an analysis at set-up (knowledge-layer item
//! 4b). Offered, never forced: Claude reads the repos (READMEs, briefs, top
//! docs, layout, markers, who commits), and any folder of documents a person
//! adds (a Confluence export, say), and drafts the Current pages and the
//! architecture page for a person to review.
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

fn git_authors(root: &Path) -> Option<String> {
    let mut cmd = Command::new("git");
    let out = crate::proc::quiet(&mut cmd).args(["shortlog", "-sne", "--all", "--no-merges"]).current_dir(root).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let top: Vec<&str> = text.lines().take(12).collect();
    (out.status.success() && !top.is_empty()).then(|| top.join("\n"))
}

/// Read what the draft is made from: per repo its READMEs and briefs, up to
/// ten top-level docs, its layout, CODEOWNERS and who commits; then every
/// readable document in `extra`. Clipped to [`SOURCE_BUDGET`] in all.
pub fn gather(repos: &[(String, PathBuf)], extra: Option<&Path>) -> Vec<Source> {
    let mut out: Vec<Source> = Vec::new();
    let push = |label: String, text: String, out: &mut Vec<Source>| {
        let used: usize = out.iter().map(|s| s.text.len()).sum();
        if used < SOURCE_BUDGET {
            out.push(Source { label, text: clip(&text, PER_FILE.min(SOURCE_BUDGET - used)) });
        }
    };
    for (name, root) in repos {
        for f in ["README.md", "readme.md", "README", "CLAUDE.md", "START-HERE.md", "AGENTS.md"] {
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
    }
    if let Some(dir) = extra {
        let label = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "documents".into());
        let walker = ignore::WalkBuilder::new(dir).hidden(true).build().flatten();
        for e in walker.filter(|e| e.path().is_file()).take(200) {
            if let Some(t) = read_text(e.path()) {
                let rel = e.path().strip_prefix(dir).unwrap_or(e.path()).to_string_lossy().replace('\\', "/");
                push(format!("{label}:{rel}"), t, &mut out);
            }
        }
    }
    out
}

/// A page Ken may write: missing, or a template nobody has filled in.
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

/// What a draft run did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftReport {
    pub drafted: Vec<String>,
    /// Pages a person already wrote, left alone.
    pub kept: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub sources: Vec<String>,
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
    for (page, purpose) in PAGES {
        let path = wiki.join(page);
        let existing = fs::read_to_string(&path).ok();
        if !may_draft(existing.as_deref()) {
            report.kept.push(page.to_string());
            continue;
        }
        match generate(&prompt(page, purpose, existing.as_deref(), sources, today)).and_then(|r| finish(&r)) {
            Ok(text) => {
                if let Some(dir) = path.parent() {
                    fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
                }
                fs::write(&path, text).map_err(|e| Error::io(&path, e))?;
                report.drafted.push(page.to_string());
            }
            Err(e) => report.failed.push((page.to_string(), e.to_string())),
        }
    }
    let mut body = String::new();
    if !report.drafted.is_empty() {
        body.push_str("**Drafted for you to read against their sources** (each is `status: draft`, not verified):\n");
        for p in &report.drafted {
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
    let title = format!("First wiki drafted: {} pages to read", report.drafted.len());
    let first = report.drafted.first().cloned().unwrap_or_default();
    db.insert_review_item(REVIEW_KIND, &title, &body, &first, None, now)?;
    Ok(report)
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
        assert_eq!(report.drafted, vec!["Current/Project.md", "Current/Who-Does-What.md", "Conventions/ARCHITECTURE.md"]);
        assert!(prompts[0].contains("{{one paragraph}}"), "the template page's own text is the template");
        assert!(prompts.iter().all(|p| p.contains("=== game:README.md ===")));
        assert_eq!(fs::read_to_string(wiki.path().join("Current/Team.md")).unwrap(), "---\ntitle: Team\n---\nAna leads; Ben reviews.\n");
        assert!(fs::read_to_string(wiki.path().join("Conventions/ARCHITECTURE.md")).unwrap().contains("status: draft"));
        let (_, body) = db.open_review_item_of_kind(REVIEW_KIND).unwrap().unwrap();
        assert!(body.contains("Left alone") && body.contains("Current/Team.md"));
    }
}
