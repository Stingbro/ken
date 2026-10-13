//! The knowledge base's settings, `.ken/knowledge.json`, as each source it
//! covers reads them (knowledge-core decisions 17 and 19).
//!
//! One knowledge base holds the settings for every source (decision 8): its
//! `repos` entry for a code repo carries that repo's role rules, and its
//! `controls` list names the source each control belongs to. Ken never
//! writes into a code repo (decision 36), so a code repo finds its settings
//! in the knowledge base that lists it: a registered repo holding
//! `.ken/knowledge.json` whose `repos` names it. A repo holding the file
//! itself (the knowledge base, or a repo that keeps its own) reads its own.
//!
//! ```json
//! "repos": {
//!   "Game": { "kind": "code", "base": "main",
//!             "roles": { "src/net/**": "protocol", "**/*System.java": "system" } }
//! },
//! "controls": [
//!   { "question": "how are worlds saved", "expect": "src/world/Save.java", "source": "Game" }
//! ]
//! ```

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::controls::Control;
use crate::wikinew::KNOWLEDGE_FILE;
use crate::{Error, Result};

/// The settings a source reads, and where they came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The repo whose `.ken/knowledge.json` this is.
    pub kb_root: PathBuf,
    /// The source's name, as the file's `repos` and `controls` name it.
    pub name: String,
    /// Whether the file sits in the source itself.
    pub own: bool,
    pub value: Value,
}

impl Settings {
    /// The source's entry in `repos`, its name matched ignoring case.
    pub fn repo_entry(&self) -> Option<&Value> {
        let repos = self.value.get("repos")?.as_object()?;
        repos.iter().find(|(k, _)| k.eq_ignore_ascii_case(&self.name)).map(|(_, v)| v)
    }

    /// The source's role rules: `repos.<name>.roles`, each a gitignore-style
    /// pattern and a role, as written.
    pub fn role_rules(&self) -> Vec<(String, String)> {
        let Some(roles) = self.repo_entry().and_then(|e| e.get("roles")).and_then(|r| r.as_object()) else {
            return Vec::new();
        };
        roles
            .iter()
            .filter_map(|(pattern, role)| {
                let role = role.as_str()?.trim();
                (!pattern.trim().is_empty() && !role.is_empty()).then(|| (pattern.trim().to_string(), role.to_string()))
            })
            .collect()
    }

    /// The controls for this source: the ones that name it, and in the
    /// file's own repo also the ones that name no source.
    pub fn controls(&self) -> Vec<Control> {
        let Some(list) = self.value.get("controls").and_then(|c| c.as_array()) else {
            return Vec::new();
        };
        list.iter()
            .filter_map(|c| serde_json::from_value::<Control>(c.clone()).ok())
            .filter(|c| !c.question.trim().is_empty() && !c.expect.trim().is_empty())
            .filter(|c| match &c.source {
                Some(s) => s.eq_ignore_ascii_case(&self.name),
                None => self.own,
            })
            .collect()
    }

    /// Whether the repo analysis has been saved for this source: its entry
    /// has `roles`, or a control names it.
    pub fn analyzed(&self) -> bool {
        self.repo_entry().is_some_and(|e| e.get("roles").is_some())
            || self
                .value
                .get("controls")
                .and_then(|c| c.as_array())
                .is_some_and(|l| l.iter().any(|c| c.get("source").and_then(|s| s.as_str()).is_some_and(|s| s.eq_ignore_ascii_case(&self.name))))
    }
}

/// The settings the repo at `root` reads: its own file when it has one,
/// else the file of a registered repo that lists it. None when neither.
pub fn for_repo(root: &Path) -> Option<Settings> {
    let name = repo_name(root);
    if let Some(value) = read(root) {
        return Some(Settings { kb_root: root.to_path_buf(), name, own: true, value });
    }
    let registry = crate::registry::default_base_dir().and_then(|b| crate::registry::Registry::load(&b)).ok()?;
    let candidates: Vec<PathBuf> = registry.projects.iter().map(|e| e.path.clone()).collect();
    among(root, &name, &candidates)
}

/// [`for_repo`]'s search, over the given repos: the one whose settings list
/// `name` (or the folder's own name) in `repos`. Where several do, the one
/// nearest `root` on disk (the longest shared path) wins, so two workspaces
/// that each list a repo called `Game` keep to their own.
pub fn among(root: &Path, name: &str, candidates: &[PathBuf]) -> Option<Settings> {
    let folder = folder_name(root);
    let mut best: Option<(usize, Settings)> = None;
    for kb in candidates {
        if same_path(kb, root) {
            continue;
        }
        let Some(value) = read(kb) else { continue };
        let Some(repos) = value.get("repos").and_then(|r| r.as_object()) else { continue };
        let Some(key) = repos.keys().find(|k| k.eq_ignore_ascii_case(name)).or_else(|| repos.keys().find(|k| k.eq_ignore_ascii_case(&folder)))
        else {
            continue;
        };
        let shared = shared_prefix(kb, root);
        if best.as_ref().is_none_or(|(n, _)| shared > *n) {
            let name = key.clone();
            best = Some((shared, Settings { kb_root: kb.clone(), name, own: false, value }));
        }
    }
    best.map(|(_, s)| s)
}

/// A source's name: what the registry calls it, else its folder's name.
pub fn repo_name(root: &Path) -> String {
    crate::registry::default_base_dir()
        .and_then(|b| crate::registry::Registry::load(&b))
        .ok()
        .and_then(|reg| reg.entry_at(root).map(|e| e.name.clone()))
        .unwrap_or_else(|| folder_name(root))
}

