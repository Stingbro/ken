//! A new team repo, laid down at set-up from the method's own template
//! (Ways-of-Working `templates/team`, bundled into Ken like the wiki's): the
//! tickets, the decisions log, the Ideas list, people, the method and the
//! team's overlay.
//!
//! Only what Ken knows is filled in: the team's name, its ticket key, the
//! names of the team repo and the wiki, and the repos set-up registered with
//! their kinds. Every other `{{…}}` is left for a person. The folder becomes a git repo with one commit; publishing it is a
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

/// A ticket key from the team's name: the initials of its words when it has
/// two or more (`Shattered-Realms` → SR), else its first six letters and
/// digits; in upper case, TEAM when it has none. The team can change it in
/// team.json. Six letters made SHATTE on 2026-10-06, a second ticket series
/// beside the team's real `SR-1174`.
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

/// The span of the JSON object or array that opens at byte `open`, to just
/// past its closing bracket; strings are skipped whole, so a `{{…}}` inside
/// a key never counts.
fn json_span(text: &str, open: usize) -> Option<usize> {
    let (mut depth, mut in_str, mut esc) = (0usize, false, false);
    for (i, c) in text.bytes().enumerate().skip(open) {
        if in_str {
            match c {
                _ if esc => esc = false,
                b'\\' => esc = true,
                b'"' => in_str = false,
                _ => {}
            }
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// The entries of the JSON object at `text[open..end]`, as (key, the value's
/// text as written).
fn json_entries(text: &str, open: usize, end: usize) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut i = open + 1;
    let b = text.as_bytes();
    while i < end {
        // The key: the next string.
        let Some(k0) = text[i..end].find('"').map(|j| i + j) else { break };
        let Some(k1) = text[k0 + 1..end].find('"').map(|j| k0 + 1 + j) else { break };
        let key = text[k0 + 1..k1].to_string();
        let Some(colon) = text[k1..end].find(':').map(|j| k1 + j) else { break };
        let mut v = colon + 1;
        while v < end && b[v].is_ascii_whitespace() {
            v += 1;
        }
        let v_end = match b.get(v) {
            Some(b'{') | Some(b'[') => json_span(text, v).unwrap_or(end),
            Some(b'"') => text[v + 1..end].find('"').map_or(end, |j| v + 2 + j),
            _ => text[v..end].find([',', '}']).map_or(end, |j| v + j),
        };
        out.push((key, text[v..v_end].to_string()));
        i = v_end;
    }
    out
}

/// What git says about a repo Ken registered: its branch (None when it is on
/// no branch), its HEAD, its `origin` URL.
fn repo_facts(dir: &Path) -> (Option<String>, Option<String>, Option<String>) {
    let git = |args: &[&str]| {
        let mut cmd = std::process::Command::new("git");
        let out = crate::proc::quiet(&mut cmd).args(args).current_dir(dir).output().ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (out.status.success() && !s.is_empty()).then_some(s)
    };
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]).filter(|b| b != "HEAD");
    (branch, git(&["rev-parse", "HEAD"]), crate::setup::origin_url(dir))
}

/// One repo's entry for team.json's `repos`, indented as the template is.
fn repo_entry(name: &str, fields: &[(&str, serde_json::Value)]) -> String {
    let q = |s: &str| serde_json::to_string(s).unwrap_or_default();
    let body: Vec<String> = fields.iter().map(|(k, v)| format!("      {}: {}", q(k), v)).collect();
    format!("    {}: {{\n{}\n    }}", q(name), body.join(",\n"))
}

