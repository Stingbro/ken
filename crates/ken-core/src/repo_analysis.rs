//! The repo analysis (knowledge-core decisions 17 and 19): when a source is
//! added, one Claude call over its file list proposes its role rules (for a
//! code or reference repo) and a few controls (for every source). The
//! proposals are saved in the knowledge base's `.ken/knowledge.json`
//! (`knowledge_settings::save_proposals`), where a person edits them; they
//! are applied without a model at index time (`coderole`, `controls`).
//!
//! Claude reads the prompt and answers with no tools; the local model is
//! never asked. When Claude is not there, or the call fails, nothing is
//! saved and the built-in core roles apply; the next time a source is added
//! the analysis is tried again. A source whose analysis is saved is never
//! analysed again, so a person's edits stay.

use std::path::Path;

use serde_json::Value;

use crate::controls::Control;
use crate::registry::RepoKind;
use crate::Result;

/// Most files listed in the prompt; a bigger repo is sampled evenly across
/// its tree, so every folder shows.
pub const MAX_FILES: usize = 600;
/// Most entries the walk reads before it stops.
const WALK_LIMIT: usize = 20_000;
/// Most pages whose title is read for the prompt.
const MAX_TITLES: usize = 200;
/// Most role rules kept from one reply.
pub const MAX_ROLES: usize = 30;

/// What the analysis proposes for one source.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Analysis {
    pub roles: Vec<(String, String)>,
    pub controls: Vec<Control>,
}

/// Whether a source of these kinds gets role rules: code does, a knowledge
/// base's pages do not.
pub fn wants_roles(kinds: &[RepoKind]) -> bool {
    kinds.iter().any(|k| matches!(k, RepoKind::Code | RepoKind::Reference))
}

/// The files the analysis reads: the code and the Markdown pages Ken would
/// index (the repo's `.gitignore`, the built-in rules: no tests, archives,
/// generated files or templates), sorted, sampled down to [`MAX_FILES`].
pub fn file_list(root: &Path) -> Vec<String> {
    let built_in = crate::kenignore::built_in_rule_sets();
    let mut files: Vec<String> = ignore::WalkBuilder::new(root)
        .hidden(true)
        .parents(false)
        .git_global(false)
        .filter_entry(|e| e.file_name() != crate::project::CONFIG_DIR && e.file_name() != "node_modules")
        .build()
        .flatten()
        .take(WALK_LIMIT)
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|e| e.path().strip_prefix(root).ok().map(|p| p.to_string_lossy().replace('\\', "/")))
        .filter(|rel| crate::kenignore::classify(rel, false, &[&built_in]) != crate::kenignore::Tier::Ignored)
        .filter(|rel| crate::chunker::is_markdown(rel) || crate::contenttype::of(rel) == crate::contenttype::ContentType::Code)
        .collect();
    files.sort();
    sample(files, MAX_FILES)
}

/// At most `n` of `files`, taken evenly from the sorted list.
fn sample(files: Vec<String>, n: usize) -> Vec<String> {
    if files.len() <= n || n == 0 {
        return files;
    }
    let step = files.len() as f64 / n as f64;
    (0..n).filter_map(|i| files.get((i as f64 * step) as usize).cloned()).collect()
}

