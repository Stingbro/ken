//! Task files (`.ken-workspace/tasks/`, the older `<project>/.ken/tasks/`
//! homes, and family boards): the byte-faithful frontmatter core that Your
//! day (`day.rs`), the team inbox (`family.rs`) and `ken-mcp` share. The
//! old task board (columns, goals, rollover) is gone; what is left here is
//! the file format and the patch core it was built on.
//!
//! Everything here is path arithmetic, frontmatter parsing, byte-faithful
//! file patching, and text composition — no `Db`/`IngestEngine`/watcher
//! access (design D3: "UI mutations are commands that call the D1 rewrite
//! core, then let the watcher event round-trip confirm"). Callers own the
//! watcher, the board model, the flag gate, and the clock.
//!
//! ## Frontmatter fidelity — S6 is binding here (unlike `memory.rs`)
//!
//! `features/multi-project/spikes/S6-frontmatter-roundtrip.md` benchmarked
//! two frontmatter patch cores and concluded: use the **raw line splitter**
//! (candidate A, 20/20 byte-exact round trips) for writes, and keep
//! serde_yaml for read-side parsing only. `memory.rs` deliberately did not
//! follow that recommendation (see its module doc) because memory files are
//! whole-file rewrites of a four-key frontmatter. Task files are the
//! opposite case and the spike's own headline consumer: they are hand
//! edited, carry unknown keys, and are patched key-by-key on every
//! drag-drop, so byte fidelity of every untouched line is the contract
//! (spec: "Unknown frontmatter keys and the body SHALL survive every
//! programmatic rewrite byte-for-byte"). So:
//!
//! - **Writes** ([`patch_text`]) go through the raw line splitter. Only the
//!   physical lines of the named keys are replaced; comments, key order,
//!   indentation, quoting style, CRLF terminators, and the body pass
//!   through untouched. Files with no frontmatter block get one prepended
//!   (the case serde_yaml had no fallback for).
//! - **Reads** ([`parse_task`]) use serde_yaml, exactly as S6 permits.
//! - **Concurrency**: S6's unplanned finding was that a naive
//!   read-modify-write loses ~17% of a concurrent external writer's edits.
//!   [`apply_edits`] therefore re-fingerprints the file (len + mtime +
//!   content hash) immediately before writing and retries the whole
//!   read-patch-write cycle on a mismatch.
//! - **Multi-line values**: the S6 prototype left "multiline_value on a
//!   *patched* key" as a TODO (its corpus only patched single-line keys).
//!   [`patch_text`] closes it: replacing a key consumes the key's whole
//!   physical extent — every following indented line, `-` sequence item at
//!   column 0, and interior blank line — so block scalars and block
//!   sequences are replaced as a unit instead of leaving orphaned
//!   continuation lines behind.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

const TASKS_SUBDIR: &str = "tasks";
const ARCHIVE_SUBDIR: &str = "archive";

/// The `## Log` heading `task_complete` reports are appended under (D4).
pub const LOG_HEADING: &str = "## Log";

/// How many read-patch-write cycles [`apply_edits`] will run before giving
/// up on a file another writer keeps changing underneath it (S6's
/// optimistic-concurrency guard: "retry on mismatch").
pub const PATCH_MAX_ATTEMPTS: usize = 4;

// ---------------------------------------------------------------------
// 1.1 / 1.3 Homes and paths
// ---------------------------------------------------------------------

/// `.ken-workspace/tasks/`, relative to the workspace parent folder
/// (`workspace::Workspace::root` — "the folder containing
/// `.ken-workspace/`", not `.ken-workspace/` itself), mirroring
/// `memory::workspace_memory_dir`.
pub fn workspace_tasks_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(crate::workspace::CONFIG_DIR).join(TASKS_SUBDIR)
}

/// `<project>/.ken/tasks/` — the opt-in per-repo home (D2: "just create
/// the folder — its existence is the opt-in").
pub fn project_tasks_dir(project_root: &Path) -> PathBuf {
    project_root.join(crate::project::CONFIG_DIR).join(TASKS_SUBDIR)
}

/// `<tasks-home>/archive/YYYY-MM/` — archiving stays inside the task's own
/// home so per-repo history ships with the repo (D2).
pub fn archive_dir(home_dir: &Path, year_month: &str) -> PathBuf {
    home_dir.join(ARCHIVE_SUBDIR).join(year_month)
}

/// Which home a task file lives in. Home is "invisible plumbing" for the
/// board (D2) but load-bearing for two things: archive pathing and the
/// `project` default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HomeKind {
    Workspace,
    Project,
    /// A family board (`<family-clone>/members/<member-id>/board/`) —
    /// ken-families' third task home (design D2 follow-up: "the board scans
    /// both [homes] and treats home as invisible plumbing" extended to a
    /// third). Deliberately a fieldless unit variant like its siblings so
    /// `Task::home` keeps serializing as a plain lowercase string
    /// (`"family"`) rather than changing shape on the wire — the extra data
    /// a family task needs (which board dir, which family's display name)
    /// lives on `TaskHome::Family` at scan time, not here.
    Family,
}

/// A task home to scan. Borrowed like `memory::MemoryScope` so callers
/// never have to know the `.ken`/`.ken-workspace` folder-naming details.
#[derive(Debug, Clone, Copy)]
pub enum TaskHome<'a> {
    Workspace {
        workspace_root: &'a Path,
    },
    /// `project` is the display name used to default the `project`
    /// frontmatter key of tasks that omit it (D2: "a per-repo task needs no
    /// `project` key — defaulted from its home").
    Project {
        project_root: &'a Path,
        project: &'a str,
    },
    /// A family board (ken-families task 2.4's "third home"). Unlike
    /// `Project`, whose `tasks_dir()` derives `<project_root>/.ken/tasks`
    /// from a root, a family board has no such fixed suffix to append — the
    /// caller already resolved `<clone>/members/<member-id>/board` (see
    /// `family::board_dir`) before it has enough context (the family clone
    /// root lives in app data, keyed by a `family_id` this module doesn't
    /// know about) to hand it to `TaskHome`, so this variant takes the
    /// board dir directly rather than re-deriving it.
    Family {
        board_dir: &'a Path,
        /// The family's display name — fills the same `default_project`
        /// role a real project's name would (mirrors `Project::project`).
        family_name: &'a str,
    },
}

impl<'a> TaskHome<'a> {
    pub fn tasks_dir(&self) -> PathBuf {
        match self {
            TaskHome::Workspace { workspace_root } => workspace_tasks_dir(workspace_root),
            TaskHome::Project { project_root, .. } => project_tasks_dir(project_root),
            // The board dir itself IS the listing dir — a family board has
            // no `.ken/tasks` (or similar) subfolder to append, files live
            // directly under `members/<id>/board/` (mirrors `family::
            // board_dir`'s own doc comment: "archive/YYYY-MM/ goes
            // underneath it, exactly as tasks::archive_dir computes for the
            // other two homes").
            TaskHome::Family { board_dir, .. } => board_dir.to_path_buf(),
        }
    }

    pub fn kind(&self) -> HomeKind {
        match self {
            TaskHome::Workspace { .. } => HomeKind::Workspace,
            TaskHome::Project { .. } => HomeKind::Project,
            TaskHome::Family { .. } => HomeKind::Family,
        }
    }

    /// The `project` value a task in this home inherits when its own
    /// frontmatter leaves the key empty.
    pub fn default_project(&self) -> &str {
        match self {
            TaskHome::Workspace { .. } => "",
            TaskHome::Project { project, .. } => project,
            TaskHome::Family { family_name, .. } => family_name,
        }
    }
}

// ---------------------------------------------------------------------
// 1.1 Enums
// ---------------------------------------------------------------------

/// Board columns, left to right. `Backlog` is the intake column (D7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Backlog,
    Todo,
    Doing,
    Review,
    Done,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Backlog => "backlog",
            TaskStatus::Todo => "todo",
            TaskStatus::Doing => "doing",
            TaskStatus::Review => "review",
            TaskStatus::Done => "done",
        }
    }

    /// Case-insensitive parse. `None` for anything not in the vocabulary —
    /// the caller decides whether that means "needs attention" (a parsed
    /// file) or "reject" (a tool argument).
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "backlog" => Some(TaskStatus::Backlog),
            "todo" => Some(TaskStatus::Todo),
            "doing" => Some(TaskStatus::Doing),
            "review" => Some(TaskStatus::Review),
            "done" => Some(TaskStatus::Done),
            _ => None,
        }
    }

    pub const ALL: [TaskStatus; 5] = [
        TaskStatus::Backlog,
        TaskStatus::Todo,
        TaskStatus::Doing,
        TaskStatus::Review,
        TaskStatus::Done,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskKind {
    Human,
    Ai,
}

impl TaskKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskKind::Human => "human",
            TaskKind::Ai => "ai",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "human" => Some(TaskKind::Human),
            "ai" => Some(TaskKind::Ai),
            _ => None,
        }
    }
}