/// team.json's `repos` with what Ken knows, one entry per repo: the team repo,
/// as `["team", "wiki"]` when it is the wiki too (on 2026-10-06 the docs repo
/// was written twice, as team then as wiki, and a JSON object keeps one);
/// the wiki; and each repo set-up registered, under its own name, with its
/// kind, the branch it is on (`main` when Ken cannot tell), and for a
/// reference repo its commit and `origin`. The template's sample code and
/// reference entries stay, placeholders and all, only while Ken knows no
/// repo of that kind.
fn fill_repos(text: &str, team_repo: &str, wiki: Option<&str>, repos: &[crate::wikinew::Covered]) -> String {
    use crate::registry::RepoKind;
    use serde_json::{json, Value};
    let Some(key_at) = text.find("\"repos\"") else { return text.to_string() };
    let Some(open) = text[key_at..].find('{').map(|i| key_at + i) else { return text.to_string() };
    let Some(end) = json_span(text, open) else { return text.to_string() };
    let sample = json_entries(text, open, end);
    let sample_of = |key: &str| sample.iter().find(|(k, _)| k == key).map(|(k, v)| format!("    \"{k}\": {v}"));
    let mut entries: Vec<String> = Vec::new();
    let both = wiki == Some(team_repo);
    let team_kind = if both { json!(["team", "wiki"]) } else { json!("team") };
    entries.push(repo_entry(team_repo, &[("kind", team_kind), ("base", json!("main"))]));
    match wiki {
        Some(w) if !both => entries.push(repo_entry(w, &[("kind", json!("wiki")), ("base", json!("main"))])),
        Some(_) => {}
        None => entries.extend(sample_of("{{wiki_repo}}")),
    }
    let kind_name = |k: &RepoKind| serde_json::to_value(k).unwrap_or(Value::Null);
    for r in repos.iter().filter(|r| r.name != team_repo && Some(r.name.as_str()) != wiki) {
        let kinds: Vec<Value> = r.kind.iter().map(kind_name).collect();
        let kind = match kinds.as_slice() {
            [] => json!("{{code or reference}}"),
            [one] => one.clone(),
            _ => Value::Array(kinds),
        };
        let (branch, head, origin) = r.path.as_deref().map(repo_facts).unwrap_or_default();
        let mut fields = vec![("kind", kind), ("base", json!(branch.unwrap_or_else(|| "main".into())))];
        if r.kind.contains(&RepoKind::Reference) {
            fields.extend(head.map(|h| ("pin", json!(h))));
            fields.extend(origin.map(|o| ("upstream", json!(o))));
        }
        entries.push(repo_entry(&r.name, &fields));
    }
    for (sample_key, kind) in [("{{code_repo}}", RepoKind::Code), ("{{reference_repo}}", RepoKind::Reference)] {
        if !repos.iter().any(|r| r.kind.contains(&kind)) {
            entries.extend(sample_of(sample_key));
        }
    }
    format!("{}{{\n{}\n  }}{}", &text[..open], entries.join(",\n"), &text[end..])
}

