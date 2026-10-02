//! A new team repo, laid down at set-up from the method's own template
//! (Ways-of-Working `templates/team`, bundled into Ken like the wiki's): the
//! tickets, the decisions log, the Ideas list, people, the method and the
//! team's overlay.
//!
//! Only what Ken knows is filled in: the team's name, its ticket key, and the
//! names of the team repo and the wiki. Every other `{{…}}` is left for a
//! person. The folder becomes a git repo with one commit; publishing it is a
//! person's step, as for the wiki.

use std::fs;
use std::path::Path;

use crate::{Error, Result};

/// Which copy of the method's template is bundled.
pub const TEMPLATE_SOURCE: &str = "Stingbro/Ways-of-Working templates/team @ 1f625d7";

macro_rules! template {
    ($($path:literal),* $(,)?) => {
        &[$(($path, include_str!(concat!("../templates/team/", $path)))),*]
    };
}

/// Every file of the template: (path in the repo, text).
pub const TEMPLATE: &[(&str, &str)] = template![
    ".wright/team.json",
    "README.md",
    "decisions/DECISIONS.md",
    "ideas/IDEA.md",
    "ideas/README.md",
    "method/README.md",
    "people/PERSON.md",
    "people/README.md",
    "runs/README.md",
    "team/README.md",
    "tickets/TICKET.md",
];

/// A ticket key from the team's name: its first six letters and digits, in
/// upper case; TEAM when it has none. The team can change it in team.json.
pub fn ticket_key(team: &str) -> String {
    let k: String = team.chars().filter(|c| c.is_ascii_alphanumeric()).take(6).collect::<String>().to_uppercase();
    if k.is_empty() {
        "TEAM".into()
    } else {
        k
    }
}

/// One template file with what Ken knows filled in. Copy templates (a
/// ticket, a person, an idea) keep their placeholders, the key aside.
pub fn fill(path: &str, text: &str, team: &str, team_repo: &str, wiki: Option<&str>) -> String {
    let key = ticket_key(team);
    let mut t = text.replace("{{KEY}}", &key);
    let is_copy = path.ends_with("TICKET.md") || path.ends_with("PERSON.md") || path.ends_with("IDEA.md");
    if !is_copy {
        t = t.replace("{{team_name}}", team).replace("{{team_repo}}", team_repo);
        if let Some(w) = wiki {
            t = t.replace("{{wiki_repo}}", w);
        }
    }
    t
}

/// Lay a new team repo down at `dir` (missing or empty) for `team`, then
/// make it a git repo with one commit. Refuses a folder that already holds
/// anything, so it never writes over a person's files.
pub fn create(dir: &Path, team: &str, wiki: Option<&str>) -> Result<()> {
    if dir.exists() && fs::read_dir(dir).map_err(|e| Error::io(dir, e))?.next().is_some() {
        return Err(Error::Other(format!("{} is not empty; pick a new folder for the team repo", dir.display())));
    }
    let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "team".into());
    for (path, text) in TEMPLATE {
        let dest = dir.join(path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let text = text.replace("\r\n", "\n");
        fs::write(&dest, fill(path, &text, team, &name, wiki)).map_err(|e| Error::io(&dest, e))?;
    }
    crate::wikinew::git_commit_all(dir, &format!("Start the {team} team repo from the Ways-of-Working template"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_comes_from_the_team_name() {
        assert_eq!(ticket_key("AUR"), "AUR");
        assert_eq!(ticket_key("Realms team"), "REALMS");
        assert_eq!(ticket_key("—"), "TEAM");
    }

    #[test]
    fn fill_names_the_team_and_repos_and_keeps_copy_templates() {
        let readme = TEMPLATE.iter().find(|(p, _)| *p == "README.md").unwrap().1;
        assert!(fill("README.md", readme, "AUR", "AUR-Team", Some("AUR-Wiki")).starts_with("# AUR — team repo"));
        let json = TEMPLATE.iter().find(|(p, _)| *p == ".wright/team.json").unwrap().1;
        let t = fill(".wright/team.json", json, "AUR", "AUR-Team", Some("AUR-Wiki"));
        assert!(t.contains("\"team\": \"AUR\"") && t.contains("\"key\": \"AUR\"") && t.contains("\"AUR-Team\": {") && t.contains("\"AUR-Wiki\": {"));
        let ticket = TEMPLATE.iter().find(|(p, _)| *p == "tickets/TICKET.md").unwrap().1;
        let t = fill("tickets/TICKET.md", ticket, "AUR", "AUR-Team", None);
        assert!(t.contains("id: AUR-001") && t.contains("{{type}}"), "a copy template keeps its placeholders");
    }

    #[test]
    fn create_lays_the_template_down_as_a_repo_and_refuses_a_full_folder() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("AUR-Team");
        create(&dir, "AUR", Some("AUR-Wiki")).unwrap();
        for (path, _) in TEMPLATE {
            assert!(dir.join(path).exists(), "{path}");
        }
        assert!(dir.join(".git").exists());
        assert!(create(&dir, "AUR", None).is_err(), "never writes over a folder with files");
    }
}
