//! A new team wiki, laid down at set-up from the method's own template
//! (Ways-of-Working `templates/wiki`, bundled into Ken so set-up works
//! offline and every team starts from the same version).
//!
//! Only the facts Ken knows are filled in: the team's name, the wiki's own
//! name, the team's repos with what each is for, and `updated:` dates. Every
//! other `{{…}}` is left for a person or the first-wiki draft, and no
//! `verified:` date is ever stamped: only a person reads a page against its
//! sources. The folder becomes a git repo with one commit; publishing it
//! (a remote, a push) is a person's step.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Which copy of the method's template is bundled.
pub const TEMPLATE_SOURCE: &str = "Stingbro/Ways-of-Working templates/wiki @ b824813";

macro_rules! template {
    ($($path:literal),* $(,)?) => {
        &[$(($path, include_str!(concat!("../templates/wiki/", $path)))),*]
    };
}

/// Every file of the template: (path in the wiki, text).
pub const TEMPLATE: &[(&str, &str)] = template![
    "CLAUDE.md",
    "START-HERE.md",
    "Vocabulary.md",
    "_meta/README.md",
    "Conventions/ARCHITECTURE.md",
    "Conventions/CONVENTION.md",
    "Current/Index.md",
    "Current/Project.md",
    "Current/Team.md",
    "Current/Who-Does-What.md",
    "Research/Ingestion/Index.md",
    "Research/Ingestion/Ingested/.gitkeep",
    "Research/Ingestion/Raw/.gitkeep",
    "Templates/How-to.md",
    "Templates/Ingested-note.md",
    "Ways-of-Working/Agents/Index.md",
    "Ways-of-Working/Lifecycle.md",
    "Ways-of-Working/Research-Ladder.md",
    "Ways-of-Working/Rules.md",
    "Ways-of-Working/Rules/RULE.md",
    "Ways-of-Working/Rules/write-for-the-altitude-like-teammates-talking.md",
    "Ways-of-Working/Rules/write-like-you-would-say-it-out-loud.md",
    "Ways-of-Working/Ways-of-Working.md",
];

/// A repo the new wiki covers: its name in the workspace and what it is for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Covered {
    pub name: String,
    pub description: String,
}

/// Pages that are templates for people to copy keep their placeholders,
/// dates included.
fn is_copy_template(path: &str) -> bool {
    path.starts_with("Templates/") || path.ends_with("/RULE.md") || path.ends_with("/CONVENTION.md")
}

fn one_line(s: &str) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ").replace('|', "/");
    if s.is_empty() {
        "(not said yet)".into()
    } else {
        s
    }
}

/// One template file with what Ken knows filled in.
pub fn fill(path: &str, text: &str, team: &str, wiki: &str, repos: &[Covered], today: &str) -> String {
    let mut t = text.replace("{{team_name}}", team).replace("{{wiki_repo}}", wiki);
    if !is_copy_template(path) {
        // `updated:` only; a `verified:` date is a person's.
        t = t
            .lines()
            .map(|l| {
                if l.trim_start().starts_with("updated:") {
                    l.replacen("{{date}}", today, 1)
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + if text.ends_with('\n') { "\n" } else { "" };
    }
    if path == "START-HERE.md" {
        // The repo table: this wiki, then each of the team's repos.
        let mut rows = format!("| `{wiki}` | **this** — all documentation |\n");
        for r in repos {
            rows.push_str(&format!("| `{}` | {} |\n", r.name, one_line(&r.description)));
        }
        let mut out = String::new();
        let mut replaced = false;
        for line in t.lines() {
            let placeholder_row = line.starts_with("| `{{") || (line.starts_with(&format!("| `{wiki}`")) && !replaced);
            if placeholder_row {
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

/// Lay a new wiki down at `dir` (missing or empty) for `team`, covering
/// `repos`, then make it a git repo with one commit. Refuses a folder that
/// already holds anything, so it can never write over a person's files.
pub fn create(dir: &Path, team: &str, repos: &[Covered], today: &str) -> Result<()> {
    if dir.exists() && fs::read_dir(dir).map_err(|e| Error::io(dir, e))?.next().is_some() {
        return Err(Error::Other(format!("{} is not empty; pick a new folder for the wiki", dir.display())));
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
    git(dir, &["init", "-q"])?;
    git(dir, &["add", "-A"])?;
    // The person's own git identity when they have one; Ken's otherwise, so
    // a machine with no identity set can still make the first commit.
    let msg = format!("Start the {team} wiki from the Ways-of-Working template");
    if git(dir, &["commit", "-q", "-m", &msg]).is_err() {
        git(dir, &["-c", "user.name=Ken", "-c", "user.email=ken@localhost", "commit", "-q", "-m", &msg])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repos() -> Vec<Covered> {
        vec![
            Covered { name: "Game".into(), description: "The game client.\nBuilt nightly.".into() },
            Covered { name: "Tools".into(), description: String::new() },
        ]
    }

    #[test]
    fn start_here_lists_this_wiki_and_the_teams_repos() {
        let text = TEMPLATE.iter().find(|(p, _)| *p == "START-HERE.md").unwrap().1;
        let t = fill("START-HERE.md", text, "Realms", "Realms-Wiki", &repos(), "2026-09-25");
        assert!(t.contains("**The Realms knowledge library.**"));
        assert!(t.contains("| `Realms-Wiki` | **this** — all documentation |\n| `Game` | The game client. Built nightly. |\n| `Tools` | (not said yet) |"));
        assert!(!t.contains("{{team_repo}}") && !t.contains("{{code_repo}}"));
    }

    #[test]
    fn updated_is_dated_and_verified_never_is() {
        let text = "---\nupdated: {{date}}\nverified: {{date}}      # a person\n---\n";
        let t = fill("Current/Project.md", text, "T", "W", &[], "2026-09-25");
        assert_eq!(t, "---\nupdated: 2026-09-25\nverified: {{date}}      # a person\n---\n");
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
