//! A new knowledge base, laid down at set-up from Ken's own template
//! (`templates/wiki`, bundled so set-up works offline and every team starts
//! from the same version). One repo holds the library, the decisions log, the
//! tickets, `log.md` and the settings in `.ken/knowledge.json`.
//!
//! Only the facts Ken knows are filled in: the team's name, the repo's own
//! name, the ticket key, the repos it covers with what each is for, and
//! `updated:` dates. Every other `{{…}}` stays for a person. The folder
//! becomes a git repo with one commit; publishing it (a remote, a push) is a
//! person's step.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

macro_rules! template {
    ($($path:literal),* $(,)?) => {
        &[$(($path, include_str!(concat!("../templates/wiki/", $path)))),*]
    };
}

/// Every file of the template: (path in the knowledge base, text).
pub const TEMPLATE: &[(&str, &str)] = template![
    ".ken/knowledge.json",
    "CLAUDE.md",
    "START-HERE.md",
    "log.md",
    "Current/Index.md",
    "Current/Project.md",
    "Decisions/DECISIONS.md",
    "Design/Index.md",
    "Reference/Index.md",
    "Reference/Platform/.gitkeep",
    "Reference/Vocabulary.md",
    "Research/Index.md",
    "Research/Ingestion/Index.md",
    "Research/Ingestion/Ingested/.gitkeep",
    "Research/Ingestion/Raw/.gitkeep",
    "Rules/Index.md",
    "Templates/Index.md",
    "Templates/Design-Note.md",
    "Templates/Feature-Status.md",
    "Templates/Finding.md",
    "Templates/How-to.md",
    "Templates/Ingested-note.md",
    "Templates/Platform.md",
    "Templates/Roadmap.md",
    "Templates/Rule.md",
    "Templates/Ticket.md",
    "tickets/README.md",
];

/// Where the settings live in a knowledge base.
pub const KNOWLEDGE_FILE: &str = ".ken/knowledge.json";

/// A repo the new knowledge base covers: its name in the workspace and what it
/// is for, and, when set-up knows them, its kinds and folder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Covered {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub kind: Vec<crate::registry::RepoKind>,
    #[serde(default)]
    pub path: Option<std::path::PathBuf>,
}

/// Pages that are templates for people to copy keep their placeholders,
/// dates included. The blanks all live in `Templates/`; its index is a page.
pub(crate) fn is_copy_template(path: &str) -> bool {
    path.starts_with("Templates/") && path != "Templates/Index.md"
}

/// A ticket key from the team's name: the initials of its words when it has
/// two or more (`Shattered-Realms` → SR), else its first six letters and
/// digits; in upper case, TEAM when it has none.
pub fn ticket_key(team: &str) -> String {
    let words: Vec<&str> = team.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let k: String = if words.len() >= 2 {
        words.iter().filter_map(|w| w.chars().next()).collect()
    } else {
        team.chars().filter(|c| c.is_ascii_alphanumeric()).take(6).collect()
    };
    let k = k.to_uppercase();
    if k.is_empty() {
        "TEAM".into()
    } else {
        k
    }
}

fn one_line(s: &str) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ").replace('|', "/");
    if s.is_empty() {
        "(not said yet)".into()
    } else {
        s
    }
}

/// What git says about a repo: the branch it is on (None on no branch), its
/// HEAD, its `origin` URL.
fn repo_facts(dir: &Path) -> (Option<String>, Option<String>, Option<String>) {
    let git = |args: &[&str]| {
        let mut cmd = Command::new("git");
        let out = crate::proc::quiet(&mut cmd).args(args).current_dir(dir).output().ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (out.status.success() && !s.is_empty()).then_some(s)
    };
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]).filter(|b| b != "HEAD");
    (branch, git(&["rev-parse", "HEAD"]), crate::setup::origin_url(dir))
}

