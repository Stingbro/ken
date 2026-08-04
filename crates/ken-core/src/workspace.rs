//! Workspace lifecycle: `<parent>/.ken-workspace/workspace.json` is the
//! shared, text-only source of truth for a parent folder whose child
//! projects are opened together (`openspec/changes/workspace`). Same
//! philosophy as `project.rs`'s `.ken/project.json` — unknown fields survive
//! a rewrite, and a manifest already on disk is adopted rather than
//! clobbered.
//!
//! Deviation from `project.rs::Project::save` (which does a plain
//! `fs::write`): `proposal.md`/`spec.md` both call out the workspace
//! manifest as written *atomically*, so [`Workspace::save`] uses the
//! temp-file + rename pattern `profiler::ProjectProfile::save` already
//! established in this crate, rather than copying `Project::save`'s
//! non-atomic write verbatim.
//!
//! `workspace.rs` only resolves members into `Project`s (via the same
//! adopt-or-create discipline as `Project::create`) and reports per-member
//! status; it holds no DB handle, engine, or watcher of its own — those are
//! `src-tauri`'s `ProjectHandle` concern (design D2/D3).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::project::{self, Project};
use crate::{Error, Result};

pub const CONFIG_DIR: &str = ".ken-workspace";
pub const CONFIG_FILE: &str = "workspace.json";

