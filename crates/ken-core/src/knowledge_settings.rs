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

    /// The source's index rules (knowledge-core decision 34): the file's
    /// top-level `ignore` lines, which hold for every source, then its
    /// `repos.<name>.ignore` lines. Gitignore-style, one pattern a line: a
    /// bare line is not indexed, a `!` line brings a path back.
    pub fn index_rules(&self) -> Vec<String> {
        let lines = |v: Option<&Value>| -> Vec<String> {
            v.and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|l| l.as_str()).map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect())
                .unwrap_or_default()
        };
        let mut out = lines(self.value.get("ignore"));
        out.extend(lines(self.repo_entry().and_then(|e| e.get("ignore"))));
        out
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

/// The knowledge base's own top-level `ignore` lines, the rules every source
/// reads (decision 34). Empty when the file has none or is missing.
pub fn ignore_lines(kb_root: &Path) -> Vec<String> {
    read(kb_root)
        .map(|value| Settings { kb_root: kb_root.to_path_buf(), name: String::new(), own: true, value })
        .map(|s| {
            s.value
                .get("ignore")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|l| l.as_str()).map(str::to_string).collect())
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// Write the knowledge base's top-level `ignore` lines, blank lines dropped,
/// keeping every other key as it was. The file must exist: a knowledge base
/// has one from its template.
pub fn set_ignore_lines(kb_root: &Path, lines: &[String]) -> Result<()> {
    let path = kb_root.join(KNOWLEDGE_FILE);
    let mut value = read(kb_root).ok_or_else(|| Error::Other(format!("{} has no readable {KNOWLEDGE_FILE}", kb_root.display())))?;
    let kept: Vec<Value> = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).map(|l| Value::String(l.to_string())).collect();
    if let Some(obj) = value.as_object_mut() {
        obj.insert("ignore".into(), Value::Array(kept));
    }
    let text = serde_json::to_string_pretty(&value).map_err(|e| Error::Other(e.to_string()))?;
    std::fs::write(&path, text + "\n").map_err(|e| Error::io(&path, e))
}

/// Write `value` into the knowledge base's file, every other key kept.
fn write(kb_root: &Path, value: &Value) -> Result<()> {
    let path = kb_root.join(KNOWLEDGE_FILE);
    let text = serde_json::to_string_pretty(value).map_err(|e| Error::Other(e.to_string()))?;
    std::fs::write(&path, text + "\n").map_err(|e| Error::io(&path, e))
}

/// The file at `kb_root`, or why there is none to edit.
fn editable(kb_root: &Path) -> Result<Value> {
    read(kb_root).ok_or_else(|| Error::Other(format!("{} has no readable {KNOWLEDGE_FILE}", kb_root.display())))
}

/// One `pattern = value` line, as the workspace settings screen edits tiers
/// and roles: split at the last `=`, both sides trimmed. None for a blank
/// line or one without both sides.
pub fn parse_pair(line: &str) -> Option<(String, String)> {
    let (pattern, value) = line.rsplit_once('=')?;
    let (pattern, value) = (pattern.trim(), value.trim());
    (!pattern.is_empty() && !value.is_empty()).then(|| (pattern.to_string(), value.to_string()))
}