/// Which board a task renders on (D5: the daily board is "a filter plus
/// two rituals", not a separate format).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BoardKind {
    Main,
    Daily,
}

impl BoardKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BoardKind::Main => "main",
            BoardKind::Daily => "daily",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "main" => Some(BoardKind::Main),
            "daily" => Some(BoardKind::Daily),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------
// 1.1 Frontmatter read model
// ---------------------------------------------------------------------

/// The modeled frontmatter keys, in canonical emit order. Used to decide
/// what counts as an "unknown key" in [`Task::extra`] and to order keys
/// appended to a file that didn't have them.
const KNOWN_KEYS: &[&str] = &[
    "id", "title", "status", "kind", "assignee", "project", "tags", "due", "goal", "board",
    "created", "updated",
];

/// Tolerant read-side frontmatter (S6: serde_yaml is fine for reads).
/// Enum-valued keys are modeled as `String` on purpose — a hand-edited
/// `status: blocked` must degrade to a needs-attention entry, not fail the
/// whole file's parse.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct TaskFrontmatter {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    assignee: String,
    #[serde(default)]
    project: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    due: String,
    #[serde(default)]
    goal: String,
    #[serde(default)]
    board: String,
    #[serde(default)]
    created: String,
    #[serde(default)]
    updated: String,
    #[serde(flatten)]
    extra: serde_yaml::Mapping,
}

/// A parsed task file. `id` is authoritative over the filename (D1), so
/// everything downstream keys on it and renaming a file changes nothing.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    /// `None` when the file's `status` is present but outside the
    /// vocabulary — the needs-attention case. An absent/empty `status`
    /// reads as `Backlog`, the intake column (D7).
    pub status: Option<TaskStatus>,
    /// Exactly what the file said, so the tray can show it and so nothing
    /// silently normalizes it.
    pub status_raw: String,
    pub kind: TaskKind,
    pub kind_raw: String,
    pub assignee: String,
    pub project: String,
    pub tags: Vec<String>,
    pub due: Option<String>,
    pub goal: Option<String>,
    pub board: BoardKind,
    pub board_raw: String,
    pub created: String,
    pub updated: String,
    pub body: String,
    pub path: PathBuf,
    pub home: HomeKind,
    /// The `tasks/` directory this task lives in — where its
    /// `archive/YYYY-MM/` goes.
    pub home_dir: PathBuf,
    /// Unknown frontmatter keys, preserved for display. Writes never
    /// rebuild frontmatter from this — [`patch_text`] leaves the original
    /// lines untouched — so this is read-side only.
    #[serde(skip)]
    extra: serde_yaml::Mapping,
}

impl Task {
    pub fn extra(&self) -> &serde_yaml::Mapping {
        &self.extra
    }

    /// The file name (`<ulid>-<slug>.md`).
    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Path relative to the member root that owns this task — the tail of
    /// its `ken://` address.
    pub fn address_rel_path(&self) -> String {
        home_rel_path(self.home, &self.home_dir, &self.file_name())
    }

    /// `ken://<host>/<rel-path>` (`routing::ken_address`'s fixed scheme).
    /// `host` is `memory::WORKSPACE_ADDRESS_ID` for workspace-home tasks
    /// and the owning project's id for per-repo tasks — this module has no
    /// `Project` handle, so the caller supplies it.
    pub fn address(&self, host: &str) -> String {
        format!("ken://{host}/{}", self.address_rel_path())
    }

    /// True when the file's `status` was present but unrecognized. Such a
    /// file is shown in the needs-attention tray and never rewritten by a
    /// patch that doesn't explicitly resolve the status (spec: "the file is
    /// not rewritten").
    pub fn has_invalid_status(&self) -> bool {
        self.status.is_none()
    }
}

/// Path of a task file relative to the member root that owns its home —
/// the tail of its `ken://` address. A family board's `home_dir` is always
/// `<clone-root>/members/<member-id>/board`, so the member id is the name
/// of its parent directory, and `family::board_rel` composes the same
/// `members/<id>/board` shape every caller uses.
pub fn home_rel_path(home: HomeKind, home_dir: &Path, file_name: &str) -> String {
    match home {
        HomeKind::Workspace => format!("{TASKS_SUBDIR}/{file_name}"),
        HomeKind::Project => format!("{}/{TASKS_SUBDIR}/{file_name}", crate::project::CONFIG_DIR),
        HomeKind::Family => {
            let member_id = home_dir
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!("{}/{file_name}", crate::family::board_rel(&member_id))
        }
    }
}

/// Split `---\n ... \n---\n` off the front of a file's raw text, keeping
/// every byte addressable. Unlike `memory::split_frontmatter` (which only
/// needs the two halves) this retains the opening/closing delimiter lines
/// verbatim so the patch core can reassemble the file without touching
/// them.
struct Split<'a> {
    /// `"---\n"` or `"---\r\n"`.
    open: &'a str,
    /// The frontmatter lines, each with its own terminator.
    fm: &'a str,
    /// The closing `---` line, with its terminator (may be missing at EOF).
    close: &'a str,
    /// Everything after the closing delimiter, byte-for-byte.
    body: &'a str,
}

fn split_raw(raw: &str) -> Option<Split<'_>> {
    let open_len = if raw.starts_with("---\r\n") {
        5
    } else if raw.starts_with("---\n") {
        4
    } else {
        return None;
    };
    let rest = &raw[open_len..];
    let mut off = 0usize;
    loop {
        let line_end = rest[off..]
            .find('\n')
            .map(|i| off + i + 1)
            .unwrap_or(rest.len());
        let line = &rest[off..line_end];
        if line.trim_end_matches('\n').trim_end_matches('\r') == "---" {
            return Some(Split {
                open: &raw[..open_len],
                fm: &rest[..off],
                close: line,
                body: &rest[line_end..],
            });
        }
        if line_end >= rest.len() {
            return None; // unterminated frontmatter — treat as no frontmatter
        }
        off = line_end;
    }
}

/// The first non-empty line of `body` with leading `#`s trimmed — the
/// title fallback for a hand-written file with no `title:` key (same
/// posture as `memory::first_line`).
fn first_line(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| l.trim_start_matches('#').trim().to_string())
        .unwrap_or_default()
}