pub fn config_path(parent: &Path) -> PathBuf {
    parent.join(CONFIG_DIR).join(CONFIG_FILE)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub name: String,
    pub id: Uuid,
    /// Parent-relative folder names (design D1: relative so the manifest
    /// survives the parent being moved or synced to a teammate).
    pub members: Vec<String>,
    /// Fields written by newer versions or other capabilities survive a
    /// round-trip through this one (same idiom as `ProjectConfig::extra`).
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// One entry of `workspace.json`'s `links` array (design D12,
/// `ken-pipeline`): an explicit cross-project link, e.g. a tool repo linked
/// to the project it supports. Not a typed `WorkspaceConfig` field — see
/// [`WorkspaceConfig::links`] for why — this struct exists only to give
/// callers a parsed shape for the entries already living in `extra`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectLink {
    pub from: String,
    pub to: String,
    pub relation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl WorkspaceConfig {
    /// Parsed `links` array (design D12). Deliberately **not** a typed
    /// `WorkspaceConfig` field: the manifest already round-trips unknown
    /// keys through `extra` (see `unknown_fields_survive_roundtrip`), and a
    /// typed field would only be needed if something here wrote `links`
    /// itself — nothing in this phase does, links are hand-authored or
    /// written by a future lane. Reading them out of `extra` on demand keeps
    /// that round-trip guarantee exactly as-is. Entries that don't parse as
    /// `{from, to, relation, note?}` are skipped rather than failing the
    /// whole read (same tolerant-load philosophy as the rest of this
    /// module).
    pub fn links(&self) -> Vec<ProjectLink> {
        self.extra
            .get("links")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|entry| serde_json::from_value(entry.clone()).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Project names linked to `name`, in either direction. `links` entries
    /// are stored directional (`from` -> `to`, carrying a `relation` that
    /// reads naturally one way, e.g. "ShatteredRealmsTools tools_for
    /// ShatteredRealms"), but every consumer named in design D12 — idea
    /// dedupe scope, the board's "include linked projects" filter, and
    /// cross-project `blocked_by` suggestions — cares about connectivity,
    /// not which side is `from`. So a link recorded as `{from: A, to: B}`
    /// makes each project visible from the other's scope; callers that need
    /// the raw direction/relation should use [`WorkspaceConfig::links`]
    /// instead.
    pub fn linked_projects(&self, name: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let Some(arr) = self.extra.get("links").and_then(|v| v.as_array()) else {
            return out;
        };
        for entry in arr {
            let from = entry.get("from").and_then(|v| v.as_str());
            let to = entry.get("to").and_then(|v| v.as_str());
            match (from, to) {
                (Some(f), Some(t)) if f == name => out.push(t),
                (Some(f), Some(t)) if t == name => out.push(f),
                _ => {}
            }
        }
        out
    }
}

/// Per-member resolution result within an opened [`Workspace`].
#[derive(Debug, Clone)]
pub enum MemberStatus {
    /// The member folder exists and its `.ken/project.json` was opened (or
    /// adopted/created if the folder had none yet — same adopt-or-create
    /// discipline as [`Project::create`]).
    Ok(Project),
    /// The parent-relative folder name in the manifest doesn't exist on
    /// disk. Not fatal to the workspace open (spec: "Missing member folders
    /// SHALL be reported as `missing` status, never fail the open").
    Missing,
    /// The folder exists but its `.ken/project.json` failed to parse. Not
    /// one of the spec's two named states, but the same tolerant-load
    /// philosophy that runs through the rest of ken-core (`project.rs`,
    /// `settings.rs`) applies here too: one corrupt member must never fail
    /// opening the other N-1.
    Invalid(String),
}

#[derive(Debug, Clone)]
pub struct Member {
    /// Parent-relative folder name, exactly as stored in the manifest.
    pub name: String,
    pub status: MemberStatus,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    /// The parent folder — i.e. the folder containing `.ken-workspace/`.
    pub root: PathBuf,
    pub config: WorkspaceConfig,
    pub members: Vec<Member>,
}

impl Workspace {
    /// Create a new workspace in `parent`, resolving each of `member_names`
    /// via `Project::create` (mkdir `.ken-workspace`, write the manifest).
    /// If a manifest already exists at `parent` (e.g. a teammate's clone),
    /// it is adopted unchanged, mirroring `Project::create`'s
    /// adopt-if-exists discipline — `member_names` is ignored in that case,
    /// same as `Project::create` ignoring `name` when adopting.
    ///
    /// Every `member_names` entry must already exist as a subfolder of
    /// `parent` (candidates come from `discover_candidates`, which only
    /// lists real folders) — unlike `Workspace::open`, a missing member
    /// here is a hard error, not a `Missing` status, since there is no
    /// prior manifest state to be tolerant of yet.
    pub fn create(parent: &Path, name: &str, member_names: &[String]) -> Result<Workspace> {
        if !parent.is_dir() {
            return Err(Error::ProjectMissing(parent.to_path_buf()));
        }
        if config_path(parent).exists() {
            return Workspace::open(parent);
        }
        let name = project::normalize_name(name)?;
        let mut members = Vec::with_capacity(member_names.len());
        for member_name in member_names {
            let member_root = parent.join(member_name);
            let project = Project::create(&member_root, member_name)?;
            members.push(Member {
                name: member_name.clone(),
                status: MemberStatus::Ok(project),
            });
        }
        let config = WorkspaceConfig {
            name,
            id: Uuid::new_v4(),
            members: member_names.to_vec(),
            extra: serde_json::Map::new(),
        };
        let workspace = Workspace {
            root: parent.to_path_buf(),
            config,
            members,
        };
        workspace.save()?;
        Ok(workspace)
    }

    /// Open a parent folder that already contains
    /// `.ken-workspace/workspace.json`. Members are resolved relative to
    /// `parent` at open time, so a renamed/moved parent still resolves them
    /// (spec: "moved parent folder still opens"). A member folder missing
    /// from disk yields `MemberStatus::Missing`; a member folder present
    /// but with an unparsable `.ken/project.json` yields
    /// `MemberStatus::Invalid` — neither fails the open.
    pub fn open(parent: &Path) -> Result<Workspace> {
        let path = config_path(parent);
        let raw = fs::read_to_string(&path).map_err(|e| {
            if !parent.is_dir() {
                Error::ProjectMissing(parent.to_path_buf())
            } else {
                Error::io(&path, e)
            }
        })?;
        let config: WorkspaceConfig =
            serde_json::from_str(&raw).map_err(|e| Error::InvalidProject {
                path: path.clone(),
                reason: e.to_string(),
            })?;
        let members = config
            .members
            .iter()
            .map(|name| Self::resolve_member_tolerant(parent, name))
            .collect();
        Ok(Workspace {
            root: parent.to_path_buf(),
            config,
            members,
        })
    }

    fn resolve_member_tolerant(parent: &Path, name: &str) -> Member {
        let member_root = parent.join(name);
        if !member_root.is_dir() {
            return Member {
                name: name.to_string(),
                status: MemberStatus::Missing,
            };
        }
        match Project::create(&member_root, name) {
            Ok(project) => Member {
                name: name.to_string(),
                status: MemberStatus::Ok(project),
            },
            Err(e) => Member {
                name: name.to_string(),
                status: MemberStatus::Invalid(e.to_string()),
            },
        }
    }

    /// Write the manifest atomically (temp file + rename — see module docs
    /// for why this diverges from `Project::save`'s plain write).
    pub fn save(&self) -> Result<()> {
        let dir = self.root.join(CONFIG_DIR);
        fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        let path = config_path(&self.root);
        let json = serde_json::to_string_pretty(&self.config)
            .map_err(|e| Error::Other(e.to_string()))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json + "\n").map_err(|e| Error::io(&tmp, e))?;
        fs::rename(&tmp, &path).map_err(|e| Error::io(&path, e))
    }
}

/// One immediate subfolder of a prospective workspace parent, as surfaced
/// to the folder-select UI (design D5: "shallow and dumb on purpose" — one
/// level deep, no recursion, no LLM).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// The folder's own name — becomes the manifest's relative member name
    /// if selected.
    pub name: String,
    /// Already has `.ken/project.json` — pre-checked in the selection UI.
    pub existing: bool,
    /// File count directly inside the folder. Not recursive: matches the
    /// "one level deep" discovery philosophy and keeps discovery cheap
    /// across many sibling folders — enough for a "N files" caption without
    /// a full tree walk per candidate.
    pub file_count: usize,
    /// Repo marker file names found directly inside the folder (see
    /// `crate::profiler::REPO_MARKERS`), plus `.git`/`*.sln` when present.
    pub markers: Vec<String>,
}