/// An object of pattern to value as `pattern = value` lines.
fn pair_lines(map: Option<&Value>) -> Vec<String> {
    map.and_then(|m| m.as_object())
        .map(|m| {
            m.iter()
                .filter_map(|(p, v)| match v {
                    Value::String(s) => Some(format!("{p} = {s}")),
                    Value::Number(n) => Some(format!("{p} = {n}")),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The tier overrides (`search.weights`, decision 23) as `pattern = tier`
/// lines; a value is a tier name or a number added to a hit's score.
pub fn weight_lines(kb_root: &Path) -> Vec<String> {
    pair_lines(read(kb_root).as_ref().and_then(|v| v.pointer("/search/weights")))
}

/// Write `search.weights` from `pattern = value` lines: a tier name
/// (binding, current, reference, record, superseded) or a number. A line
/// that is neither is refused, with the line said, and nothing is written.
pub fn set_weight_lines(kb_root: &Path, lines: &[String]) -> Result<()> {
    let mut weights = serde_json::Map::new();
    for line in lines.iter().filter(|l| !l.trim().is_empty()) {
        let (pattern, value) = parse_pair(line).ok_or_else(|| Error::Other(format!("\"{}\" is not pattern = tier", line.trim())))?;
        let v = if let Some(t) = crate::authority::Tier::parse(&value) {
            Value::String(t.name().to_string())
        } else if let Some(n) = value.parse::<f64>().ok().and_then(serde_json::Number::from_f64) {
            Value::Number(n)
        } else {
            return Err(Error::Other(format!("\"{value}\" is not a tier (binding, current, reference, record, superseded) or a number")));
        };
        weights.insert(pattern, v);
    }
    let mut value = editable(kb_root)?;
    let obj = value.as_object_mut().ok_or_else(|| Error::Other("the settings are not an object".into()))?;
    let search = obj.entry("search").or_insert_with(|| Value::Object(Default::default()));
    if !search.is_object() {
        *search = Value::Object(Default::default());
    }
    if let Some(s) = search.as_object_mut() {
        s.insert("weights".into(), Value::Object(weights));
    }
    write(kb_root, &value)
}

/// A source's role rules (decision 17) as `pattern = role` lines, from its
/// entry in the knowledge base's `repos`.
pub fn role_lines(kb_root: &Path, repo: &str) -> Vec<String> {
    let value = read(kb_root);
    let entry = value.as_ref().and_then(|v| v.get("repos")).and_then(|r| r.as_object()).and_then(|r| {
        r.iter().find(|(k, _)| k.eq_ignore_ascii_case(repo)).map(|(_, e)| e)
    });
    pair_lines(entry.and_then(|e| e.get("roles")))
}

/// Write a source's role rules from `pattern = role` lines, roles in lower
/// case, under its `repos` entry (made when the file does not list it).
pub fn set_role_lines(kb_root: &Path, repo: &str, lines: &[String]) -> Result<()> {
    let mut roles = serde_json::Map::new();
    for line in lines.iter().filter(|l| !l.trim().is_empty()) {
        let (pattern, role) = parse_pair(line).ok_or_else(|| Error::Other(format!("\"{}\" is not pattern = role", line.trim())))?;
        roles.insert(pattern, Value::String(role.to_lowercase()));
    }
    let mut value = editable(kb_root)?;
    let obj = value.as_object_mut().ok_or_else(|| Error::Other("the settings are not an object".into()))?;
    let repos = obj.entry("repos").or_insert_with(|| Value::Object(Default::default()));
    let repos = repos.as_object_mut().ok_or_else(|| Error::Other("repos is not an object".into()))?;
    let key = repos.keys().find(|k| k.eq_ignore_ascii_case(repo)).cloned().unwrap_or_else(|| repo.to_string());
    let entry = repos.entry(key).or_insert_with(|| Value::Object(Default::default()));
    let entry = entry.as_object_mut().ok_or_else(|| Error::Other(format!("the {repo} entry is not an object")))?;
    entry.insert("roles".into(), Value::Object(roles));
    write(kb_root, &value)
}

/// Every control in the knowledge base's file, in its order.
pub fn all_controls(kb_root: &Path) -> Vec<Control> {
    read(kb_root)
        .and_then(|v| v.get("controls").cloned())
        .and_then(|c| serde_json::from_value::<Vec<Value>>(c).ok())
        .map(|list| list.into_iter().filter_map(|c| serde_json::from_value::<Control>(c).ok()).collect())
        .unwrap_or_default()
}

/// Write the knowledge base's `controls`: those with a question and a file,
/// trimmed; a blank source means the knowledge base itself.
pub fn set_controls(kb_root: &Path, controls: &[Control]) -> Result<()> {
    let list: Vec<Value> = controls
        .iter()
        .filter(|c| !c.question.trim().is_empty() && !c.expect.trim().is_empty())
        .map(|c| Control {
            question: c.question.trim().to_string(),
            expect: c.expect.trim().replace('\\', "/"),
            source: c.source.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
        })
        .filter_map(|c| serde_json::to_value(c).ok())
        .collect();
    let mut value = editable(kb_root)?;
    let obj = value.as_object_mut().ok_or_else(|| Error::Other("the settings are not an object".into()))?;
    obj.insert("controls".into(), Value::Array(list));
    write(kb_root, &value)
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
    fn index_rules_are_the_files_lines_then_the_sources() {
        let d = tempfile::tempdir().unwrap();
        let (wiki, game) = (d.path().join("Wiki"), d.path().join("Game"));
        kb(&wiki, r#"{ "ignore": ["*.log", " "], "repos": { "Game": { "kind": "code", "ignore": ["assets/raw/", "!tests/"] } } }"#);
        let s = among(&game, "Game", &[wiki.clone()]).unwrap();
        assert_eq!(s.index_rules(), vec!["*.log", "assets/raw/", "!tests/"]);
        assert_eq!(ignore_lines(&wiki), vec!["*.log", " "]);
        set_ignore_lines(&wiki, &["build-cache/".into(), "".into()]).unwrap();
        assert_eq!(ignore_lines(&wiki), vec!["build-cache/"]);
        assert_eq!(read(&wiki).unwrap()["repos"]["Game"]["kind"], "code", "the rest of the file stays");
        assert!(set_ignore_lines(&d.path().join("None"), &[]).is_err());
    }

    /// The workspace settings screen's editors: tiers, roles and controls,
    /// each written back with every other key kept.
    #[test]
    fn tiers_roles_and_controls_are_edited_in_place() {
        let d = tempfile::tempdir().unwrap();
        let wiki = d.path().join("Wiki");
        kb(&wiki, r#"{ "key": "SR", "repos": { "Game": { "kind": "code", "base": "main" } }, "search": { "weights": { "Research/": "record" } } }"#);
        assert_eq!(parse_pair(" src/net/** = protocol "), Some(("src/net/**".into(), "protocol".into())));
        assert_eq!(parse_pair("no value ="), None);
        assert_eq!(parse_pair(""), None);

        assert_eq!(weight_lines(&wiki), vec!["Research/ = record"]);
        set_weight_lines(&wiki, &["Rules/ = binding".into(), "".into(), "Old/ = -0.5".into(), "Research/ = Record".into()]).unwrap();
        let v = read(&wiki).unwrap();
        assert_eq!(v["search"]["weights"]["Rules/"], "binding");
        assert_eq!(v["search"]["weights"]["Old/"], -0.5);
        assert_eq!(v["search"]["weights"]["Research/"], "record", "a tier is written by its name");
        assert_eq!(v["key"], "SR", "the rest of the file stays");
        assert!(set_weight_lines(&wiki, &["Rules/ = very".into()]).is_err(), "neither a tier nor a number");
        assert_eq!(read(&wiki).unwrap()["search"]["weights"]["Rules/"], "binding", "a refused edit writes nothing");

        set_role_lines(&wiki, "game", &["src/net/** = Protocol".into()]).unwrap();
        assert_eq!(role_lines(&wiki, "Game"), vec!["src/net/** = protocol"]);
        assert_eq!(read(&wiki).unwrap()["repos"]["Game"]["base"], "main", "the entry keeps its other keys");
        set_role_lines(&wiki, "Tools", &["cli/** = command".into()]).unwrap();
        assert_eq!(role_lines(&wiki, "Tools"), vec!["cli/** = command"], "an entry is made for a source the file lacks");
        assert!(set_role_lines(&wiki, "Game", &["just words".into()]).is_err());

        let c = |q: &str, e: &str, s: Option<&str>| Control { question: q.into(), expect: e.into(), source: s.map(str::to_string) };
        set_controls(&wiki, &[c(" how are worlds saved ", "src\\world\\Save.java", Some("Game")), c("", "x.md", None), c("where do rules live", "Rules/Index.md", Some(" "))]).unwrap();
        assert_eq!(all_controls(&wiki), vec![c("how are worlds saved", "src/world/Save.java", Some("Game")), c("where do rules live", "Rules/Index.md", None)]);
        assert!(set_controls(&d.path().join("None"), &[]).is_err(), "no settings file, nothing written");
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