pub(crate) fn map_str(m: &serde_yaml::Mapping, key: &str) -> String {
    match m.get(serde_yaml::Value::String(key.to_string())) {
        Some(serde_yaml::Value::String(s)) => s.clone(),
        Some(serde_yaml::Value::Number(n)) => n.to_string(),
        Some(serde_yaml::Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

pub(crate) fn map_list(m: &serde_yaml::Mapping, key: &str) -> Vec<String> {
    match m.get(serde_yaml::Value::String(key.to_string())) {
        Some(serde_yaml::Value::Sequence(items)) => items
            .iter()
            .filter_map(|v| match v {
                serde_yaml::Value::String(s) => Some(s.trim().to_string()),
                serde_yaml::Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .filter(|s| !s.is_empty())
            .collect(),
        // A hand edit like `tags: alpha, beta` is a scalar, not a list.
        Some(serde_yaml::Value::String(s)) => s
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

/// Parse the frontmatter block. The typed `#[serde(default)]` struct
/// (tasks.md 1.1) is the happy path; when a hand edit makes the *typed*
/// shape fail (e.g. `tags:` written as a comma string) we fall back to a
/// tolerant `Mapping` read instead of `unwrap_or_default()`-ing the whole
/// file into blanks. Either way this is read-side only.
fn parse_frontmatter(fm: &str) -> TaskFrontmatter {
    if let Ok(parsed) = serde_yaml::from_str::<TaskFrontmatter>(fm) {
        return parsed;
    }
    let Ok(map) = serde_yaml::from_str::<serde_yaml::Mapping>(fm) else {
        return TaskFrontmatter::default();
    };
    let mut extra = map.clone();
    for k in KNOWN_KEYS {
        extra.remove(serde_yaml::Value::String((*k).to_string()));
    }
    TaskFrontmatter {
        id: map_str(&map, "id"),
        title: map_str(&map, "title"),
        status: map_str(&map, "status"),
        kind: map_str(&map, "kind"),
        assignee: map_str(&map, "assignee"),
        project: map_str(&map, "project"),
        tags: map_list(&map, "tags"),
        due: map_str(&map, "due"),
        goal: map_str(&map, "goal"),
        board: map_str(&map, "board"),
        created: map_str(&map, "created"),
        updated: map_str(&map, "updated"),
        extra,
    }
}

/// Parse a task file's raw text. Infallible and tolerant: no frontmatter,
/// malformed YAML, or an out-of-vocabulary `status`/`kind`/`board` all
/// degrade to a renderable task rather than an error (spec: "invalid hand
/// edit is surfaced, not destroyed").
///
/// `default_project` is the owning home's project name (empty for the
/// workspace home) and only applies when the file leaves `project` empty.
pub fn parse_task(path: &Path, home: HomeKind, default_project: &str, raw: &str) -> Task {
    let (fm, body) = match split_raw(raw) {
        Some(s) => (parse_frontmatter(s.fm), s.body.to_string()),
        None => (TaskFrontmatter::default(), raw.to_string()),
    };
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    // `id` is authoritative over the filename; a file with no `id` yet
    // (hand-created) falls back to its stem so it still shows on the board
    // with a stable identity.
    let id = if fm.id.trim().is_empty() {
        stem.clone()
    } else {
        fm.id.trim().to_string()
    };
    let body_trimmed = body.trim().to_string();
    let title = if fm.title.trim().is_empty() {
        first_line(&body_trimmed)
    } else {
        fm.title.trim().to_string()
    };

    // Absent ⇒ the documented default; present-but-unrecognized ⇒ flagged.
    let status = if fm.status.trim().is_empty() {
        Some(TaskStatus::Backlog)
    } else {
        TaskStatus::parse(&fm.status)
    };
    let kind_parsed = TaskKind::parse(&fm.kind);
    let board_parsed = BoardKind::parse(&fm.board);

    let home_dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let project = if fm.project.trim().is_empty() {
        default_project.to_string()
    } else {
        fm.project.trim().to_string()
    };

    Task {
        id,
        title,
        status,
        status_raw: fm.status.trim().to_string(),
        kind: kind_parsed.unwrap_or(TaskKind::Human),
        kind_raw: fm.kind.trim().to_string(),
        assignee: fm.assignee.trim().to_string(),
        project,
        tags: fm
            .tags
            .iter()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect(),
        due: non_empty(&fm.due),
        goal: non_empty(&fm.goal),
        board: board_parsed.unwrap_or(BoardKind::Main),
        board_raw: fm.board.trim().to_string(),
        created: fm.created.trim().to_string(),
        updated: fm.updated.trim().to_string(),
        body: body_trimmed,
        path: path.to_path_buf(),
        home,
        home_dir,
        extra: fm.extra,
    }
}

fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

// ---------------------------------------------------------------------
// 1.2 Raw line-splitter patch core (S6 candidate A)
// ---------------------------------------------------------------------

/// YAML plain scalars that would resolve to something other than a string.
const YAML_AMBIGUOUS: &[&str] = &[
    "true", "false", "yes", "no", "on", "off", "null", "nil", "~", "y", "n",
];

/// Conservative: a value is emitted bare only when it is unambiguously a
/// plain string (starts with an ASCII letter or `_`, contains only
/// alphanumerics/`_-./ `, no trailing space, not a YAML keyword). Anything
/// else — dates, numbers, ids with `:`, empty strings — is single-quoted,
/// which is always safe.
fn scalar_needs_quoting(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    if YAML_AMBIGUOUS.contains(&s.to_ascii_lowercase().as_str()) {
        return true;
    }
    let first = s.chars().next().unwrap_or(' ');
    if !(first.is_ascii_alphabetic() || first == '_') {
        return true;
    }
    if s.ends_with(' ') {
        return true;
    }
    !s.chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ' '))
}

/// Render a value as a single-line YAML scalar. Embedded newlines are
/// folded to spaces: the patch core is line-based, so a value carrying a
/// raw newline would desynchronize the block. Task titles/assignees are
/// single-line by construction; this is a guard, not a feature.
fn render_scalar(s: &str) -> String {
    let flat = s.replace("\r\n", " ").replace(['\n', '\r'], " ");
    if scalar_needs_quoting(&flat) {
        format!("'{}'", flat.replace('\'', "''"))
    } else {
        flat
    }
}

/// `key: value` as one physical line. Public because [`patch_text`] and
/// [`apply_edits`] take *rendered* lines — a caller patching a key this
/// module doesn't model (a future frontmatter key, an unknown key a tool
/// wants to set) needs the same quoting rules.
pub fn scalar_lines(key: &str, value: &str) -> Vec<String> {
    vec![format!("{key}: {}", render_scalar(value))]
}

/// A block sequence (`key:` + `  - item` lines), or `key: []` when empty.
/// Multi-line by design — this is the case the S6 prototype's TODO was
/// about on the *write* side.
pub fn seq_lines(key: &str, items: &[String]) -> Vec<String> {
    if items.is_empty() {
        return vec![format!("{key}: []")];
    }
    let mut out = vec![format!("{key}:")];
    for item in items {
        out.push(format!("  - {}", render_scalar(item)));
    }
    out
}

/// Split text into `(content, terminator)` pairs. The terminator is `""`
/// only for a final line with no newline, so joining the pairs back
/// reproduces the input byte-for-byte.
fn split_lines(s: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut off = 0usize;
    while off < s.len() {
        let end = s[off..].find('\n').map(|i| off + i + 1).unwrap_or(s.len());
        let line = &s[off..end];
        let pair = if let Some(stripped) = line.strip_suffix("\r\n") {
            (stripped, &line[line.len() - 2..])
        } else if let Some(stripped) = line.strip_suffix('\n') {
            (stripped, &line[line.len() - 1..])
        } else {
            (line, "")
        };
        out.push(pair);
        off = end;
    }
    out
}

/// The key a top-level frontmatter line declares, if any. Indented lines,
/// comments, blank lines, and sequence items are all `None` — only column-0
/// `key:` / `key: value` lines are patch targets, so nested mappings are
/// never mistaken for the keys we own.
fn top_level_key(content: &str) -> Option<&str> {
    if content.starts_with(' ') || content.starts_with('\t') {
        return None;
    }
    let trimmed = content.trim_end();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
        return None;
    }
    let colon = trimmed.find(':')?;
    let key = &trimmed[..colon];
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return None;
    }
    let after = &trimmed[colon + 1..];
    if after.is_empty() || after.starts_with(' ') || after.starts_with('\t') {
        Some(key)
    } else {
        None
    }
}

/// The dominant line terminator of a file — CRLF files stay CRLF (S6:
/// candidate A "never rewrites terminators of untouched lines"; newly
/// *added* lines have to pick one, and matching the file is the only
/// answer that keeps a diff clean).
fn detect_eol(raw: &str) -> &'static str {
    match raw.find('\n') {
        Some(i) if i > 0 && raw.as_bytes()[i - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

/// The byte-faithful patch core (S6 candidate A). Replaces the physical
/// lines of each named key in `edits`, appends keys the file doesn't have
/// yet (in `edits` order), and optionally appends text to the end of the
/// body. Everything else — comments, key order, indentation, quoting
/// style, unknown keys, terminators, and the body — passes through
/// untouched.
///
/// A file with no frontmatter block gets one prepended (the case
/// serde_yaml had no fallback for in S6); its whole content becomes the
/// body, unchanged.
pub fn patch_text(raw: &str, edits: &[(&str, Vec<String>)], append_body: Option<&str>) -> String {
    let eol = detect_eol(raw);

    let (open, fm, close, body) = match split_raw(raw) {
        Some(s) => (
            s.open.to_string(),
            s.fm.to_string(),
            s.close.to_string(),
            s.body.to_string(),
        ),
        None => (
            format!("---{eol}"),
            String::new(),
            format!("---{eol}{eol}"),
            raw.to_string(),
        ),
    };

    let lines = split_lines(&fm);
    let mut written = vec![false; edits.len()];
    let mut out = String::with_capacity(raw.len() + 128);
    let mut i = 0usize;

    while i < lines.len() {
        let (content, term) = lines[i];
        let hit = top_level_key(content).and_then(|k| edits.iter().position(|(ek, _)| *ek == k));
        match hit {
            Some(idx) if !written[idx] => {
                let t = if term.is_empty() { eol } else { term };
                for line in &edits[idx].1 {
                    out.push_str(line);
                    out.push_str(t);
                }
                written[idx] = true;
                // Consume this key's whole physical extent: following
                // indented lines, column-0 sequence items, and interior
                // blank lines (S6's multi-line-value TODO). Trailing blank
                // lines before the next key stay where they are.
                i += 1;
                let mut j = i;
                let mut last = i;
                while j < lines.len() {
                    let c = lines[j].0;
                    if c.trim().is_empty() {
                        j += 1;
                        continue;
                    }
                    let continuation =
                        c.starts_with(' ') || c.starts_with('\t') || c.starts_with('-');
                    if continuation {
                        j += 1;
                        last = j;
                    } else {
                        break;
                    }
                }
                i = last;
            }
            _ => {
                out.push_str(content);
                out.push_str(term);
                i += 1;
            }
        }
    }

    for (idx, (_, value_lines)) in edits.iter().enumerate() {
        if written[idx] {
            continue;
        }
        for line in value_lines {
            out.push_str(line);
            out.push_str(eol);
        }
    }

    let mut result = String::with_capacity(raw.len() + 256);
    result.push_str(&open);
    result.push_str(&out);
    result.push_str(&close);
    result.push_str(&body);

    if let Some(extra) = append_body {
        if !result.ends_with('\n') {
            result.push_str(eol);
        }
        if !result.ends_with(&format!("{eol}{eol}")) {
            result.push_str(eol);
        }
        result.push_str(&extra.replace("\r\n", "\n").replace('\n', eol));
        if !result.ends_with(eol) {
            result.push_str(eol);
        }
    }
    result
}

/// len + mtime + content hash — the optimistic-concurrency precondition
/// S6 requires. mtime alone is too coarse (Windows FAT/NTFS granularity
/// and same-millisecond writes), so the content hash is the real check and
/// the metadata is the cheap early-out.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Fingerprint {
    len: u64,
    mtime_ms: Option<u128>,
    hash: u64,
}

fn fingerprint(path: &Path) -> Result<(String, Fingerprint)> {
    let raw = fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    let meta = fs::metadata(path).map_err(|e| Error::io(path, e))?;
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis());
    let fp = Fingerprint {
        len: meta.len(),
        mtime_ms,
        hash: twox_hash::XxHash64::oneshot(0x4B454E5F5441534B, raw.as_bytes()), // "KEN_TASK"
    };
    Ok((raw, fp))
}

/// A body-append computed from the *current* body, so a retry recomputes
/// it against whatever the other writer left behind (see
/// [`compose_log_entry`], which only adds a `## Log` heading if the body
/// doesn't already have one).
pub type BodyAppend<'a> = &'a dyn Fn(&str) -> String;

/// Read → patch → verify-unchanged → write, retrying up to
/// [`PATCH_MAX_ATTEMPTS`] times when the file changed underneath us (S6:
/// "read-then-write alone drops ~1 in 6 concurrent external edits").
///
/// A patch that produces byte-identical text writes nothing at all, so a
/// no-op update never wakes the watcher.
pub fn apply_edits(
    path: &Path,
    edits: &[(&str, Vec<String>)],
    append_body: Option<BodyAppend>,
) -> Result<()> {
    apply_text_change(path, &|raw: &str| {
        let appended = append_body.map(|f| {
            let body = split_raw(raw).map(|s| s.body.to_string()).unwrap_or_else(|| raw.to_string());
            f(&body)
        });
        patch_text(raw, edits, appended.as_deref())
    })
}

/// The frontmatter block and the body of a task file, as `(frontmatter,
/// body)` — `None` when the file has no (terminated) frontmatter block.
/// Read-side only; writes go through [`patch_text`] / [`replace_body`].
pub fn split_frontmatter(raw: &str) -> Option<(&str, &str)> {
    split_raw(raw).map(|s| (s.fm, s.body))
}

/// `raw` with its body replaced by `body` and every frontmatter byte kept.
/// The new body follows the closing `---` after one blank line (the shape
/// new files are written in) and ends with one terminator; an empty `body`
/// leaves just that blank line. Line endings follow the file's own.
pub fn replace_body(raw: &str, body: &str) -> String {
    let eol = detect_eol(raw);
    let text = body.trim().replace("\r\n", "\n").replace('\n', eol);
    let tail = if text.is_empty() { eol.to_string() } else { format!("{eol}{text}{eol}") };
    match split_raw(raw) {
        Some(s) => {
            let close = if s.close.ends_with('\n') { s.close.to_string() } else { format!("{}{eol}", s.close) };
            format!("{}{}{close}{tail}", s.open, s.fm)
        }
        None => format!("---{eol}---{eol}{tail}"),
    }
}

/// The optimistic-concurrency write loop behind [`apply_edits`], for any
/// pure text change: read, compute `change(raw)`, check the file did not
/// move underneath, write. A change that produces the same text writes
/// nothing.
pub fn apply_text_change(path: &Path, change: &dyn Fn(&str) -> String) -> Result<()> {
    for _ in 0..PATCH_MAX_ATTEMPTS {
        let (raw, before) = fingerprint(path)?;
        let next = change(&raw);
        if next == raw {
            return Ok(());
        }
        let (_, now) = fingerprint(path)?;
        if now != before {
            continue; // someone else wrote between our read and our write
        }
        fs::write(path, &next).map_err(|e| Error::io(path, e))?;
        return Ok(());
    }
    Err(Error::Other(format!(
        "task file changed concurrently {PATCH_MAX_ATTEMPTS} times, giving up: {}",
        path.display()
    )))
}

// ---------------------------------------------------------------------
// 1.2 Typed patches
// ---------------------------------------------------------------------

/// The keys a `task_update` may name. `None` means "leave alone" — the
/// whole point of the patch core is that unnamed keys are never touched.
/// There is deliberately no "remove key" variant: clearing a value means
/// setting it empty (`assignee: ''`), which keeps the line — and therefore
/// the file's key order — stable.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub status: Option<TaskStatus>,
    pub kind: Option<TaskKind>,
    pub assignee: Option<String>,
    pub project: Option<String>,
    pub tags: Option<Vec<String>>,
    pub due: Option<String>,
    pub board: Option<BoardKind>,
}

impl TaskPatch {
    pub fn is_empty(&self) -> bool {
        *self == TaskPatch::default()
    }

    /// True when this patch resolves the `status` key one way or the other
    /// — the escape hatch [`apply_patch`] allows for a ticket whose
    /// on-disk status it doesn't understand.
    pub fn sets_status(&self) -> bool {
        self.status.is_some()
    }

    /// Rendered edit lines in canonical key order. `updated` is appended by
    /// [`apply_patch`], not here, so this stays a pure view of the caller's
    /// intent.
    fn edits(&self) -> Vec<(&'static str, Vec<String>)> {
        let mut out: Vec<(&'static str, Vec<String>)> = Vec::new();
        if let Some(v) = &self.title {
            out.push(("title", scalar_lines("title", v)));
        }
        if let Some(v) = self.status {
            out.push(("status", scalar_lines("status", v.as_str())));
        }
        if let Some(v) = self.kind {
            out.push(("kind", scalar_lines("kind", v.as_str())));
        }
        if let Some(v) = &self.assignee {
            out.push(("assignee", scalar_lines("assignee", v)));
        }
        if let Some(v) = &self.project {
            out.push(("project", scalar_lines("project", v)));
        }
        if let Some(v) = &self.tags {
            out.push(("tags", seq_lines("tags", v)));
        }
        if let Some(v) = &self.due {
            out.push(("due", scalar_lines("due", v)));
        }
        if let Some(v) = self.board {
            out.push(("board", scalar_lines("board", v.as_str())));
        }
        out
    }
}

/// Rewrite only the patch's named keys plus `updated` (spec: "patches SHALL
/// rewrite only the named keys plus `updated`"; drag-drop = `status` +
/// `updated`).
///
/// Refuses to touch a file whose on-disk `status` is out of vocabulary
/// unless the patch itself sets `status` — that is what "the file is not
/// rewritten" means for a needs-attention task: Ken never edits around a
/// hand edit it doesn't understand, but an explicit fix is always allowed.
pub fn apply_patch(path: &Path, patch: &TaskPatch, updated: &str) -> Result<()> {
    let raw = fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    let current = parse_task(path, HomeKind::Workspace, "", &raw);
    if current.has_invalid_status() && !patch.sets_status() {
        return Err(Error::Other(format!(
            "task '{}' has an unrecognized status '{}' — resolve it before patching other keys",
            current.id, current.status_raw
        )));
    }
    let mut edits = patch.edits();
    edits.push(("updated", scalar_lines("updated", updated)));
    apply_edits(path, &edits, None)
}

// ---------------------------------------------------------------------
// 1.1 / 1.6 Creation
// ---------------------------------------------------------------------

/// Crockford base32 (no I, L, O, U) — the ULID alphabet.
const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Encode a ULID from its parts: 48-bit millisecond timestamp + 80 bits of
/// entropy, 26 Crockford-base32 characters, most significant first.
pub fn ulid_from_parts(unix_ms: u64, entropy: [u8; 10]) -> String {
    let mut s = String::with_capacity(26);
    let ts = unix_ms & 0xFFFF_FFFF_FFFF;
    for i in 0..10 {
        let shift = 45 - 5 * i;
        s.push(CROCKFORD[((ts >> shift) & 0x1F) as usize] as char);
    }
    let mut rand: u128 = 0;
    for b in entropy {
        rand = (rand << 8) | b as u128;
    }
    for i in 0..16 {
        let shift = 75 - 5 * i;
        s.push(CROCKFORD[((rand >> shift) & 0x1F) as usize] as char);
    }
    s
}

/// A fresh ULID from the wall clock plus 80 bits of `Uuid::new_v4`
/// randomness. No `ulid` crate is in this workspace's dependency tree and
/// adding one for 20 lines of base32 isn't worth it; `uuid` (already a
/// dependency, `v4` feature) is the entropy source.
///
/// Tests never call this — [`NewTask::id`] lets the caller supply the id,
/// the same caller-supplies-nondeterminism convention `memory.rs` uses for
/// dates.
pub fn new_ulid() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let bytes = *uuid::Uuid::new_v4().as_bytes();
    let mut entropy = [0u8; 10];
    entropy.copy_from_slice(&bytes[..10]);
    ulid_from_parts(ms, entropy)
}

const SLUG_MAX: usize = 40;

/// Kebab-case filename tail for a task/goal title. Kept local rather than
/// reusing `research::slugify`: that one caps at 50 and falls back to the
/// literal string `"research"`, which would be a confusing task filename.
pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let mut slug = out.trim_matches('-').to_string();
    if slug.len() > SLUG_MAX {
        let end = SLUG_MAX;
        let cut = slug[..end].rfind('-').unwrap_or(end);
        slug.truncate(cut);
    }
    if slug.is_empty() {
        "task".into()
    } else {
        slug
    }
}

/// `<ulid>-<slug>.md` (D1: "the `id` in frontmatter is authoritative; the
/// filename is for humans").
pub fn task_file_name(id: &str, title: &str) -> String {
    format!("{id}-{}.md", slugify(title))
}

/// A task to create. `id` is caller-supplied-or-generated so tests can be
/// deterministic (mirrors `memory.rs`'s caller-supplied `today`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewTask {
    pub id: Option<String>,
    pub title: String,
    pub body: String,
    pub fields: TaskPatch,
}