/// List `parent`'s immediate subfolders as workspace-member candidates.
/// Hidden folders (dot-prefixed — this also excludes `.ken-workspace`
/// itself) and junk build dirs (`node_modules`, `target`, ... —
/// `crate::scan::is_junk_dir_name`, the same table the ingest walk and the
/// profiler use) are never candidates. One level deep only: neither this
/// listing nor a candidate's `file_count`/`markers` recurse into
/// subfolders (D5).
pub fn discover_candidates(parent: &Path) -> Result<Vec<Candidate>> {
    let entries = fs::read_dir(parent).map_err(|e| Error::io(parent, e))?;
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    let mut out = Vec::with_capacity(dirs.len());
    for dir in dirs {
        let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') || crate::scan::is_junk_dir_name(name) {
            continue;
        }
        out.push(scan_candidate(&dir, name)?);
    }
    Ok(out)
}

/// One level deep into a single candidate: does it have `.ken/project.json`
/// already, how many files sit directly in it, and which repo markers
/// (`crate::profiler::REPO_MARKERS`, `.git`, `*.sln`) does it carry.
fn scan_candidate(dir: &Path, name: &str) -> Result<Candidate> {
    let existing = project::config_path(dir).exists();
    let mut file_count = 0usize;
    let mut markers: Vec<String> = Vec::new();

    // `.git` is itself a hidden dir, so it's checked directly rather than
    // encountered in the (non-hidden) listing below — same reasoning as
    // `profiler::scan_stats`.
    if dir.join(".git").is_dir() {
        markers.push(".git".to_string());
    }

    let entries = fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        file_count += 1;
        let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if crate::profiler::REPO_MARKERS.contains(&fname) && !markers.iter().any(|m| m == fname) {
            markers.push(fname.to_string());
        } else if fname.to_ascii_lowercase().ends_with(".sln") && !markers.iter().any(|m| m == "*.sln") {
            markers.push("*.sln".to_string());
        }
    }

    Ok(Candidate {
        name: name.to_string(),
        existing,
        file_count,
        markers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn create_then_open_round_trip() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("alpha")).unwrap();
        fs::create_dir_all(dir.path().join("beta")).unwrap();
        let created =
            Workspace::create(dir.path(), "My Workspace", &["alpha".into(), "beta".into()])
                .unwrap();
        assert!(config_path(dir.path()).exists());
        assert_eq!(created.members.len(), 2);
        assert!(created
            .members
            .iter()
            .all(|m| matches!(m.status, MemberStatus::Ok(_))));

        let reopened = Workspace::open(dir.path()).unwrap();
        assert_eq!(reopened.config, created.config);
        assert_eq!(reopened.members.len(), 2);
        for m in &reopened.members {
            assert!(
                matches!(m.status, MemberStatus::Ok(_)),
                "member {} not ok",
                m.name
            );
        }
    }

    #[test]
    fn unknown_fields_survive_roundtrip() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("alpha")).unwrap();
        Workspace::create(dir.path(), "WS", &["alpha".into()]).unwrap();

        // Simulate a newer version adding a field.
        let path = config_path(dir.path());
        let mut v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        v["ingestRunner"] = "hidden-tui".into();
        fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();

        let reopened = Workspace::open(dir.path()).unwrap();
        assert_eq!(
            reopened.config.extra.get("ingestRunner").and_then(|x| x.as_str()),
            Some("hidden-tui")
        );
        reopened.save().unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("ingestRunner"), "extra field lost: {raw}");
    }

    #[test]
    fn links_round_trip_through_unknown_keys() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("alpha")).unwrap();
        Workspace::create(dir.path(), "WS", &["alpha".into()]).unwrap();

        // Simulate a newer/other-tool write that adds `links` alongside an
        // unrelated unknown key, same as `unknown_fields_survive_roundtrip`.
        let path = config_path(dir.path());
        let mut v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        v["links"] = serde_json::json!([
            { "from": "ShatteredRealms", "to": "ShatteredRealmsTools", "relation": "tools_for" },
            { "from": "ShatteredRealms", "to": "Docs", "relation": "documents", "note": "wiki" },
        ]);
        v["ingestRunner"] = "hidden-tui".into();
        fs::write(&path, serde_json::to_string(&v).unwrap()).unwrap();

        let reopened = Workspace::open(dir.path()).unwrap();
        let links = reopened.config.links();
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].from, "ShatteredRealms");
        assert_eq!(links[0].to, "ShatteredRealmsTools");
        assert_eq!(links[0].relation, "tools_for");
        assert_eq!(links[0].note, None);
        assert_eq!(links[1].note.as_deref(), Some("wiki"));

        // Write path preserves both the unrelated unknown key and `links`
        // itself (a manifest saved by this build must not drop either).
        reopened.save().unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("ingestRunner"), "extra field lost: {raw}");
        assert!(raw.contains("ShatteredRealmsTools"), "links lost: {raw}");
    }

    #[test]
    fn linked_projects_resolves_both_directions() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("alpha")).unwrap();
        let mut ws = Workspace::create(dir.path(), "WS", &["alpha".into()]).unwrap();
        ws.config.extra.insert(
            "links".into(),
            serde_json::json!([
                { "from": "ShatteredRealms", "to": "ShatteredRealmsTools", "relation": "tools_for" },
            ]),
        );

        assert_eq!(
            ws.config.linked_projects("ShatteredRealms"),
            vec!["ShatteredRealmsTools"]
        );
        assert_eq!(
            ws.config.linked_projects("ShatteredRealmsTools"),
            vec!["ShatteredRealms"]
        );
        assert!(ws.config.linked_projects("Unrelated").is_empty());
    }

    #[test]
    fn links_absent_when_no_links_key() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("alpha")).unwrap();
        let ws = Workspace::create(dir.path(), "WS", &["alpha".into()]).unwrap();
        assert!(ws.config.links().is_empty());
        assert!(ws.config.linked_projects("alpha").is_empty());
    }

    #[test]
    fn adopt_existing_manifest() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("alpha")).unwrap();
        let first = Workspace::create(dir.path(), "Original", &["alpha".into()]).unwrap();
        // A second create (e.g. teammate opening a cloned folder) adopts.
        let second = Workspace::create(dir.path(), "Renamed", &["alpha".into()]).unwrap();
        assert_eq!(second.config.id, first.config.id);
        assert_eq!(second.config.name, "Original");
    }

    #[test]
    fn missing_member_reported() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("alpha")).unwrap();
        fs::create_dir_all(dir.path().join("beta")).unwrap();
        Workspace::create(dir.path(), "WS", &["alpha".into(), "beta".into()]).unwrap();
        fs::remove_dir_all(dir.path().join("beta")).unwrap();

        let reopened = Workspace::open(dir.path()).unwrap();
        let beta = reopened.members.iter().find(|m| m.name == "beta").unwrap();
        assert!(matches!(beta.status, MemberStatus::Missing));
        let alpha = reopened.members.iter().find(|m| m.name == "alpha").unwrap();
        assert!(matches!(alpha.status, MemberStatus::Ok(_)));
    }

    #[test]
    fn relative_member_resolution_after_parent_rename() {
        let base = tempdir().unwrap();
        let parent = base.path().join("parent");
        fs::create_dir_all(parent.join("alpha")).unwrap();
        let created = Workspace::create(&parent, "WS", &["alpha".into()]).unwrap();
        let original_id = match &created.members[0].status {
            MemberStatus::Ok(p) => p.config.id,
            other => panic!("expected ok member, got {other:?}"),
        };

        let moved = base.path().join("parent-renamed");
        fs::rename(&parent, &moved).unwrap();

        let reopened = Workspace::open(&moved).unwrap();
        match &reopened.members[0].status {
            MemberStatus::Ok(p) => {
                assert_eq!(p.config.id, original_id);
                assert_eq!(p.root, moved.join("alpha"));
            }
            other => panic!("expected ok member, got {other:?}"),
        }
    }

    #[test]
    fn discovery_excludes_junk_and_tags_existing_vs_new() {
        let dir = tempdir().unwrap();
        // An already-adopted Ken project. (`Project::create` adopts an
        // existing folder — it never mkdirs — so the folder comes first.)
        fs::create_dir_all(dir.path().join("existing")).unwrap();
        Project::create(&dir.path().join("existing"), "Existing").unwrap();
        // A plain repo, not yet a Ken project.
        fs::create_dir_all(dir.path().join("repo")).unwrap();
        fs::write(
            dir.path().join("repo").join("Cargo.toml"),
            "[package]\nname = \"x\"\n",
        )
        .unwrap();
        fs::write(dir.path().join("repo").join("main.rs"), "fn main() {}").unwrap();
        // Junk / excluded.
        fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        fs::create_dir_all(dir.path().join(".hidden")).unwrap();
        fs::create_dir_all(dir.path().join(".ken-workspace")).unwrap();

        let candidates = discover_candidates(dir.path()).unwrap();
        let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"existing"));
        assert!(names.contains(&"repo"));
        assert!(!names.contains(&"node_modules"));
        assert!(!names.contains(&".hidden"));
        assert!(!names.contains(&".ken-workspace"));

        let existing = candidates.iter().find(|c| c.name == "existing").unwrap();
        assert!(existing.existing);

        let repo = candidates.iter().find(|c| c.name == "repo").unwrap();
        assert!(!repo.existing);
        assert!(repo.markers.contains(&"Cargo.toml".to_string()));
        assert_eq!(repo.file_count, 2);
    }
}