/// The prompt: the source, what to propose, and its files, each page with
/// its title when one is known.
pub fn prompt(name: &str, kinds: &[RepoKind], files: &[String], titles: &[(String, String)]) -> String {
    let kinds: Vec<&str> = kinds
        .iter()
        .map(|k| match k {
            RepoKind::Wiki => "wiki",
            RepoKind::Code => "code",
            RepoKind::Reference => "reference",
        })
        .collect();
    let mut p = format!(
        "You are setting up search for one source of a project's knowledge base: the repo \"{name}\" (kind: {}). \
         Below is a list of its files.\n\n",
        if kinds.is_empty() { "not said".to_string() } else { kinds.join(", ") }
    );
    if wants_roles_names(&kinds) {
        let core: Vec<&str> = crate::coderole::core_roles().collect();
        p.push_str(&format!(
            "1. Roles. Search labels each code file with its role in the repo. It already reads these roles from folder and \
             file names: {}. Propose rules for the roles this repo has that those names miss or get wrong, each a \
             gitignore-style pattern relative to the repo root and a role of one or two lowercase words \
             (for example \"src/net/**\": \"protocol\"). At most {MAX_ROLES} rules, and none when the built-in roles fit.\n",
            core.join(", ")
        ));
    } else {
        p.push_str("1. Roles. This source is not code: propose no roles.\n");
    }
    p.push_str(&format!(
        "2. Controls. Propose up to {} questions a person on the team would really ask that this repo answers, each \
         with the one file from the list that must come back for it. Pick questions with one clear answer file, in the \
         words a person would type rather than the file's name.\n\n\
         Reply with JSON only, in this shape:\n\
         {{\"roles\": {{\"pattern\": \"role\"}}, \"controls\": [{{\"question\": \"...\", \"expect\": \"path/from/the/list\"}}]}}\n\n\
         Files:\n",
        crate::controls::MAX_PROPOSED
    ));
    for f in files {
        match titles.iter().find(|(p, _)| p == f) {
            Some((_, t)) => p.push_str(&format!("{f} | {t}\n")),
            None => p.push_str(&format!("{f}\n")),
        }
    }
    p
}

fn wants_roles_names(kinds: &[&str]) -> bool {
    kinds.iter().any(|k| matches!(*k, "code" | "reference"))
}

/// What a reply proposes: the JSON object in it, read leniently. A role
/// rule needs a pattern and a short role; a control needs a question and a
/// file that is in `files`. Anything else is left out.
pub fn parse(reply: &str, files: &[String], roles_wanted: bool) -> Analysis {
    let (Some(start), Some(end)) = (reply.find('{'), reply.rfind('}')) else {
        return Analysis::default();
    };
    let Ok(v) = serde_json::from_str::<Value>(reply.get(start..=end).unwrap_or("")) else {
        return Analysis::default();
    };
    let mut out = Analysis::default();
    if roles_wanted {
        if let Some(roles) = v.get("roles").and_then(|r| r.as_object()) {
            for (pattern, role) in roles {
                let pattern = pattern.trim();
                let role = role.as_str().map(crate::coderole::normalize_role).unwrap_or_default();
                if pattern.is_empty() || pattern.len() > 200 || role.is_empty() || role.len() > 40 {
                    continue;
                }
                out.roles.push((pattern.to_string(), role));
                if out.roles.len() == MAX_ROLES {
                    break;
                }
            }
        }
    }
    if let Some(list) = v.get("controls").and_then(|c| c.as_array()) {
        for c in list {
            let question = c.get("question").and_then(|q| q.as_str()).map(str::trim).unwrap_or("");
            let expect = c.get("expect").and_then(|e| e.as_str()).map(|e| e.trim().trim_start_matches("./")).unwrap_or("");
            let Some(file) = files.iter().find(|f| f.eq_ignore_ascii_case(expect)) else { continue };
            if question.is_empty() || question.len() > 300 {
                continue;
            }
            out.controls.push(Control { question: question.to_string(), expect: file.clone(), source: None });
            if out.controls.len() == crate::controls::MAX_PROPOSED {
                break;
            }
        }
    }
    out
}

/// The titles of the first [`MAX_TITLES`] pages in `files`, read from the
/// head of each.
fn page_titles(root: &Path, files: &[String]) -> Vec<(String, String)> {
    files
        .iter()
        .filter(|f| crate::chunker::is_markdown(f))
        .take(MAX_TITLES)
        .filter_map(|f| {
            let text = std::fs::read_to_string(root.join(f)).ok()?;
            let head: String = text.chars().take(4000).collect();
            crate::chunker::page_title(&head).map(|t| (f.clone(), t))
        })
        .collect()
}