/// Create a task file in `home`. Status defaults to `backlog` — the intake
/// column (spec: "Task creation without an explicit status SHALL default
/// to `backlog`") — kind to `human`, board to `main`, and
/// `created`/`updated` to the caller-supplied `today`. `due`/`goal` keys
/// are emitted only when set.
pub fn create_task(home: TaskHome, new: &NewTask, today: &str) -> Result<Task> {
    let title = new.title.trim();
    if title.is_empty() {
        return Err(Error::Other("a task needs a title".into()));
    }
    let id = match &new.id {
        Some(i) if !i.trim().is_empty() => i.trim().to_string(),
        _ => new_ulid(),
    };
    let dir = home.tasks_dir();
    let path = dir.join(task_file_name(&id, title));
    if path.exists() {
        return Err(Error::Other(format!(
            "a task file already exists at {}",
            path.display()
        )));
    }

    let f = &new.fields;
    let mut fm = String::from("---\n");
    fm.push_str(&format!("id: {}\n", render_scalar(&id)));
    fm.push_str(&format!("title: {}\n", render_scalar(title)));
    fm.push_str(&format!(
        "status: {}\n",
        f.status.unwrap_or(TaskStatus::Backlog).as_str()
    ));
    fm.push_str(&format!(
        "kind: {}\n",
        f.kind.unwrap_or(TaskKind::Human).as_str()
    ));
    fm.push_str(&format!(
        "assignee: {}\n",
        render_scalar(f.assignee.as_deref().unwrap_or(""))
    ));
    fm.push_str(&format!(
        "project: {}\n",
        render_scalar(
            f.project
                .as_deref()
                .unwrap_or_else(|| home.default_project())
        )
    ));
    for line in seq_lines("tags", f.tags.as_deref().unwrap_or(&[])) {
        fm.push_str(&line);
        fm.push('\n');
    }
    if let Some(due) = f.due.as_deref().filter(|d| !d.trim().is_empty()) {
        fm.push_str(&format!("due: {}\n", render_scalar(due)));
    }
    fm.push_str(&format!(
        "board: {}\n",
        f.board.unwrap_or(BoardKind::Main).as_str()
    ));
    fm.push_str(&format!("created: {}\n", render_scalar(today)));
    fm.push_str(&format!("updated: {}\n", render_scalar(today)));
    fm.push_str("---\n\n");
    fm.push_str(new.body.trim());
    if !new.body.trim().is_empty() {
        fm.push('\n');
    }

    fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    fs::write(&path, &fm).map_err(|e| Error::io(&path, e))?;
    Ok(parse_task(&path, home.kind(), home.default_project(), &fm))
}