/// The `repos` entries for `.ken/knowledge.json`: this knowledge base, then
/// each covered repo with its kind, the branch it is on, and for a reference
/// repo the commit read and its `origin`.
fn knowledge_repos(wiki: &str, repos: &[Covered]) -> String {
    use crate::registry::RepoKind;
    use serde_json::{json, Value};
    let q = |s: &str| serde_json::to_string(s).unwrap_or_default();
    let mut entries = vec![format!("    {}: {}", q(wiki), json!({ "kind": ["wiki", "team"], "base": "main" }))];
    for r in repos.iter().filter(|r| r.name != wiki) {
        let kinds: Vec<Value> = r.kind.iter().map(|k| serde_json::to_value(k).unwrap_or(Value::Null)).collect();
        let kind = match kinds.as_slice() {
            [] => json!("code"),
            [one] => one.clone(),
            _ => Value::Array(kinds),
        };
        let (branch, head, origin) = r.path.as_deref().map(repo_facts).unwrap_or_default();
        let mut entry = serde_json::Map::new();
        entry.insert("kind".into(), kind);
        entry.insert("base".into(), json!(branch.unwrap_or_else(|| "main".into())));
        if r.kind.contains(&RepoKind::Reference) {
            if let Some(h) = head {
                entry.insert("pin".into(), json!(h));
            }
            if let Some(o) = origin {
                entry.insert("upstream".into(), json!(o));
            }
        }
        entries.push(format!("    {}: {}", q(&r.name), Value::Object(entry)));
    }
    entries.join(",\n")
}

