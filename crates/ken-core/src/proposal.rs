//! A change to one page that waits for a person: the page as Ken read it,
//! the page as Ken would write it, and what to append when the change is a
//! decisions-log entry. Ingest files one when staging holds an edit, a ruling
//! waits for its confirm, or a ticket is drafted from an action; Apply
//! writes it only while the page still reads as Ken read it.
//!
//! Also the prompt and the finishing for a new page an ingested note calls
//! for, so ingest owns how it writes a page.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::{Error, Result};

/// Review kind of a proposed change to one page.
pub const PROPOSAL_KIND: &str = "page-proposal";

/// One thing a page is written from, labelled the way the page cites it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// `[[Note Name]]` for an ingested note, `repo:path` for a file.
    pub label: String,
    pub text: String,
}

/// A change to one page, held until a person applies it. `base` is the page
/// as Ken read it; the change applies only while the page still reads so.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub page: String,
    pub base: String,
    pub proposed: String,
    /// The repo the page is in, when not the one whose Review holds the card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// A decisions-log entry to append, worked out against the log as it is
    /// when applied: rulings from one note then apply in any order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub append: Option<RulingEntry>,
    /// The ingested note this came from.
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

/// Store one proposed page change, to apply or discard on its ingest card.
pub fn file_page_proposal(db: &mut Db, p: &Proposal, title: &str, body: &str, now: i64) -> Result<()> {
    let payload = serde_json::to_string(p).map_err(|e| Error::Other(e.to_string()))?;
    db.insert_review_item(PROPOSAL_KIND, title, body, &p.page, Some(&payload), now)?;
    Ok(())
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
/// `root` is the repo whose Review holds the card; `p.root` wins when set.
pub fn apply(root: &Path, p: &Proposal) -> std::result::Result<(), ApplyError> {
    let root = p.root.as_deref().map(Path::new).unwrap_or(root);
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

fn write_page(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    fs::write(path, text).map_err(|e| Error::io(path, e))
}

/// The prompt for a new page an ingested note calls for: written only from
/// the note, citing it.
pub fn page_prompt(page: &str, purpose: &str, sources: &[Source], today: &str) -> String {
    let mut s = format!(
        "You are writing one new page of a team's wiki, `{page}`, from what was just ingested. The page is: {purpose}.\n\n\
         Rules:\n\
         - Use only what the sources below say. Leave out anything they do not say; never write that something is unknown.\n\
         - Cite each statement with the label of the source that says it, such as `[[Note Name]]`.\n\
         - The wiki complements the code, it does not copy it: say what the sources say about why and what was decided, and \
           point to a file by its path in backticks rather than restating it.\n\
         - Frontmatter: title, status: draft, updated: {today}, and the labels you used under sources.\n\
         - Plain words for a reader who has not seen the code.\n\
         - Reply with the finished page only, in Markdown, starting with `---`. No preamble, no code fences around it.\n\n\
         SOURCES:\n"
    );
    for src in sources {
        s.push_str(&format!("\n=== {} ===\n{}\n", src.label, src.text));
    }
    s
}

/// The reply as a page: fences stripped, `status: draft` set, and any
/// `verified:` line dropped.
pub fn finish(reply: &str) -> Result<String> {
    let t = strip_fences(reply);
    if !t.starts_with("---") {
        return Err(Error::Other("the page did not start with its frontmatter".into()));
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
        return Err(Error::Other("the page's frontmatter was never closed".into()));
    }
    Ok(out)
}

/// The model's reply as a changed page, or `None` for no change.
pub fn finish_update(reply: &str, existing: &str) -> Result<Option<String>> {
    let t = reply.trim();
    if t.eq_ignore_ascii_case("NO CHANGE") {
        return Ok(None);
    }
    let t = strip_fences(t);
    if existing.starts_with("---") && !t.starts_with("---") {
        return Err(Error::Other("the proposed page lost its frontmatter".into()));
    }
    let page = format!("{t}\n");
    Ok((page.trim() != existing.trim()).then_some(page))
}

fn strip_fences(reply: &str) -> &str {
    let t = reply.trim();
    match t.strip_prefix("```markdown").or_else(|| t.strip_prefix("```md")).or_else(|| t.strip_prefix("```")) {
        Some(rest) => {
            let rest = rest.trim_start();
            rest.strip_suffix("```").unwrap_or(rest).trim()
        }
        None => t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_forces_draft_and_drops_verified() {
        let page = finish("```md\n---\ntitle: X\nstatus: current\nverified: 2026-01-01\n---\n\nBody.\n```").unwrap();
        assert!(page.starts_with("---\ntitle: X\nstatus: draft\n---\n"), "{page}");
        assert!(!page.contains("verified"));
        assert!(finish("no frontmatter").is_err());
    }

    #[test]
    fn finish_update_reads_no_change_and_keeps_frontmatter() {
        assert_eq!(finish_update("NO CHANGE", "---\na: 1\n---\n").unwrap(), None);
        assert!(finish_update("Body only", "---\na: 1\n---\n").is_err());
        assert_eq!(finish_update("---\na: 2\n---\n", "---\na: 1\n---\n").unwrap().as_deref(), Some("---\na: 2\n---\n"));
    }

    #[test]
    fn apply_refuses_a_page_that_moved_on() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("P.md"), "now").unwrap();
        let p = Proposal::new("P.md", "then", "proposed");
        assert_eq!(apply(dir.path(), &p), Err(ApplyError::Changed));
        let p = Proposal::new("P.md", "now", "proposed");
        apply(dir.path(), &p).unwrap();
        assert_eq!(fs::read_to_string(dir.path().join("P.md")).unwrap(), "proposed");
    }
}