// ---------------------------------------------------------------------
// 1.3 Scanning + aggregation
// ---------------------------------------------------------------------

/// Parse every `.md` file directly in `home`'s tasks dir, sorted by
/// filename for determinism. Not recursive — `archive/` and `goals/` are
/// subfolders and are therefore skipped for free. A missing folder reads as
/// no tasks (homes are created lazily on first write, and a per-repo home
/// that doesn't exist is simply not opted in).
pub fn list_tasks(home: TaskHome) -> Result<Vec<Task>> {
    let dir = home.tasks_dir();
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| Error::io(&dir, e))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "md"))
        .collect();
    paths.sort();
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        let raw = fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        out.push(parse_task(&path, home.kind(), home.default_project(), &raw));
    }
    Ok(out)
}

/// Scan every home into one board list (D2: "the board scans both and
/// treats home as invisible plumbing"). Homes are scanned in the order
/// given; duplicate `id`s (e.g. a task file copied between homes) collapse
/// to the first occurrence, because `id` — not the path — is a task's
/// identity.
pub fn scan_tasks(homes: &[TaskHome]) -> Result<Vec<Task>> {
    let mut out: Vec<Task> = Vec::new();
    for home in homes {
        for task in list_tasks(*home)? {
            if !out.iter().any(|t| t.id == task.id) {
                out.push(task);
            }
        }
    }
    Ok(out)
}

/// Find a task by its authoritative `id` (never by filename).
pub fn find_by_id<'a>(tasks: &'a [Task], id: &str) -> Option<&'a Task> {
    tasks.iter().find(|t| t.id == id)
}

// ---------------------------------------------------------------------
// 1.4 Archive, log append, journal line
// ---------------------------------------------------------------------

/// True for a strict `YYYY-MM-DD` string. Date arithmetic isn't needed
/// anywhere in this module — ISO dates compare correctly as plain strings,
/// so `memory.rs`'s `days_from_civil` machinery stays where it is.
fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..].iter().all(u8::is_ascii_digit)
}

/// `YYYY-MM` from a `YYYY-MM-DD` date — the archive month bucket.
pub fn archive_month(date: &str) -> Result<String> {
    if !is_iso_date(date) {
        return Err(Error::Other(format!(
            "invalid date '{date}' — expected YYYY-MM-DD"
        )));
    }
    Ok(date[..7].to_string())
}

/// Where a task archives to: `<its own home>/archive/YYYY-MM/<same
/// filename>` (D2 — per-repo history ships with the repo).
pub fn archive_target(task: &Task, today: &str) -> Result<PathBuf> {
    let month = archive_month(today)?;
    Ok(archive_dir(&task.home_dir, &month).join(task.file_name()))
}

/// Move a task file into its home's `archive/YYYY-MM/`. A name collision in
/// the target month (same task archived, restored, and archived again)
/// gets a `-2`, `-3`, … suffix rather than clobbering history.
pub fn archive_task(task: &Task, today: &str) -> Result<PathBuf> {
    archive_file(&task.path, &task.home_dir, today)
}

/// [`archive_task`] for any task file: `path` moves into
/// `<home_dir>/archive/YYYY-MM/`, keeping its name (or `-2`, `-3`, … when
/// that name is taken).
pub fn archive_file(path: &Path, home_dir: &Path, today: &str) -> Result<PathBuf> {
    let month = archive_month(today)?;
    let dir = archive_dir(home_dir, &month);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "task.md".into());
    let mut target = dir.join(&name);
    fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    if target.exists() {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "task".into());
        for n in 2..1000 {
            let candidate = dir.join(format!("{stem}-{n}.md"));
            if !candidate.exists() {
                target = candidate;
                break;
            }
        }
    }
    fs::rename(path, &target).map_err(|e| Error::io(path, e))?;
    Ok(target)
}

/// The text `task_complete` appends to a task body: a `## Log` heading if
/// the body doesn't already have one, then a `### <date> <time>` entry with
/// the report verbatim (D4).
pub fn compose_log_entry(body: &str, report: &str, today: &str, time_hhmm: &str) -> String {
    let has_heading = body
        .lines()
        .any(|l| l.trim_end().eq_ignore_ascii_case(LOG_HEADING));
    let mut out = String::new();
    if !has_heading {
        out.push_str(LOG_HEADING);
        out.push_str("\n\n");
    }
    out.push_str(&format!("### {today} {time_hhmm}\n\n"));
    out.push_str(report.trim());
    out.push('\n');
    out
}