/// Analyse the source at `root` with `generate` (one Claude call).
pub fn analyze(root: &Path, name: &str, kinds: &[RepoKind], generate: &mut dyn FnMut(&str) -> Result<String>) -> Result<Analysis> {
    let files = file_list(root);
    if files.is_empty() {
        return Ok(Analysis::default());
    }
    let titles = page_titles(root, &files);
    let reply = generate(&prompt(name, kinds, &files, &titles))?;
    Ok(parse(&reply, &files, wants_roles(kinds)))
}

/// Analyse the source at `root` and save what it proposes, when it has a
/// knowledge base to save into and no analysis saved yet. Returns whether
/// the settings changed. No call is made when there is nothing to do.
pub fn analyze_and_save(root: &Path, kinds: &[RepoKind], generate: &mut dyn FnMut(&str) -> Result<String>) -> Result<bool> {
    let Some(settings) = crate::knowledge_settings::for_repo(root) else {
        return Ok(false);
    };
    if settings.analyzed() {
        return Ok(false);
    }
    let analysis = analyze(root, &settings.name, kinds, generate)?;
    crate::knowledge_settings::save_proposals(root, &analysis.roles, &analysis.controls)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_keeps_what_holds() {
        let files = vec!["src/net/Packet.java".to_string(), "Design/Saving.md".to_string()];
        let reply = "Here it is:\n```json\n{\"roles\": {\"src/net/**\": \"Network Protocol\", \"\": \"x\", \"src/a/**\": \"\"},\n \
                     \"controls\": [{\"question\": \"how are packets framed\", \"expect\": \"./src/net/packet.java\"},\n \
                     {\"question\": \"made up\", \"expect\": \"src/Nowhere.java\"}, {\"question\": \"\", \"expect\": \"Design/Saving.md\"}]}\n```";
        let a = parse(reply, &files, true);
        assert_eq!(a.roles, vec![("src/net/**".to_string(), "network protocol".to_string())]);
        assert_eq!(a.controls, vec![Control { question: "how are packets framed".into(), expect: "src/net/Packet.java".into(), source: None }]);
        assert!(parse(reply, &files, false).roles.is_empty(), "a knowledge base gets no roles");
        assert_eq!(parse("no json here", &files, true), Analysis::default());
    }

    #[test]
    fn the_prompt_lists_files_with_titles_and_asks_for_roles_only_of_code() {
        let files = vec!["src/Save.java".to_string(), "Design/Saving.md".to_string()];
        let titles = vec![("Design/Saving.md".to_string(), "How worlds save".to_string())];
        let p = prompt("Game", &[RepoKind::Code], &files, &titles);
        assert!(p.contains("\"Game\" (kind: code)") && p.contains("service, ") && p.contains("Design/Saving.md | How worlds save\n"), "{p}");
        let p = prompt("Wiki", &[RepoKind::Wiki], &files, &[]);
        assert!(p.contains("propose no roles"), "{p}");
    }

    #[test]
    fn the_file_list_skips_tests_and_what_is_not_code_or_pages() {
        let d = tempfile::tempdir().unwrap();
        for f in ["src/Save.java", "src/test/SaveTest.java", "Design/Saving.md", "assets/logo.png", "build.zip"] {
            let p = d.path().join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        }
        assert_eq!(file_list(d.path()), vec!["Design/Saving.md".to_string(), "src/Save.java".to_string()]);
        assert_eq!(sample((0..10).map(|i| i.to_string()).collect(), 5), vec!["0", "2", "4", "6", "8"]);
    }

    #[test]
    fn with_no_settings_nothing_is_asked() {
        let d = tempfile::tempdir().unwrap();
        let mut never = |_: &str| -> Result<String> { panic!("no knowledge base to save into") };
        assert!(!analyze_and_save(d.path(), &[RepoKind::Code], &mut never).unwrap());
    }
}