/// One template file with what Ken knows filled in.
pub fn fill(path: &str, text: &str, team: &str, wiki: &str, repos: &[Covered], today: &str) -> String {
    let key = ticket_key(team);
    let mut t = text.replace("{{KEY}}", &key);
    if path == KNOWLEDGE_FILE {
        // The one `repos` line of the template becomes every repo's entry.
        let line = t.lines().find(|l| l.trim_start().starts_with("\"{{wiki_repo}}\"")).map(str::to_string);
        if let Some(line) = line {
            t = t.replace(&line, &knowledge_repos(wiki, repos));
        }
        // Inside JSON strings: escaped, the quotes the template already has.
        let inner = |s: &str| {
            let quoted = serde_json::to_string(s).unwrap_or_default();
            quoted.strip_prefix('"').and_then(|q| q.strip_suffix('"')).unwrap_or(&quoted).to_string()
        };
        return t.replace("{{team_name}}", &inner(team)).replace("{{wiki_repo}}", &inner(wiki));
    }
    t = t.replace("{{team_name}}", team).replace("{{wiki_repo}}", wiki);
    if !is_copy_template(path) {
        t = t
            .lines()
            .map(|l| if l.trim_start().starts_with("updated:") { l.replacen("{{date}}", today, 1) } else { l.to_string() })
            .collect::<Vec<_>>()
            .join("\n")
            + if text.ends_with('\n') { "\n" } else { "" };
    }
    if path == "START-HERE.md" {
        // The repo table: this knowledge base, then each repo it covers.
        let mut rows = format!("| `{wiki}` | this repo · the knowledge base |\n");
        for r in repos.iter().filter(|r| r.name != wiki) {
            rows.push_str(&format!("| `{}` | {} |\n", r.name, one_line(&r.description)));
        }
        let mut out = String::new();
        let mut replaced = false;
        for line in t.lines() {
            if line.starts_with(&format!("| `{wiki}` |")) {
                if !replaced {
                    out.push_str(&rows);
                    replaced = true;
                }
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        t = out;
    }
    t
}

fn git(dir: &Path, args: &[&str]) -> Result<()> {
    let mut cmd = Command::new("git");
    let out = crate::proc::quiet(&mut cmd)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| Error::Other(format!("git could not run: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(Error::Other(format!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim())))
    }
}

/// Lay a new knowledge base down at `dir` (missing or empty) for `team`,
/// covering `repos`, then make it a git repo with one commit. Refuses a
/// folder that already holds anything, so it never writes over a person's
/// files.
pub fn create(dir: &Path, team: &str, repos: &[Covered], today: &str) -> Result<()> {
    if dir.exists() && fs::read_dir(dir).map_err(|e| Error::io(dir, e))?.next().is_some() {
        return Err(Error::Other(format!("{} is not empty; pick a new folder for the knowledge base", dir.display())));
    }
    let wiki = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "wiki".into());
    for (path, text) in TEMPLATE {
        let dest = dir.join(path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        // LF, whatever the checkout Ken was built from used.
        let text = text.replace("\r\n", "\n");
        fs::write(&dest, fill(path, &text, team, &wiki, repos, today)).map_err(|e| Error::io(&dest, e))?;
    }
    git_commit_all(dir, &format!("Start the {team} knowledge base from Ken's template"))
}

/// `git init` in `dir` and one commit of everything in it. The person's own
/// git identity when they have one; Ken's otherwise, so a machine with no
/// identity set can still make the first commit.
pub(crate) fn git_commit_all(dir: &Path, msg: &str) -> Result<()> {
    git(dir, &["init", "-q"])?;
    git(dir, &["add", "-A"])?;
    if git(dir, &["commit", "-q", "-m", msg]).is_err() {
        git(dir, &["-c", "user.name=Ken", "-c", "user.email=ken@localhost", "commit", "-q", "-m", msg])?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn repos() -> Vec<Covered> {
        vec![
            Covered { name: "Game".into(), description: "The game client.\nBuilt nightly.".into(), ..Default::default() },
            Covered { name: "Tools".into(), description: String::new(), ..Default::default() },
        ]
    }

    fn template(path: &str) -> &'static str {
        TEMPLATE.iter().find(|(p, _)| *p == path).unwrap().1
    }

    #[test]
    fn start_here_lists_this_knowledge_base_and_its_repos() {
        let t = fill("START-HERE.md", template("START-HERE.md"), "Realms", "Realms-Wiki", &repos(), "2026-09-25");
        assert!(t.contains("The Realms knowledge base"));
        assert!(t.contains(
            "| `Realms-Wiki` | this repo · the knowledge base |\n| `Game` | The game client. Built nightly. |\n| `Tools` | (not said yet) |"
        ));
        assert!(!t.contains("{{wiki_repo}}"));
    }

    #[test]
    fn the_knowledge_file_names_every_repo_and_is_json() {
        let t = fill(KNOWLEDGE_FILE, template(KNOWLEDGE_FILE), "Shattered Realms", "SR-Wiki", &repos(), "2026-09-25");
        let v: serde_json::Value = serde_json::from_str(&t).unwrap();
        assert_eq!(v["team"], "Shattered Realms");
        assert_eq!(v["key"], "SR");
        assert_eq!(v["repos"]["SR-Wiki"]["kind"], serde_json::json!(["wiki", "team"]));
        assert_eq!(v["repos"]["Game"]["kind"], "code");
        assert!(v["repos"]["Tools"].is_object());
    }

    #[test]
    fn ticket_keys_come_from_the_teams_initials() {
        assert_eq!(ticket_key("Shattered-Realms"), "SR");
        assert_eq!(ticket_key("ken"), "KEN");
        assert_eq!(ticket_key("--"), "TEAM");
    }

    /// Every file under `templates/<dir>`, as a path inside it, sorted.
    pub(crate) fn files_under(dir: &str) -> Vec<String> {
        fn walk(root: &Path, at: &Path, out: &mut Vec<String>) {
            for e in fs::read_dir(at).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(root, &p, out);
                } else {
                    out.push(p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"));
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates").join(dir);
        let mut out = Vec::new();
        walk(&root, &root, &mut out);
        out.sort();
        out
    }

    #[test]
    fn the_template_list_is_exactly_the_bundled_folder() {
        let mut listed: Vec<String> = TEMPLATE.iter().map(|(p, _)| p.to_string()).collect();
        listed.sort();
        assert_eq!(listed, files_under("wiki"), "TEMPLATE and templates/wiki differ");
        for (path, text) in TEMPLATE {
            assert!(!text.contains('\r'), "{path} has a CR");
        }
    }

    #[test]
    fn updated_is_dated_and_copy_templates_keep_placeholders() {
        let text = "---\nupdated: {{date}}\n---\n";
        assert_eq!(fill("Current/Project.md", text, "T", "W", &[], "2026-09-25"), "---\nupdated: 2026-09-25\n---\n");
        assert_eq!(fill("Templates/How-to.md", text, "T", "W", &[], "2026-09-25"), text, "a template to copy keeps its placeholders");
    }

    #[test]
    fn create_lays_the_template_down_as_a_repo_and_refuses_a_full_folder() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("Realms-Wiki");
        create(&dir, "Realms", &repos(), "2026-09-25").unwrap();
        for (path, _) in TEMPLATE {
            assert!(dir.join(path).exists(), "{path}");
        }
        assert!(dir.join(".git").exists());
        assert!(fs::read_to_string(dir.join("CLAUDE.md")).unwrap().starts_with("# Realms-Wiki"));
        assert!(create(&dir, "Realms", &repos(), "2026-09-25").is_err(), "never writes over a folder with files");
    }
}