/// One template file with what Ken knows filled in. Copy templates (a
/// ticket, a person, an idea) keep their placeholders, the key aside.
/// `repos` are the team's other repos as set-up registered them.
pub fn fill(
    path: &str,
    text: &str,
    team: &str,
    team_repo: &str,
    wiki: Option<&str>,
    repos: &[crate::wikinew::Covered],
) -> String {
    let key = ticket_key(team);
    let text = if path == ".wright/team.json" { fill_repos(text, team_repo, wiki, repos) } else { text.to_string() };
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
/// anything, so it never writes over a person's files. `repos` are the
/// team's other repos, for team.json.
pub fn create(dir: &Path, team: &str, wiki: Option<&str>, repos: &[crate::wikinew::Covered]) -> Result<()> {
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
        fs::write(&dest, fill(path, &text, team, &name, wiki, repos)).map_err(|e| Error::io(&dest, e))?;
    }
    crate::wikinew::git_commit_all(dir, &format!("Start the {team} team repo from the Ways-of-Working template"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::registry::RepoKind;
    use crate::wikinew::Covered;
    use std::path::PathBuf;

    #[test]
    fn the_key_comes_from_the_team_name() {
        assert_eq!(ticket_key("AUR"), "AUR");
        assert_eq!(ticket_key("Shattered-Realms"), "SR", "the initials of a name of several words");
        assert_eq!(ticket_key("Realms team"), "RT");
        assert_eq!(ticket_key("Payments"), "PAYMEN", "one word: its first six letters");
        assert_eq!(ticket_key("—"), "TEAM");
    }

    fn team_json(team: &str, team_repo: &str, wiki: Option<&str>, repos: &[Covered]) -> serde_json::Value {
        let json = TEMPLATE.iter().find(|(p, _)| *p == ".wright/team.json").unwrap().1.replace("\r\n", "\n");
        let t = fill(".wright/team.json", &json, team, team_repo, wiki, repos);
        serde_json::from_str(&t).unwrap_or_else(|e| panic!("{e}\n{t}"))
    }

    #[test]
    fn fill_names_the_team_and_repos_and_keeps_copy_templates() {
        let readme = TEMPLATE.iter().find(|(p, _)| *p == "README.md").unwrap().1;
        assert!(fill("README.md", readme, "AUR", "AUR-Team", Some("AUR-Wiki"), &[]).starts_with("# AUR — team repo"));
        let json = TEMPLATE.iter().find(|(p, _)| *p == ".wright/team.json").unwrap().1;
        let t = fill(".wright/team.json", json, "AUR", "AUR-Team", Some("AUR-Wiki"), &[]);
        assert!(t.contains("\"team\": \"AUR\"") && t.contains("\"key\": \"AUR\"") && t.contains("\"AUR-Team\": {") && t.contains("\"AUR-Wiki\": {"));
        let ticket = TEMPLATE.iter().find(|(p, _)| *p == "tickets/TICKET.md").unwrap().1;
        let t = fill("tickets/TICKET.md", ticket, "AUR", "AUR-Team", None, &[]);
        assert!(t.contains("id: AUR-001") && t.contains("{{type}}"), "a copy template keeps its placeholders");
    }

    /// The 2026-10-06 dry run: the docs repo is the team repo too, three
    /// repos registered (two code, one reference).
    #[test]
    fn team_json_names_each_registered_repo_once_with_its_kind() {
        let d = tempfile::tempdir().unwrap();
        let mod_dir = d.path().join("Shattered-Realms");
        fs::create_dir_all(&mod_dir).unwrap();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(args)
                .current_dir(&mod_dir)
                .output()
                .unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        };
        git(&["init", "-q", "-b", "dev"]);
        git(&["commit", "-q", "--allow-empty", "-m", "first"]);
        let covered = |name: &str, kind: RepoKind, path: Option<PathBuf>| Covered {
            name: name.into(),
            description: String::new(),
            kind: vec![kind],
            path,
        };
        let repos = vec![
            covered("Shattered-Realms", RepoKind::Code, Some(mod_dir.clone())),
            covered("Shattered-Realms-Tools", RepoKind::Code, None),
            covered("hytale-shared-source", RepoKind::Reference, None),
        ];
        let wiki = "Shattered-Realms-Wiki";
        let v = team_json("Shattered-Realms", wiki, Some(wiki), &repos);
        assert_eq!(v["key"], "SR");
        let r = v["repos"].as_object().unwrap();
        let mut names: Vec<&String> = r.keys().collect();
        names.sort();
        assert_eq!(names, vec!["Shattered-Realms", "Shattered-Realms-Tools", "Shattered-Realms-Wiki", "hytale-shared-source"]);
        assert_eq!(r[wiki]["kind"], serde_json::json!(["team", "wiki"]), "one entry of both kinds, not two");
        assert_eq!(r["Shattered-Realms"]["kind"], "code");
        assert_eq!(r["Shattered-Realms"]["base"], "dev", "the branch the checkout is on");
        assert_eq!(r["Shattered-Realms-Tools"]["base"], "main", "main when Ken cannot tell");
        assert_eq!(r["hytale-shared-source"]["kind"], "reference");
        let text = serde_json::to_string(&v["repos"]).unwrap();
        assert!(!text.contains("{{"), "no sample entry is left once each kind is known: {text}");

        // Nothing registered: the samples stay for a person to fill.
        let v = team_json("AUR", "AUR-Team", Some("AUR-Wiki"), &[]);
        let r = v["repos"].as_object().unwrap();
        assert!(r.contains_key("{{code_repo}}") && r.contains_key("{{reference_repo}}"));
        assert_eq!(r["AUR-Team"]["kind"], "team");
        assert_eq!(r["AUR-Wiki"]["kind"], "wiki");
    }

    #[test]
    fn create_lays_the_template_down_as_a_repo_and_refuses_a_full_folder() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("AUR-Team");
        create(&dir, "AUR", Some("AUR-Wiki"), &[]).unwrap();
        for (path, _) in TEMPLATE {
            assert!(dir.join(path).exists(), "{path}");
        }
        assert!(dir.join(".git").exists());
        assert!(create(&dir, "AUR", None, &[]).is_err(), "never writes over a folder with files");
    }
}