fn folder_name(root: &Path) -> String {
    root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn read(root: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(root.join(KNOWLEDGE_FILE)).ok()?;
    serde_json::from_str::<Value>(&text).ok().filter(|v| v.is_object())
}

fn same_path(a: &Path, b: &Path) -> bool {
    let c = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    c(a) == c(b)
}

/// How many leading path components `a` and `b` share.
fn shared_prefix(a: &Path, b: &Path) -> usize {
    a.components().zip(b.components()).take_while(|(x, y)| x == y).count()
}

/// Save what the repo analysis proposed for the source at `root`: its role
/// rules under its `repos` entry and its controls in `controls`, each only
/// when the file has none for it yet, so a person's edits are never
/// overwritten. The entry is made when the file does not list the source.
/// Returns whether the file changed; false when there is no settings file
/// to write to.
pub fn save_proposals(root: &Path, roles: &[(String, String)], controls: &[Control]) -> Result<bool> {
    let Some(settings) = for_repo(root) else {
        return Ok(false);
    };
    let Some(value) = with_proposals(&settings, roles, controls) else {
        return Ok(false);
    };
    let path = settings.kb_root.join(KNOWLEDGE_FILE);
    let text = serde_json::to_string_pretty(&value).map_err(|e| Error::Other(e.to_string()))?;
    std::fs::write(&path, text + "\n").map_err(|e| Error::io(&path, e))?;
    Ok(true)
}

/// [`save_proposals`]'s change to the settings, None when nothing changes.
pub fn with_proposals(settings: &Settings, roles: &[(String, String)], controls: &[Control]) -> Option<Value> {
    let mut value = settings.value.clone();
    let obj = value.as_object_mut()?;
    let mut changed = false;
    if !roles.is_empty() {
        let repos = obj.entry("repos").or_insert_with(|| Value::Object(Default::default()));
        if let Some(repos) = repos.as_object_mut() {
            let key = repos.keys().find(|k| k.eq_ignore_ascii_case(&settings.name)).cloned().unwrap_or_else(|| settings.name.clone());
            let entry = repos.entry(key).or_insert_with(|| Value::Object(Default::default()));
            if let Some(entry) = entry.as_object_mut().filter(|e| !e.contains_key("roles")) {
                let rules: serde_json::Map<String, Value> = roles.iter().map(|(p, r)| (p.clone(), Value::String(r.clone()))).collect();
                entry.insert("roles".into(), Value::Object(rules));
                changed = true;
            }
        }
    }
    if !controls.is_empty() && settings.controls().is_empty() {
        let list = obj.entry("controls").or_insert_with(|| Value::Array(Vec::new()));
        if let Some(list) = list.as_array_mut() {
            for c in controls {
                let c = Control { source: Some(settings.name.clone()), ..c.clone() };
                list.push(serde_json::to_value(c).ok()?);
            }
            changed = true;
        }
    }
    changed.then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kb(dir: &Path, json: &str) {
        std::fs::create_dir_all(dir.join(".ken")).unwrap();
        std::fs::write(dir.join(KNOWLEDGE_FILE), json).unwrap();
    }

    #[test]
    fn a_code_repo_reads_the_knowledge_base_that_lists_it() {
        let d = tempfile::tempdir().unwrap();
        let (wiki, game, other) = (d.path().join("Wiki"), d.path().join("Game"), d.path().join("Elsewhere"));
        std::fs::create_dir_all(&game).unwrap();
        kb(
            &wiki,
            r#"{ "repos": { "game": { "kind": "code", "roles": { "src/net/**": "protocol", "bad": "" } } },
                 "controls": [ { "question": "how are worlds saved", "expect": "src/Save.java", "source": "Game" },
                               { "question": "who rules", "expect": "Decisions/DECISIONS.md" } ] }"#,
        );
        kb(&other, r#"{ "repos": {} }"#);
        let s = among(&game, "Game", &[other.clone(), wiki.clone()]).unwrap();
        assert_eq!(s.kb_root, wiki);
        assert!(!s.own);
        assert_eq!(s.role_rules(), vec![("src/net/**".to_string(), "protocol".to_string())]);
        assert_eq!(s.controls().len(), 1, "only its own; one with no source is the knowledge base's");
        assert!(s.analyzed());
        assert!(among(&other, "Elsewhere", &[wiki.clone()]).is_none(), "not listed");
    }

    #[test]
    fn proposals_fill_what_is_missing_and_never_overwrite() {
        let d = tempfile::tempdir().unwrap();
        let wiki = d.path().join("Wiki");
        kb(&wiki, r#"{ "repos": { "Wiki": { "kind": ["wiki", "team"] } } }"#);
        let s = among(&d.path().join("Tools"), "Tools", &[wiki.clone()]);
        assert!(s.is_none(), "a repo the file does not list finds nothing");

        let s = Settings { kb_root: wiki.clone(), name: "Tools".into(), own: false, value: read(&wiki).unwrap() };
        assert!(!s.analyzed());
        let roles = vec![("cli/**".to_string(), "command".to_string())];
        let controls = vec![Control { question: "how is a release cut".into(), expect: "cli/release.rs".into(), source: None }];
        let v = with_proposals(&s, &roles, &controls).unwrap();
        assert_eq!(v["repos"]["Tools"]["roles"]["cli/**"], "command");
        assert_eq!(v["controls"][0]["source"], "Tools");
        let again = Settings { value: v, ..s };
        assert!(again.analyzed());
        let other = vec![("x/**".to_string(), "y".to_string())];
        assert!(with_proposals(&again, &other, &controls).is_none(), "a person's rules and controls stay");
    }
}