/// Longest report excerpt carried into the journal — the journal line is a
/// pointer, not a copy (the full report lives in the task's `## Log`).
const JOURNAL_EXCERPT_CHARS: usize = 160;

/// The one-line journal summary `task_complete` writes when `kenMemory` is
/// on (D4: "a one-line journal summary linking the task's `ken://`
/// address"). Pure text composition — the caller passes it to
/// `memory::append_journal`.
pub fn journal_summary_line(task: &Task, host: &str, report: &str) -> String {
    let excerpt = report
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let excerpt: String = if excerpt.chars().count() > JOURNAL_EXCERPT_CHARS {
        let cut: String = excerpt.chars().take(JOURNAL_EXCERPT_CHARS).collect();
        format!("{}…", cut.trim_end())
    } else {
        excerpt.to_string()
    };
    let address = task.address(host);
    if excerpt.is_empty() {
        format!("Completed task \"{}\" ({address})", task.title)
    } else {
        format!("Completed task \"{}\" — {excerpt} ({address})", task.title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{tempdir, TempDir};

    fn ws(dir: &TempDir) -> TaskHome<'_> {
        TaskHome::Workspace {
            workspace_root: dir.path(),
        }
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    fn task_at(path: &Path) -> Task {
        parse_task(path, HomeKind::Workspace, "", &read(path))
    }

    /// The adversarial corpus in one file: a comment, every modeled key,
    /// a block sequence, unknown scalar and *nested* unknown keys, and a
    /// body with load-bearing whitespace. Built with `concat!` rather than
    /// a raw string so the literal stays LF regardless of how the source
    /// file is checked out (the CRLF case is exercised explicitly below).
    const RICH: &str = concat!(
        "---\n",
        "# a hand-written comment\n",
        "id: 01J0AAAAAAAAAAAAAAAAAAAAAA\n",
        "title: Decompile the mob spawner\n",
        "status: todo\n",
        "kind: ai\n",
        "assignee: ''\n",
        "project: ShatteredRealms\n",
        "tags:\n",
        "  - hytale\n",
        "  - decomp\n",
        "due: '2026-08-30'\n",
        "board: main\n",
        "created: '2026-08-01'\n",
        "updated: '2026-08-01'\n",
        "severity: high\n",
        "links:\n",
        "  upstream: https://example.invalid/x\n",
        "---\n",
        "\n",
        "# Decompile the mob spawner\n",
        "\n",
        "Some   *hand-written*   body   with   odd  spacing.\n",
        "\n",
        "- [ ] find the entry point\n",
    );

    // ---- 1.1 parse / round-trip ----

    #[test]
    fn parses_full_frontmatter() {
        let t = parse_task(Path::new("/w/tasks/01J0-x.md"), HomeKind::Workspace, "", RICH);
        assert_eq!(t.id, "01J0AAAAAAAAAAAAAAAAAAAAAA");
        assert_eq!(t.title, "Decompile the mob spawner");
        assert_eq!(t.status, Some(TaskStatus::Todo));
        assert_eq!(t.kind, TaskKind::Ai);
        assert_eq!(t.assignee, "");
        assert_eq!(t.project, "ShatteredRealms");
        assert_eq!(t.tags, vec!["hytale".to_string(), "decomp".to_string()]);
        assert_eq!(t.due.as_deref(), Some("2026-08-30"));
        assert_eq!(t.board, BoardKind::Main);
        assert!(t.body.contains("odd  spacing"));
        assert!(t.extra().contains_key(serde_yaml::Value::String("severity".into())));
    }

    #[test]
    fn no_frontmatter_file_still_parses() {
        let t = parse_task(
            Path::new("/w/tasks/hand-made.md"),
            HomeKind::Workspace,
            "",
            "# Look at the loot tables\n\nnotes\n",
        );
        assert_eq!(t.id, "hand-made", "id falls back to the file stem");
        assert_eq!(t.title, "Look at the loot tables");
        assert_eq!(t.status, Some(TaskStatus::Backlog), "absent status = intake");
    }

    #[test]
    fn id_is_authoritative_over_filename() {
        let a = parse_task(
            Path::new("/w/tasks/01J0AAAAAAAAAAAAAAAAAAAAAA-typoed-slgu.md"),
            HomeKind::Workspace,
            "",
            RICH,
        );
        let b = parse_task(
            Path::new("/w/tasks/01J0AAAAAAAAAAAAAAAAAAAAAA-fixed-slug.md"),
            HomeKind::Workspace,
            "",
            RICH,
        );
        assert_eq!(a.id, b.id);
        // Aggregation keys on id, so a rename is a no-op for the board.
        assert_eq!(find_by_id(&[a, b], "01J0AAAAAAAAAAAAAAAAAAAAAA").is_some(), true);
    }

    #[test]
    fn tolerates_scalar_tags_hand_edit() {
        let raw = "---\nid: t1\ntitle: T\nstatus: todo\ntags: alpha, beta\n---\n\nbody\n";
        let t = parse_task(Path::new("/w/tasks/t1.md"), HomeKind::Workspace, "", raw);
        assert_eq!(t.title, "T", "a bad tags shape must not blank the file");
        assert_eq!(t.tags, vec!["alpha".to_string(), "beta".to_string()]);
    }

    // ---- 1.2 patch core: byte fidelity ----

    #[test]
    fn patch_touches_only_named_keys() {
        let next = patch_text(
            RICH,
            &[
                ("status", scalar_lines("status", "doing")),
                ("updated", scalar_lines("updated", "2026-08-03")),
            ],
            None,
        );
        let before: Vec<&str> = RICH.lines().collect();
        let after: Vec<&str> = next.lines().collect();
        assert_eq!(before.len(), after.len(), "no lines added or removed");
        let changed: Vec<(usize, &str, &str)> = before
            .iter()
            .zip(after.iter())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| (i, *a, *b))
            .collect();
        assert_eq!(changed.len(), 2, "changed: {changed:?}");
        assert_eq!(changed[0].2, "status: doing");
        assert_eq!(changed[1].2, "updated: '2026-08-03'");
    }

    #[test]
    fn patch_preserves_comments_unknown_keys_and_body() {
        let next = patch_text(RICH, &[("status", scalar_lines("status", "review"))], None);
        assert!(next.contains("# a hand-written comment"));
        assert!(next.contains("severity: high"));
        assert!(next.contains("  upstream: https://example.invalid/x"));
        assert!(next.contains("Some   *hand-written*   body   with   odd  spacing."));
        assert!(next.contains("- [ ] find the entry point"));
    }

    #[test]
    fn patch_preserves_crlf() {
        let crlf = RICH.replace('\n', "\r\n");
        let next = patch_text(&crlf, &[("status", scalar_lines("status", "done"))], None);
        assert!(next.contains("status: done\r\n"));
        assert!(!next.contains("status: done\n\r"), "no mangled terminators");
        assert_eq!(next.matches('\n').count(), next.matches("\r\n").count());
    }

    #[test]
    fn patch_replaces_multi_line_block_sequence_as_a_unit() {
        // S6's open TODO: patching a key whose value spans several lines.
        let next = patch_text(
            RICH,
            &[("tags", seq_lines("tags", &["one".into(), "two".into(), "three".into()]))],
            None,
        );
        assert!(next.contains("tags:\n  - one\n  - two\n  - three\n"));
        assert!(!next.contains("- hytale"), "old items must not linger: {next}");
        assert!(next.contains("due: '2026-08-30'"), "next key survives");
    }

    #[test]
    fn patch_replaces_multi_line_block_scalar_as_a_unit() {
        let raw = "---\nid: t1\nnotes: |\n  line one\n  line two\n\nstatus: todo\n---\n\nbody\n";
        let next = patch_text(raw, &[("notes", scalar_lines("notes", "flat"))], None);
        assert!(next.contains("notes: flat\n"));
        assert!(!next.contains("line one"));
        assert!(next.contains("status: todo"));
    }

    #[test]
    fn patch_adds_a_missing_key_at_the_end_of_frontmatter() {
        let raw = "---\nid: t1\nstatus: todo\n---\n\nbody\n";
        let next = patch_text(raw, &[("goal", scalar_lines("goal", "g1"))], None);
        assert_eq!(next, "---\nid: t1\nstatus: todo\ngoal: g1\n---\n\nbody\n");
    }

    #[test]
    fn patch_creates_frontmatter_when_absent() {
        let raw = "# Just a note\n\nno frontmatter here\n";
        let next = patch_text(raw, &[("status", scalar_lines("status", "todo"))], None);
        assert_eq!(next, "---\nstatus: todo\n---\n\n# Just a note\n\nno frontmatter here\n");
    }

    #[test]
    fn patch_ignores_nested_keys_with_the_same_name() {
        let raw = "---\nid: t1\nstatus: todo\nmeta:\n  status: nested\n---\n\nbody\n";
        let next = patch_text(raw, &[("status", scalar_lines("status", "done"))], None);
        assert!(next.contains("status: done\n"));
        assert!(next.contains("  status: nested"), "nested key untouched: {next}");
    }

    #[test]
    fn patch_is_idempotent() {
        let once = patch_text(RICH, &[("status", scalar_lines("status", "doing"))], None);
        let twice = patch_text(&once, &[("status", scalar_lines("status", "doing"))], None);
        assert_eq!(once, twice);
    }

    #[test]
    fn quoting_is_conservative_but_round_trips() {
        assert_eq!(render_scalar("todo"), "todo");
        assert_eq!(render_scalar(""), "''");
        assert_eq!(render_scalar("2026-08-01"), "'2026-08-01'");
        assert_eq!(render_scalar("no"), "'no'");
        assert_eq!(render_scalar("it's"), "'it''s'");
        assert_eq!(render_scalar("ken://a/b"), "'ken://a/b'");
        // Newlines fold to spaces first, so the result is a plain scalar.
        assert_eq!(render_scalar("multi\nline"), "multi line");
        assert_eq!(render_scalar("multi\r\nline: x"), "'multi line: x'");
        // Everything above must survive a serde_yaml read.
        for v in ["", "2026-08-01", "no", "it's", "ken://a/b", "plain value"] {
            let raw = format!("---\nid: x\ntitle: {}\n---\n\nb\n", render_scalar(v));
            let t = parse_task(Path::new("/w/tasks/x.md"), HomeKind::Workspace, "", &raw);
            let expected = if v.is_empty() { "b" } else { v };
            assert_eq!(t.title, expected, "round trip of {v:?} via {raw}");
        }
    }

    // ---- 1.2 apply_patch on disk ----

    #[test]
    fn apply_patch_writes_only_status_and_updated() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("tasks/01J0-x.md");
        write(&path, RICH);
        apply_patch(
            &path,
            &TaskPatch {
                status: Some(TaskStatus::Doing),
                ..TaskPatch::default()
            },
            "2026-08-03",
        )
        .unwrap();
        let after = read(&path);
        let diff = RICH
            .lines()
            .zip(after.lines())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(diff, 2, "{after}");
        assert_eq!(task_at(&path).status, Some(TaskStatus::Doing));
    }

    #[test]
    fn dragging_twice_still_diffs_only_two_lines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("tasks/01J0-x.md");
        write(&path, RICH);
        for (status, day) in [(TaskStatus::Doing, "2026-08-03"), (TaskStatus::Review, "2026-08-04")]
        {
            let before = read(&path);
            apply_patch(
                &path,
                &TaskPatch {
                    status: Some(status),
                    ..TaskPatch::default()
                },
                day,
            )
            .unwrap();
            let after = read(&path);
            let changed = before.lines().zip(after.lines()).filter(|(a, b)| a != b).count();
            assert_eq!(changed, 2, "{after}");
            assert_eq!(before.lines().count(), after.lines().count());
        }
        assert!(read(&path).contains("severity: high"));
    }

    #[test]
    fn no_op_patch_does_not_rewrite_the_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("tasks/01J0-x.md");
        write(&path, RICH);
        apply_patch(
            &path,
            &TaskPatch {
                status: Some(TaskStatus::Todo),
                ..TaskPatch::default()
            },
            "2026-08-01",
        )
        .unwrap();
        assert_eq!(read(&path), RICH, "byte-identical, no watcher churn");
    }

    #[test]
    fn invalid_status_is_surfaced_and_the_file_is_not_rewritten() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("tasks/01J0-x.md");
        let raw = RICH.replace("status: todo", "status: blocked");
        write(&path, &raw);

        let t = task_at(&path);
        assert!(t.has_invalid_status());
        assert_eq!(t.status_raw, "blocked");

        // A patch that doesn't resolve the status is refused outright.
        let err = apply_patch(
            &path,
            &TaskPatch {
                assignee: Some("agent-desktop".into()),
                ..TaskPatch::default()
            },
            "2026-08-03",
        )
        .unwrap_err();
        assert!(err.to_string().contains("unrecognized status"), "{err}");
        assert_eq!(read(&path), raw, "file untouched");

        // Explicitly fixing it is allowed.
        apply_patch(
            &path,
            &TaskPatch {
                status: Some(TaskStatus::Todo),
                ..TaskPatch::default()
            },
            "2026-08-03",
        )
        .unwrap();
        assert_eq!(task_at(&path).status, Some(TaskStatus::Todo));
    }

    #[test]
    fn concurrent_change_between_read_and_write_retries_and_wins() {
        // Direct check of the S6 precondition: a file that changes between
        // the fingerprint and the write is re-read, not clobbered.
        let dir = tempdir().unwrap();
        let path = dir.path().join("tasks/01J0-x.md");
        write(&path, RICH);
        // An "external writer" adds a key; our patch must keep it.
        let external = read(&path).replace("severity: high", "severity: high\nexternal: kept");
        write(&path, &external);
        apply_patch(
            &path,
            &TaskPatch {
                status: Some(TaskStatus::Done),
                ..TaskPatch::default()
            },
            "2026-08-05",
        )
        .unwrap();
        let after = read(&path);
        assert!(after.contains("external: kept"));
        assert!(after.contains("status: done"));
    }

    // ---- 1.1 / 1.6 creation ----

    #[test]
    fn create_defaults_to_backlog_and_round_trips() {
        let dir = tempdir().unwrap();
        let t = create_task(
            ws(&dir),
            &NewTask {
                id: Some("01J0TESTTESTTESTTESTTESTTE".into()),
                title: "Groom the backlog".into(),
                body: "Check the intake column.".into(),
                fields: TaskPatch::default(),
            },
            "2026-08-03",
        )
        .unwrap();
        assert_eq!(t.status, Some(TaskStatus::Backlog));
        assert_eq!(t.kind, TaskKind::Human);
        assert_eq!(t.board, BoardKind::Main);
        assert_eq!(t.created, "2026-08-03");
        assert_eq!(
            t.path.file_name().unwrap().to_string_lossy(),
            "01J0TESTTESTTESTTESTTESTTE-groom-the-backlog.md"
        );
        assert_eq!(task_at(&t.path), task_at(&t.path));
        assert_eq!(task_at(&t.path).title, "Groom the backlog");
        assert!(read(&t.path).contains("Check the intake column."));
    }

    #[test]
    fn create_omits_optional_keys_and_defaults_project_from_home() {
        let dir = tempdir().unwrap();
        let home = TaskHome::Project {
            project_root: dir.path(),
            project: "ShatteredRealms",
        };
        let t = create_task(
            home,
            &NewTask {
                id: Some("01J0PROJPROJPROJPROJPROJPR".into()),
                title: "Ship it".into(),
                ..NewTask::default()
            },
            "2026-08-03",
        )
        .unwrap();
        let raw = read(&t.path);
        assert!(!raw.contains("due:"));
        assert!(!raw.contains("goal:"));
        assert!(raw.contains("tags: []"));
        assert_eq!(t.project, "ShatteredRealms", "defaulted from the home");
        assert!(t.path.ends_with(".ken/tasks/01J0PROJPROJPROJPROJPROJPR-ship-it.md"));
    }

    #[test]
    fn ulid_encoding_is_26_crockford_chars() {
        let id = ulid_from_parts(0x0192_3456_789A, [0xFF; 10]);
        assert_eq!(id.len(), 26);
        assert!(id.chars().all(|c| CROCKFORD.contains(&(c as u8))));
        assert_eq!(&id[10..], "ZZZZZZZZZZZZZZZZ");
        assert_eq!(ulid_from_parts(0, [0; 10]), "0".repeat(26));
        assert_ne!(new_ulid(), new_ulid());
    }

    // ---- 1.3 scanning + aggregation ----

    fn seed_board(dir: &TempDir) -> (Vec<Task>, PathBuf) {
        let wsroot = dir.path().join("ws");
        let proj = dir.path().join("ws/ShatteredRealms");
        let mk = |home: TaskHome, id: &str, title: &str, fields: TaskPatch| {
            create_task(
                home,
                &NewTask {
                    id: Some(id.into()),
                    title: title.into(),
                    body: String::new(),
                    fields,
                },
                "2026-08-01",
            )
            .unwrap();
        };
        let w = TaskHome::Workspace {
            workspace_root: &wsroot,
        };
        let p = TaskHome::Project {
            project_root: &proj,
            project: "ShatteredRealms",
        };
        mk(
            w,
            "01J0W1",
            "Workspace todo",
            TaskPatch {
                status: Some(TaskStatus::Todo),
                kind: Some(TaskKind::Ai),
                project: Some("ItemSearch".into()),
                tags: Some(vec!["alpha".into()]),
                ..TaskPatch::default()
            },
        );
        mk(
            w,
            "01J0W2",
            "Workspace done",
            TaskPatch {
                status: Some(TaskStatus::Done),
                assignee: Some("agent-desktop".into()),
                ..TaskPatch::default()
            },
        );
        mk(
            p,
            "01J0P1",
            "Repo task",
            TaskPatch {
                status: Some(TaskStatus::Doing),
                tags: Some(vec!["Alpha".into(), "beta".into()]),
                ..TaskPatch::default()
            },
        );
        let tasks = scan_tasks(&[w, p]).unwrap();
        (tasks, wsroot)
    }

    #[test]
    fn scan_aggregates_both_homes_and_defaults_project() {
        let dir = tempdir().unwrap();
        let (tasks, _) = seed_board(&dir);
        assert_eq!(tasks.len(), 3);
        let repo = find_by_id(&tasks, "01J0P1").unwrap();
        assert_eq!(repo.project, "ShatteredRealms", "defaulted from its home");
        assert_eq!(repo.home, HomeKind::Project);
        let wtask = find_by_id(&tasks, "01J0W1").unwrap();
        assert_eq!(wtask.project, "ItemSearch", "explicit key wins");
        assert_eq!(wtask.home, HomeKind::Workspace);
    }

    #[test]
    fn missing_home_folder_is_not_an_error() {
        let dir = tempdir().unwrap();
        assert!(list_tasks(ws(&dir)).unwrap().is_empty());
    }

    #[test]
    fn archive_and_goals_subfolders_are_not_scanned_as_tasks() {
        let dir = tempdir().unwrap();
        let home = workspace_tasks_dir(dir.path());
        write(&home.join("01J0A-live.md"), "---\nid: live\nstatus: todo\n---\n\nx\n");
        write(&home.join("archive/2026-07/01J0B-old.md"), "---\nid: old\n---\n\nx\n");
        write(&home.join("goals/01J0G-goal.md"), "---\nid: g\n---\n\nx\n");
        let tasks = list_tasks(ws(&dir)).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "live");
    }

    // ---- 1.4 archive / log / journal ----

    #[test]
    fn archive_month_cases() {
        assert_eq!(archive_month("2026-07-31").unwrap(), "2026-07");
        assert_eq!(archive_month("2026-01-01").unwrap(), "2026-01");
        assert!(archive_month("2026-7-1").is_err());
        assert!(archive_month("").is_err());
        assert!(archive_month("not-a-date").is_err());
    }

    #[test]
    fn archive_stays_in_the_tasks_own_home() {
        let dir = tempdir().unwrap();
        let (tasks, _) = seed_board(&dir);
        let repo = find_by_id(&tasks, "01J0P1").unwrap();
        let moved = archive_task(repo, "2026-07-15").unwrap();
        assert!(
            moved.ends_with("ShatteredRealms/.ken/tasks/archive/2026-07/01J0P1-repo-task.md"),
            "{}",
            moved.display()
        );
        assert!(!repo.path.exists());

        let wtask = find_by_id(&tasks, "01J0W1").unwrap();
        let moved = archive_task(wtask, "2026-08-02").unwrap();
        assert!(
            moved.ends_with(".ken-workspace/tasks/archive/2026-08/01J0W1-workspace-todo.md"),
            "{}",
            moved.display()
        );
    }

    #[test]
    fn archiving_the_same_name_twice_does_not_clobber() {
        let dir = tempdir().unwrap();
        let (tasks, _) = seed_board(&dir);
        let t = find_by_id(&tasks, "01J0W2").unwrap();
        let first = archive_task(t, "2026-08-02").unwrap();
        write(&t.path, &read(&first));
        let second = archive_task(t, "2026-08-02").unwrap();
        assert_ne!(first, second);
        assert!(second.to_string_lossy().ends_with("-2.md"), "{}", second.display());
        assert!(first.exists() && second.exists());
    }

    #[test]
    fn journal_line_cites_the_task_address() {
        let dir = tempdir().unwrap();
        let (tasks, _) = seed_board(&dir);
        let t = find_by_id(&tasks, "01J0W1").unwrap();
        let line = journal_summary_line(t, crate::memory::WORKSPACE_ADDRESS_ID, "Found the spawner in Mob.class.\nmore");
        assert_eq!(
            line,
            "Completed task \"Workspace todo\" — Found the spawner in Mob.class. (ken://workspace/tasks/01J0W1-workspace-todo.md)"
        );
    }

    #[test]
    fn journal_line_addresses_a_per_repo_task_under_its_project() {
        let dir = tempdir().unwrap();
        let (tasks, _) = seed_board(&dir);
        let t = find_by_id(&tasks, "01J0P1").unwrap();
        let line = journal_summary_line(t, "proj-uuid", "");
        assert_eq!(
            line,
            "Completed task \"Repo task\" (ken://proj-uuid/.ken/tasks/01J0P1-repo-task.md)"
        );
    }

    #[test]
    fn log_entry_is_pure_and_only_adds_the_heading_once() {
        let first = compose_log_entry("Body.", "2026-08-03", "10:00", "hi");
        assert!(first.starts_with("## Log\n\n"));
        let second = compose_log_entry("Body.\n\n## Log\n\n### x\n", "2026-08-03", "10:00", "hi");
        assert!(!second.contains("## Log"));
    }

    // ---- ken-families follow-up: TaskHome::Family (folds src-tauri's old
    // `list_family_board_tasks` duplication back into the shared scanner) ----

    #[test]
    fn family_home_lists_a_board_and_defaults_project() {
        let dir = tempdir().unwrap();
        let board_dir = dir.path().join("families/FAM1/members/mem-1/board");
        let home = TaskHome::Family {
            board_dir: &board_dir,
            family_name: "The Smiths",
        };
        create_task(
            home,
            &NewTask {
                id: Some("01J0FAM1".into()),
                title: "Pick up groceries".into(),
                ..NewTask::default()
            },
            "2026-08-03",
        )
        .unwrap();
        let listed = list_tasks(home).unwrap();
        assert_eq!(listed.len(), 1);
        let t = &listed[0];
        assert_eq!(t.home, HomeKind::Family);
        assert_eq!(t.project, "The Smiths", "defaulted from the family's display name");
        assert!(
            t.path.ends_with("families/FAM1/members/mem-1/board/01J0FAM1-pick-up-groceries.md"),
            "{}",
            t.path.display()
        );
        assert_eq!(
            t.address_rel_path(),
            "members/mem-1/board/01J0FAM1-pick-up-groceries.md",
            "member id is recovered from home_dir, matching family::board_rel's shape"
        );
    }

    #[test]
    fn family_archive_stays_inside_the_family_clone() {
        let dir = tempdir().unwrap();
        let board_dir = dir.path().join("families/FAM1/members/mem-1/board");
        let home = TaskHome::Family {
            board_dir: &board_dir,
            family_name: "The Smiths",
        };
        let t = create_task(
            home,
            &NewTask {
                id: Some("01J0FAM2".into()),
                title: "Book the vet".into(),
                ..NewTask::default()
            },
            "2026-08-03",
        )
        .unwrap();
        let moved = archive_task(&t, "2026-08-15").unwrap();
        assert!(
            moved.ends_with("families/FAM1/members/mem-1/board/archive/2026-08/01J0FAM2-book-the-vet.md"),
            "{}",
            moved.display()
        );
        assert!(!t.path.exists());
    }

    #[test]
    fn family_home_dedupes_by_id_against_another_home() {
        let dir = tempdir().unwrap();
        let wsroot = dir.path().join("ws");
        let board_dir = dir.path().join("families/FAM1/members/mem-1/board");
        let w = TaskHome::Workspace {
            workspace_root: &wsroot,
        };
        let f = TaskHome::Family {
            board_dir: &board_dir,
            family_name: "The Smiths",
        };
        let mk = |home: TaskHome, title: &str| {
            create_task(
                home,
                &NewTask {
                    id: Some("01J0DUP".into()),
                    title: title.into(),
                    ..NewTask::default()
                },
                "2026-08-03",
            )
            .unwrap()
        };
        mk(w, "Workspace version");
        mk(f, "Family version");

        // Same `id` in two homes collapses to the first-home-wins task
        // (scan_tasks: "duplicate ids ... collapse to the first occurrence"),
        // exactly like a workspace/project collision already does.
        let scanned = scan_tasks(&[w, f]).unwrap();
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].home, HomeKind::Workspace);
        assert_eq!(scanned[0].title, "Workspace version");

        let scanned2 = scan_tasks(&[f, w]).unwrap();
        assert_eq!(scanned2.len(), 1);
        assert_eq!(scanned2[0].home, HomeKind::Family);
        assert_eq!(scanned2[0].title, "Family version");
    }
}
