//! The first wiki, drafted from an analysis at set-up (knowledge-layer item
//! 4b). Offered, never forced: Claude reads the repos (briefs by section,
//! READMEs, build files, top docs, layout, imports, changelogs by version,
//! who has committed recently), and any folder of documents a person adds (a
//! Confluence export, say), and drafts the pages the templates README says
//! Ken drafts (Current, Conventions, Platform, Reference, the release notes),
//! a Repo Map page for every repo and an architecture page for every code
//! repo, for a person to review.
//!
//! Four rules keep it honest:
//! - A page a person wrote is never touched. A page is drafted only when it
//!   is missing or is still an untouched template (it has `{{…}}` left).
//! - Every drafted page is `status: draft` with no `verified:` date, so the
//!   age rule keeps raising it until someone reads it against its sources.
//! - Every page names the sources it came from, in its frontmatter and
//!   inline, as `repo:path` locators.
//! - Every repo path a drafted page names is checked against what git
//!   tracks, and a page naming one the checkout lacks goes back once.
//!
//! The model call is passed in (`generate`), as for ingest.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::{Error, Result};

pub const REVIEW_KIND: &str = "wiki-draft";
/// Characters of source text given to the model per page, all sources
/// together, so a large folder cannot overflow the prompt.
const SOURCE_BUDGET: usize = 120_000;
const PER_FILE: usize = 8_000;
/// Characters of one section of a brief. A brief is read by section, each up
/// to this, so a rule deep in a long brief still reaches the page: the
/// 2026-10-06 dry run read the first 8,000 characters of the mod's 152 KB
/// `CLAUDE.md`, and the architecture page said no source set out which layer
/// may call which while its lines 834 and 1,135 did.
const SECTION_MAX: usize = 12_000;
/// Characters of one version's release notes.
const VERSION_MAX: usize = 4_000;
/// Characters of one build file's summary.
const BUILD_MAX: usize = 6_000;
/// Characters a team page reads from the repos themselves (brief sections,
/// release notes, people files), all repos together, beside the repo pages.
const PAGE_BUDGET: usize = 80_000;

/// One thing read for the draft, labelled the way a page cites it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// `repo:path`, or `repo:(layout)` / `repo:(git authors)` for derived facts.
    pub label: String,
    pub text: String,
}

/// The pages drafted, in order: (path in the wiki, what the page is for).
/// The library pages the templates README says Ken drafts; a repo's own
/// pages are [`REPO_MAP`] and [`KIND_PAGES`].
pub const PAGES: &[(&str, &str)] = &[
    (
        "Current/Project.md",
        "what the product is and what it does for the people who use it, for someone who never reads code, from the READMEs and briefs; \
         who it is for, the goal and what is out of scope only where a source says so",
    ),
    (
        "Current/Team.md",
        "how the team is structured: roles, who decides what, how work is handed off; \
         roles and deciders only as CODEOWNERS, `people/` files or the repos' own docs name them",
    ),
    (
        "Current/Who-Does-What.md",
        "who owns each area and repo, and who to ask, from CODEOWNERS, `people/` files and what the repos' own docs say; \
         git adds only a column of who has committed to each area recently (the recent contributors by folder), never an owner",
    ),
    (
        "Current/Feature-Status.md",
        "each feature, from the changelog and the code that registers it, with the tests that name it and the commit read; \
         whether it was used in the running product is a person's to say",
    ),
    (
        "Conventions/Architecture.md",
        "how the code repos fit together: each code repo, what it builds, which repos import or call which and how, with a mermaid diagram; \
         each repo's own layers are on its page, `Conventions/Architecture-<repo>.md`, so link it rather than repeat it",
    ),
    (
        "Conventions/Code.md",
        "the code conventions the tools enforce, from the lint and formatter config, and the helpers that exist and where they live",
    ),
    (
        "Conventions/Data-and-Config.md",
        "the data and config folders, their formats and the code that loads each",
    ),
    (
        "Conventions/Testing.md",
        "the test suites and how each is run, from `.ken/gates.json` and the test config, and the fixture folders",
    ),
    (
        "Conventions/Registries.md",
        "the registries in the code (installers, routes, command lists): where each is and what registers in it",
    ),
    (
        "Platform/Index.md",
        "one row per reference repo and pinned dependency: what it is, the version pinned and where the pin is",
    ),
    (
        "Work/Releases.md",
        "a business doc (item 2b): what changed in each release, newest first, in words for someone who never reads code, from changelogs and release tags; \
         first an Unreleased section of what landed since the newest tag, grouped by what it does for the user, from the changes since it",
    ),
    (
        "Reference/Systems.md",
        "one row per system, from the code that registers it: its code and data paths, and the rulings (D-nnn) its code cites",
    ),
    (
        "Reference/Config-Map.md",
        "every config file and key, its default, and the code that reads it",
    ),
    (
        "Reference/Build-and-Run.md",
        "per repo: the tools and versions the build files pin, the scripts and tasks the build files define, \
         the `.ken/gates.json` commands, and the environment variables the code reads",
    ),
    (
        "Reference/Vocabulary.md",
        "the traps where a search in one word misses a page in another, and nothing else: a word the team, the users or the briefs use \
         that differs from the code's or the platform's word; a word with two meanings; a rename, from commit messages and the briefs. \
         A word that means what it says gets no row; glossaries first",
    ),
];

fn clip(s: &str, n: usize) -> String {
    if s.len() <= n {
        return s.to_string();
    }
    let mut end = n;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[… clipped]", &s[..end])
}

fn read_text(path: &Path) -> Option<String> {
    let t = crate::extract::extract(path).ok()?.text;
    (!t.trim().is_empty()).then_some(t)
}

/// A file's text as it is on disk, line numbers intact, for files read by
/// section (a brief, a changelog) or by line (a build file).
fn read_plain(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let t = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    (!t.trim().is_empty()).then_some(t)
}

/// Run git in `root` and read what it prints as UTF-8, whatever the
/// console's code page: on Windows a contributor's accented name came out
/// garbled in the 2026-10-06 dry run. None when git fails.
fn git_text(root: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("git");
    let out = crate::proc::quiet(&mut cmd)
        .args(["-c", "i18n.logOutputEncoding=UTF-8", "-c", "core.quotePath=false"])
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Every file git tracks in `root`, as `/`-separated paths.
fn ls_files(root: &Path) -> Option<Vec<String>> {
    let text = git_text(root, &["ls-files", "-z"])?;
    Some(text.split('\0').filter(|f| !f.is_empty()).map(str::to_string).collect())
}

/// The files at the top of `root` named in `wanted`, matched without regard
/// to case and each read once, under its real name, in `wanted`'s order. On
/// Windows `README.md` and `readme.md` are one file, and the 2026-10-06 dry
/// run read it under both names and told the page the repo had two.
fn top_files(root: &Path, wanted: &[&str]) -> Vec<(String, PathBuf)> {
    let names: Vec<String> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut out = Vec::new();
    for w in wanted {
        let mut matching: Vec<&String> = names.iter().filter(|n| n.eq_ignore_ascii_case(w)).collect();
        matching.sort();
        for n in matching {
            let path = root.join(n);
            let canon = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if !seen.contains(&canon) {
                seen.push(canon);
                out.push((n.clone(), path));
            }
        }
    }
    out
}

/// Dot-folders listed in the layout although other dot-folders are not: they
/// say how the repo is worked on (Ken's gates, CI, the team manifest).
const DOT_FOLDERS: &[&str] = &[".ken", ".github", ".wright", ".gitlab", ".circleci", ".devcontainer"];
/// Entries listed under one top-level folder.
const LEVEL_TWO_MAX: usize = 200;
/// Characters of the whole folder listing.
const LAYOUT_BUDGET: usize = 16_000;

/// The repo's shape: its top two levels, folders first, skipping dotfiles
/// (the dot-folders of [`DOT_FOLDERS`] aside) and dependency folders; then the
/// folders git tracks files in, one depth at a time, while
/// [`LAYOUT_BUDGET`] lasts. Two levels at 15 entries a folder hid
/// `packages/weather-compositor`, `.ken/` and `HytaleServer/builtin`, and never
/// reached the mod's 37 `data/` folders (2026-10-06).
fn layout(root: &Path, tracked: &[String]) -> String {
    let shown = |name: &str| !name.starts_with('.') || DOT_FOLDERS.contains(&name);
    let mut out = String::new();
    let list = |dir: &Path| -> Vec<(String, bool)> {
        let mut v: Vec<(String, bool)> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let is_dir = e.path().is_dir();
                let keep = if is_dir { shown(&name) && !crate::scan::is_junk_dir_name(&name) } else { !name.starts_with('.') };
                keep.then_some((name, is_dir))
            })
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    };
    for (name, is_dir) in list(root) {
        out.push_str(&format!("{name}{}\n", if is_dir { "/" } else { "" }));
        if is_dir {
            let children = list(&root.join(&name));
            let more = children.len().saturating_sub(LEVEL_TWO_MAX);
            for (child, cdir) in children.into_iter().take(LEVEL_TWO_MAX) {
                out.push_str(&format!("  {child}{}\n", if cdir { "/" } else { "" }));
            }
            if more > 0 {
                out.push_str(&format!("  (… {more} more)\n"));
            }
        }
    }
    if out.len() >= LAYOUT_BUDGET {
        return clip(&out, LAYOUT_BUDGET);
    }
    // Deeper, from the files git tracks: folders only, a whole depth at a time.
    let mut by_depth: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    for f in tracked {
        let segs: Vec<&str> = f.split('/').collect();
        if !shown(segs[0]) {
            continue;
        }
        for d in 3..segs.len() {
            by_depth.entry(d).or_default().insert(segs[..d].join("/"));
        }
    }
    let mut deeper = String::new();
    for (depth, folders) in by_depth {
        let block: String = folders.iter().map(|f| format!("{f}/\n")).collect();
        if out.len() + deeper.len() + block.len() > LAYOUT_BUDGET {
            deeper.push_str(&format!("({} folders {depth} levels down, and any deeper, not listed)\n", folders.len()));
            break;
        }
        deeper.push_str(&block);
    }
    if !deeper.is_empty() {
        out.push_str("\nfolders further down, from the files git tracks:\n");
        out.push_str(&deeper);
    }
    out
}

/// Whether a tag names a version: `v1.2`, `1.2.3`, `v0.0.5.1`. An
/// `archive/…` tag is not one.
fn is_version_tag(tag: &str) -> bool {
    tag.strip_prefix(['v', 'V']).unwrap_or(tag).starts_with(|c: char| c.is_ascii_digit())
}

/// Every version tag, newest first, as `tag date subject`: all of them (a
/// 40-line cap showed 5 of the mod's 13 version tags on 2026-10-06), and only
/// version tags.
fn git_tags(root: &Path) -> Option<String> {
    let text = git_text(
        root,
        &["for-each-ref", "--sort=-creatordate", "--format=%(refname:short) %(creatordate:short) %(subject)", "refs/tags"],
    )?;
    let lines: Vec<&str> = text.lines().filter(|l| is_version_tag(l.split(' ').next().unwrap_or(""))).collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// What landed after the newest version tag the checkout contains (`v1.2`,
/// else `1.2`), as `date subject` lines, newest first; the recent commits
/// when the repo has no version tag. The label says which. Only a version
/// tag counts: a plain `describe --tags` picked
/// `archive/ui-faction-profession-combat-2026-09-30` on 2026-10-06, and the
/// Unreleased notes started five days late.
fn git_unreleased(root: &Path) -> Option<(String, String)> {
    let describe = |pattern: &str| {
        git_text(root, &["describe", "--tags", "--abbrev=0", "--match", pattern])
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
    };
    let tag = describe("v[0-9]*").or_else(|| describe("[0-9]*"));
    let range = tag.as_ref().map_or_else(|| "HEAD".to_string(), |t| format!("{t}..HEAD"));
    let log = git_text(root, &["log", "--no-merges", "--date=short", "--format=%ad %s", "-n", "200", &range])
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())?;
    let label = match &tag {
        Some(t) => format!("(changes since {t})"),
        None => "(recent changes, no tags)".to_string(),
    };
    Some((label, log))
}

/// Bots and AI agents, by name or email: never a person to ask. Dependabot,
/// GitHub Actions, any `[bot]`, Copilot, Claude, Cursor Agent, a name ending
/// in "Bot" (Hytale Sync Bot), and the noreply addresses bots commit from.
pub fn is_bot(name: &str, email: &str) -> bool {
    let n = name.trim().to_lowercase();
    let e = email.trim().to_lowercase();
    let local = e.split('@').next().unwrap_or("");
    ["copilot", "claude", "cursor agent", "cursoragent", "github-actions", "dependabot"].contains(&n.as_str())
        || n.starts_with("dependabot")
        || n.starts_with("github-actions")
        || n.ends_with("[bot]")
        || name.trim().ends_with("Bot")
        || n.ends_with(" bot")
        || local.ends_with("[bot]")
        || ["noreply@anthropic.com", "cursoragent@cursor.com", "copilot@github.com"].contains(&e.as_str())
        || (e.contains("noreply") && local.contains("bot"))
}

/// Who has committed to each top-level folder in the last 90 days, as
/// `folder/: Name (commits), …`, at most three a folder, most first. A
/// commit identity the team's `roster` lists counts as that person, under
/// their roster name, and is never dropped as a bot; any other is one person
/// per email ([`crate::people::person_key`]), shown under the name they
/// commit with most, bots and AI agents left out. The Shattered Realms draft
/// listed one person's three identities as three people. This is a column of
/// recent hands, never an owner: the 2026-10-06 draft read a whole-history
/// shortlog and put the template's committers from before the team existed
/// into the team's roles table.
pub fn recent_contributors(root: &Path, roster: &[crate::people::Person]) -> Option<String> {
    let text = git_text(root, &["log", "--since=90.days", "--no-merges", "--format=%x1e%aN%x1f%aE", "--name-only"])?;
    let mut counts: BTreeMap<String, HashMap<String, usize>> = BTreeMap::new();
    let mut names: HashMap<String, HashMap<String, usize>> = HashMap::new();
    for rec in text.split('\x1e').filter(|r| !r.trim().is_empty()) {
        let mut lines = rec.lines();
        let Some((name, email)) = lines.next().and_then(|l| l.split_once('\x1f')) else { continue };
        let (key, shown) = match crate::people::by_commit(roster, name, email) {
            Some(p) => (format!("roster:{}", p.id), p.name.clone()),
            None if is_bot(name, email) => continue,
            None => (crate::people::person_key(email), name.trim().to_string()),
        };
        *names.entry(key.clone()).or_default().entry(shown).or_default() += 1;
        let areas: BTreeSet<String> = lines
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|f| f.split_once('/').map_or_else(|| "(top-level files)".to_string(), |(a, _)| format!("{a}/")))
            .collect();
        for a in areas {
            *counts.entry(a).or_default().entry(key.clone()).or_default() += 1;
        }
    }
    let name_of = |k: &str| {
        names
            .get(k)
            .and_then(|m| m.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0))))
            .map(|(n, _)| n.clone())
            .unwrap_or_default()
    };
    let mut out = String::new();
    for (area, people) in counts {
        let mut ranked: Vec<(String, usize)> = people.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut shown: Vec<(String, usize)> = Vec::new();
        for (k, n) in ranked {
            let name = name_of(&k);
            if !shown.iter().any(|(s, _)| *s == name) {
                shown.push((name, n));
            }
            if shown.len() == 3 {
                break;
            }
        }
        let row: Vec<String> = shown.iter().map(|(s, n)| format!("{s} ({n})")).collect();
        out.push_str(&format!("{area}: {}\n", row.join(", ")));
    }
    (!out.is_empty()).then_some(out)
}

/// Where a code file's source tree starts: the folder up to
/// `src/<set>/<java|kotlin|scala|groovy>/`, else up to `packages/<name>/` or
/// `apps/<name>/`, else up to the first `src/` or `lib/`; the repo's top when
/// it has none of them.
fn source_marker(path: &str) -> Vec<&str> {
    let segs: Vec<&str> = path.split('/').collect();
    let dirs = &segs[..segs.len().saturating_sub(1)];
    for i in 0..dirs.len() {
        if dirs[i] == "src" && i + 2 < dirs.len() && ["java", "kotlin", "scala", "groovy"].contains(&dirs[i + 2]) {
            return dirs[..=i + 2].to_vec();
        }
    }
    if let Some(i) = dirs.iter().position(|d| *d == "packages" || *d == "apps").filter(|i| i + 1 < dirs.len()) {
        return dirs[..=i + 1].to_vec();
    }
    if let Some(i) = dirs.iter().position(|d| *d == "src" || *d == "lib") {
        return dirs[..=i].to_vec();
    }
    Vec::new()
}

/// Each code file's folder for the import map, `depth` folders below its
/// source root, where the source root is the deepest folder that every code
/// file under the same [`source_marker`] shares (`src/main/java/dev/hytalemodding`).
/// A fixed first four segments named every Java file `src/main/java/dev`, so
/// the mod's 1,349 plugin files were one folder and the services <-> systems
/// cycle (13 files one way, 139 the other) never showed (2026-10-06).
fn folders_below_source_root(code: &[&String], depth: usize) -> HashMap<String, String> {
    let dirs_of = |f: &str| -> Vec<String> {
        let segs: Vec<&str> = f.split('/').collect();
        segs[..segs.len() - 1].iter().map(|s| s.to_string()).collect()
    };
    let mut roots: HashMap<Vec<&str>, Vec<String>> = HashMap::new();
    for f in code {
        let marker = source_marker(f);
        let dirs = dirs_of(f);
        match roots.get_mut(&marker) {
            None => {
                roots.insert(marker, dirs);
            }
            Some(common) => {
                let n = common.iter().zip(&dirs).take_while(|(a, b)| a == b).count();
                common.truncate(n);
            }
        }
    }
    code.iter()
        .map(|f| {
            let root = &roots[&source_marker(f)];
            let dirs = dirs_of(f);
            let name: Vec<&str> = dirs.iter().take(root.len() + depth).map(String::as_str).collect();
            let name = if name.is_empty() { ".".to_string() } else { name.join("/") };
            (f.to_string(), name)
        })
        .collect()
}

/// Which folders' code imports which, from the code itself: one line per
/// pair, `from -> to (imports)`, most first, each folder named two levels
/// below its source root. Then every pair that imports both ways, one level
/// and two levels below the root, so a cycle between two layers shows however
/// its files are spread. What a layer may call, as the code has it, for a
/// repo whose docs never say. None for a repo with no mapped code.
pub fn folder_imports(root: &Path) -> Option<String> {
    let files = ls_files(root)?;
    let code: Vec<&String> = files.iter().filter(|f| crate::codemap::Lang::of(f).is_some()).collect();
    let two = folders_below_source_root(&code, 2);
    let one = folders_below_source_root(&code, 1);
    let mut edges: HashMap<(String, String), usize> = HashMap::new();
    let mut edges_one: HashMap<(String, String), usize> = HashMap::new();
    // How many other files import each file: the hubs to start reading at.
    let mut importers: HashMap<String, HashSet<String>> = HashMap::new();
    for rel in &code {
        let Some(lang) = crate::codemap::Lang::of(rel) else { continue };
        let Ok(text) = fs::read_to_string(root.join(rel)) else { continue };
        if text.len() > 1_000_000 {
            continue;
        }
        for target in crate::codemap::imports_of(lang, &text) {
            let Some(to) = crate::codemap::resolve_import(rel, &target, &files) else { continue };
            if &to != *rel {
                importers.entry(to.clone()).or_default().insert(rel.to_string());
            }
            let (Some(a), Some(b)) = (two.get(*rel), two.get(&to)) else { continue };
            if a != b {
                *edges.entry((a.clone(), b.clone())).or_default() += 1;
            }
            let (a, b) = (&one[*rel], &one[&to]);
            if a != b {
                *edges_one.entry((a.clone(), b.clone())).or_default() += 1;
            }
        }
    }
    if edges.is_empty() {
        return None;
    }
    // Pairs that import each other: not a one-way layering, whatever the
    // docs say.
    // A pair named the same at both depths (a folder with no subfolders) is
    // listed once, with the one-level counts, which take in its subfolders.
    let both_ways = |edges: &HashMap<(String, String), usize>| -> Vec<(String, String, usize)> {
        edges
            .iter()
            .filter(|((a, b), _)| a < b)
            .filter_map(|((a, b), n)| edges.get(&(b.clone(), a.clone())).map(|m| (format!("{a} <-> {b}"), format!("({n} / {m})"), n + m)))
            .collect()
    };
    let mut both: Vec<(String, String, usize)> = both_ways(&edges_one);
    for (pair, counts, n) in both_ways(&edges) {
        if !both.iter().any(|(p, _, _)| *p == pair) {
            both.push((pair, counts, n));
        }
    }
    let mut both: Vec<(String, usize)> = both.into_iter().map(|(p, c, n)| (format!("{p} {c}"), n)).collect();
    // The most-used pairs when there are many, listed in name order.
    both.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(&y.0)));
    let more = both.len().saturating_sub(80);
    let mut both: Vec<String> = both.into_iter().take(80).map(|(p, _)| p).collect();
    both.sort();
    if more > 0 {
        both.push(format!("(… {more} more pairs)"));
    }
    let mut rows: Vec<_> = edges.into_iter().collect();
    rows.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(&y.0)));
    let mut out = rows.into_iter().take(80).map(|((a, b), n)| format!("{a} -> {b} ({n})")).collect::<Vec<_>>().join("\n");
    if !both.is_empty() {
        out.push_str("\n\nimport each other (a -> b / b -> a):\n");
        out.push_str(&both.join("\n"));
    }
    let mut hubs: Vec<(String, usize)> = importers.into_iter().map(|(f, by)| (f, by.len())).filter(|(_, n)| *n > 1).collect();
    hubs.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(&y.0)));
    if !hubs.is_empty() {
        out.push_str("\n\nmost imported files (how many files import each):\n");
        out.push_str(&hubs.iter().take(15).map(|(f, n)| format!("{f} ({n})")).collect::<Vec<_>>().join("\n"));
    }
    Some(out)
}

/// Characters of source text read from one repo for its Repo Map page, so
/// every repo gets its own budget however many the team has. Raised from
/// 60,000 when briefs began to be read by section and build files, gates and
/// deeper folders joined the read (2026-10-06).
const REPO_BUDGET: usize = 90_000;

fn pusher(budget: usize) -> impl Fn(String, String, &mut Vec<Source>) {
    pusher_of(budget, PER_FILE)
}

/// A push into a source list that stops at `budget` characters in all, each
/// source clipped to `each`.
fn pusher_of(budget: usize, each: usize) -> impl Fn(String, String, &mut Vec<Source>) {
    move |label: String, text: String, out: &mut Vec<Source>| {
        let used: usize = out.iter().map(|s| s.text.len()).sum();
        if used < budget {
            out.push(Source { label, text: clip(&text, each.min(budget - used)) });
        }
    }
}

/// One part of a Markdown file: the line its heading is on (1-based), the
/// heading's words, and its text, heading included.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub line: usize,
    pub heading: String,
    pub text: String,
}

/// Split Markdown at the lines `is_head` picks, never inside a code fence.
/// What comes before the first is a section with no heading, at line 1,
/// when it has any words.
fn split_at(text: &str, is_head: impl Fn(&str) -> bool) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut cur = Section { line: 1, heading: String::new(), text: String::new() };
    let mut fence = false;
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
        }
        if !fence && is_head(line) {
            if !cur.text.trim().is_empty() {
                out.push(cur);
            }
            cur = Section { line: i + 1, heading: line.trim_start_matches('#').trim().to_string(), text: String::new() };
        }
        cur.text.push_str(line);
        cur.text.push('\n');
    }
    if !cur.text.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// A brief split at its `##` headings; a `###` stays inside its section.
pub fn brief_sections(text: &str) -> Vec<Section> {
    split_at(text, |l| l.starts_with("## "))
}

/// Whether a line is a version's heading in a changelog: `#` to `###`, then
/// words that, past any `[` or `v`, start with a digit or say Unreleased.
fn is_version_heading(line: &str) -> bool {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if !(1..=3).contains(&hashes) || !line[hashes..].starts_with(' ') {
        return false;
    }
    let w = line[hashes..].trim().trim_start_matches('[').trim_start_matches(['v', 'V']);
    w.starts_with(|c: char| c.is_ascii_digit()) || w.to_ascii_lowercase().starts_with("unreleased")
}

/// A changelog split into one section per version, in the file's order;
/// what comes before the first version is left out. The Releases page gets
/// each version's own notes: on 2026-10-06 it got the first 3,000 characters
/// of the mod's 339 KB changelog, and 0.0.11 and 0.0.12 came out empty.
pub fn version_sections(text: &str) -> Vec<Section> {
    split_at(text, is_version_heading).into_iter().filter(|s| is_version_heading(s.text.lines().next().unwrap_or(""))).collect()
}

/// What a brief's section is about, which says which page it serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    /// How to build, run, test and deploy: the repo's page.
    Build,
    /// Layers, structure, patterns, systems: the architecture page.
    Architecture,
    /// Glossaries, naming, gotchas: Vocabulary.
    Vocabulary,
    /// Team, owners, roles, reviews: Team and Who Does What.
    People,
    /// Releases and versions: Releases.
    Releases,
    /// What the project is, its goal: Project.
    Project,
    Other,
}

/// The words that route a section, checked in this order. A heading picks
/// the first topic one of whose words begins a word of the heading (a word
/// ending in a space must be the whole word).
const TOPIC_WORDS: &[(Topic, &[&str])] = &[
    (
        Topic::Build,
        &[
            "build", "run", "install", "setup", "set up", "getting started", "quick start", "quickstart", "develop", "deploy",
            "test", "command", "script", "usage", "prerequisite", "requirement", "toolchain", "environment", "debug", "ci ",
        ],
    ),
    (
        Topic::Architecture,
        &[
            "architect", "layer", "structure", "pattern", "wiring", "module", "package", "thread", "dependenc", "design",
            "system", "pipeline", "flow", "entry point", "config", "convention",
        ],
    ),
    (
        Topic::Vocabulary,
        &["vocabulary", "glossary", "term", "naming", "names", "gotcha", "pitfall", "trap", "jargon", "words", "called", "spelling"],
    ),
    (
        Topic::People,
        &["team", "people", "owner", "maintainer", "contributor", "contact", "role", "who ", "review", "governance", "decision"],
    ),
    (Topic::Releases, &["release", "changelog", "version", "patch note", "history"]),
    (
        Topic::Project,
        &["what this is", "what it is", "about", "overview", "introduction", "goal", "roadmap", "feature", "purpose", "vision", "status"],
    ),
];

/// The topic of a brief's section, from its heading by [`TOPIC_WORDS`]. The
/// text before a brief's first heading says what the repo is: Project.
pub fn topic_of(heading: &str) -> Topic {
    if heading.trim().is_empty() {
        return Topic::Project;
    }
    let lower = heading.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let spaced = format!(" {} ", words.join(" "));
    TOPIC_WORDS
        .iter()
        .find(|(_, keys)| keys.iter().any(|k| spaced.contains(&format!(" {k}"))))
        .map_or(Topic::Other, |(t, _)| *t)
}

/// The briefs read by section, in this order, matched without regard to case.
const BRIEFS: &[&str] = &["CLAUDE.md", "AGENTS.md", "README.md", "README", "CONTRIBUTING.md"];

/// Each brief of `root`, split into its sections.
fn briefs(root: &Path) -> Vec<(String, Vec<Section>)> {
    top_files(root, BRIEFS).into_iter().filter_map(|(n, p)| read_plain(&p).map(|t| (n, brief_sections(&t)))).collect()
}

/// Every heading of the briefs with the line it is on: what each brief
/// covers, sent whole, so a page knows where a section it was not given is.
fn brief_outline(briefs: &[(String, Vec<Section>)]) -> String {
    let mut out = String::new();
    for (file, sections) in briefs {
        out.push_str(&format!("{file}:\n"));
        for s in sections.iter().filter(|s| !s.heading.is_empty()) {
            out.push_str(&format!("  {}: {}\n", s.line, s.heading));
        }
    }
    out
}

/// How a part of a file is cited: `repo:FILE:line`, or `repo:FILE` for the
/// part at the top of the file.
fn part_label(name: &str, file: &str, line: usize) -> String {
    if line <= 1 {
        format!("{name}:{file}")
    } else {
        format!("{name}:{file}:{line}")
    }
}

/// A repo's brief sections whose topic `pick` takes, in file order, each
/// labelled by [`part_label`], while `budget` lasts.
fn sections_for(name: &str, briefs: &[(String, Vec<Section>)], pick: impl Fn(Topic) -> bool, budget: usize) -> Vec<Source> {
    let mut out = Vec::new();
    let mut used = 0;
    let picked = briefs.iter().flat_map(|(f, secs)| secs.iter().map(move |s| (f, s))).filter(|(_, s)| pick(topic_of(&s.heading)));
    for (file, s) in picked {
        if budget.saturating_sub(used) < 200 {
            break;
        }
        let text = clip(&s.text, SECTION_MAX.min(budget - used));
        used += text.len();
        out.push(Source { label: part_label(name, file, s.line), text });
    }
    out
}

/// The tables of a TOML file named in `tables` (`[package]`, `[[bin]]`),
/// each with its header line.
fn toml_tables(text: &str, tables: &[&str]) -> String {
    let mut out = String::new();
    let mut on = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            on = tables.contains(&t);
        }
        if on {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// A Makefile's or justfile's targets, each with up to four lines of its
/// recipe: what a person types to build, test or run.
fn make_targets(text: &str) -> String {
    let mut out = String::new();
    let mut recipe_left = 0;
    for line in text.lines() {
        if line.starts_with('\t') || line.starts_with("    ") {
            if recipe_left > 0 {
                out.push_str(line);
                out.push('\n');
                recipe_left -= 1;
            }
            continue;
        }
        recipe_left = 0;
        let t = line.trim_end();
        let Some((head, _)) = t.split_once(':') else { continue };
        if head.trim().is_empty() || t.starts_with(['#', '.', ' ']) || head.contains(['=', '$']) || t.contains(":=") {
            continue;
        }
        out.push_str(t);
        out.push('\n');
        recipe_left = 4;
    }
    out
}

/// A Gradle build file's head (plugins, versions) and every line that
/// registers or configures a task, with its line number: the mod's 35 KB
/// `build.gradle.kts` is mostly task bodies.
fn gradle_summary(text: &str) -> String {
    let head: String = text.lines().take(60).map(|l| format!("{l}\n")).collect();
    let tasks: Vec<String> = text
        .lines()
        .enumerate()
        .skip(60)
        .filter(|(_, l)| {
            let t = l.trim_start();
            ["tasks.register", "tasks.named", "tasks.create", "by registering", "by tasks", "by creating"].iter().any(|k| t.contains(k))
                || t.starts_with("task(")
                || t.starts_with("task ")
        })
        .map(|(i, l)| format!("{}: {}", i + 1, l.trim()))
        .take(80)
        .collect();
    if tasks.is_empty() {
        head
    } else {
        format!("{head}\ntask lines further down:\n{}\n", tasks.join("\n"))
    }
}

/// package.json's name and scripts.
fn package_scripts(text: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let scripts = v.get("scripts")?.as_object()?;
    let mut out = String::new();
    if let Some(n) = v.get("name").and_then(|n| n.as_str()) {
        out.push_str(&format!("name: {n}\n"));
    }
    out.push_str("scripts:\n");
    for (k, c) in scripts {
        out.push_str(&format!("  {k}: {}\n", c.as_str().unwrap_or_default()));
    }
    Some(out)
}

/// What a repo's build files say a person can run, as (file, summary):
/// package.json scripts, the Gradle build and settings, Cargo.toml's
/// `[package]`, `[workspace]` and `[[bin]]`, pyproject's scripts, Makefile
/// and justfile targets, and `.ken/gates.json` (what each gate runs). On
/// 2026-10-06 none was read, and the Tools page said how the tools start was
/// unknown while `package.json` had `"start"`.
fn build_files(root: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let wanted = [
        "package.json",
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
        "Cargo.toml",
        "pyproject.toml",
        "Makefile",
        "GNUmakefile",
        "justfile",
        ".justfile",
    ];
    for (file, path) in top_files(root, &wanted) {
        let Some(text) = read_plain(&path) else { continue };
        let lower = file.to_lowercase();
        let summary = match lower.as_str() {
            "package.json" => package_scripts(&text),
            "build.gradle" | "build.gradle.kts" => Some(gradle_summary(&text)),
            "settings.gradle" | "settings.gradle.kts" => Some(text),
            "cargo.toml" => Some(toml_tables(&text, &["[package]", "[workspace]", "[[bin]]"])),
            "pyproject.toml" => Some(toml_tables(&text, &["[project.scripts]", "[tool.poetry.scripts]"])),
            _ => Some(make_targets(&text)),
        };
        if let Some(s) = summary.filter(|s| !s.trim().is_empty()) {
            out.push((file, s));
        }
    }
    if let Some(t) = read_plain(&root.join(".ken").join("gates.json")) {
        out.push((".ken/gates.json".to_string(), t));
    }
    out
}

/// The release-notes files read per version.
const RELEASE_NOTES: &[&str] = &["CHANGELOG.md", "PATCHNOTES.md", "RELEASES.md", "HISTORY.md", "CHANGELOG"];

/// A repo's release notes, one source per version labelled `repo:FILE:line`,
/// in each file's order (newest first, as changelogs keep them), at most
/// `per_file` versions a file, while `budget` lasts.
fn release_notes(name: &str, root: &Path, per_file: usize, budget: usize) -> Vec<Source> {
    let mut out = Vec::new();
    let push = pusher_of(budget, VERSION_MAX);
    for (file, path) in top_files(root, RELEASE_NOTES) {
        let Some(text) = read_plain(&path) else { continue };
        for s in version_sections(&text).into_iter().take(per_file) {
            push(part_label(name, &file, s.line), s.text, &mut out);
        }
    }
    out
}

/// A repo's `people/` files, its README and the PERSON.md template aside.
fn people_files(name: &str, root: &Path) -> Vec<Source> {
    crate::people::files(root)
        .into_iter()
        .filter_map(|p| {
            let text = read_plain(&p)?;
            Some(Source { label: format!("{name}:people/{}", p.file_name()?.to_string_lossy()), text: clip(&text, 3_000) })
        })
        .collect()
}

/// What one repo's page is made from, in this order, clipped to
/// [`REPO_BUDGET`]: its description; the outline of its briefs; its build
/// files; the brief sections on building, structure and what it is; its
/// layout and import map; INSTALL; its newest release notes; CODEOWNERS,
/// `people/` and who has committed recently; its version tags; up to ten
/// docs; then every other brief section, in order, while the budget lasts.
/// `roster` is the team's ([`team_roster`]), for the recent contributors.
pub fn gather_repo(name: &str, root: &Path, roster: &[crate::people::Person]) -> Vec<Source> {
    let mut out: Vec<Source> = Vec::new();
    let push = pusher(REPO_BUDGET);
    // What a person said this repo is for, at set-up: read first, so the
    // draft knows how to use each source.
    if let Some(d) = crate::registry::description_of(root) {
        push(format!("{name}:(what this repo is for, in the team's words)"), d, &mut out);
    }
    let briefs = briefs(root);
    if !briefs.is_empty() {
        push(format!("{name}:(headings of its briefs, with their lines)"), brief_outline(&briefs), &mut out);
    }
    for (file, summary) in build_files(root) {
        push(format!("{name}:{file}"), clip(&summary, BUILD_MAX), &mut out);
    }
    let used: usize = out.iter().map(|s| s.text.len()).sum();
    let wants = [Topic::Build, Topic::Architecture, Topic::Project];
    out.extend(sections_for(name, &briefs, |t| wants.contains(&t), (REPO_BUDGET * 2 / 3).saturating_sub(used)));
    // The shape of the repo before its long docs, so a documentation-heavy
    // repo cannot crowd it out of the budget; each whole up to its own budget.
    let push_map = pusher_of(REPO_BUDGET, LAYOUT_BUDGET);
    let tracked = ls_files(root).unwrap_or_default();
    push_map(format!("{name}:(layout)"), layout(root, &tracked), &mut out);
    // Not for a reference repo: the team reads it and never changes it, and
    // its import map was the slowest part of a draft (1,514 s on a 74,000-file
    // platform source, against 26 s for the team's own mod, 2026-10-05).
    let reference = crate::registry::index_of(root).0.contains(&crate::registry::RepoKind::Reference);
    if !reference {
        if let Some(d) = folder_imports(root) {
            push_map(format!("{name}:(imports between folders, from the code)"), d, &mut out);
        }
    }
    for (file, path) in top_files(root, &["INSTALL.md", "INSTALL"]) {
        if let Some(t) = read_plain(&path) {
            push(format!("{name}:{file}"), t, &mut out);
        }
    }
    let used: usize = out.iter().map(|s| s.text.len()).sum();
    out.extend(release_notes(name, root, 3, (REPO_BUDGET - used.min(REPO_BUDGET)).min(3 * VERSION_MAX)));
    for f in ["CODEOWNERS", ".github/CODEOWNERS", "docs/CODEOWNERS"] {
        if let Ok(t) = fs::read_to_string(root.join(f)) {
            push(format!("{name}:{f}"), t, &mut out);
        }
    }
    for s in people_files(name, root) {
        push(s.label, s.text, &mut out);
    }
    if let Some(a) = recent_contributors(root, roster) {
        push(format!("{name}:(recent contributors by folder, last 90 days, from git)"), a, &mut out);
    }
    if let Some(t) = git_tags(root) {
        push(format!("{name}:(version tags, newest first)"), t, &mut out);
    }
    for dir in ["docs", "doc", "documentation"] {
        let mut docs: Vec<PathBuf> = fs::read_dir(root.join(dir))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect();
        docs.sort();
        for p in docs.into_iter().take(10) {
            if let Some(t) = read_text(&p) {
                let rel = format!("{dir}/{}", p.file_name().unwrap().to_string_lossy());
                push(format!("{name}:{rel}"), t, &mut out);
            }
        }
    }
    // Everything else the briefs say, in order, while the budget lasts.
    let used: usize = out.iter().map(|s| s.text.len()).sum();
    out.extend(sections_for(name, &briefs, |t| !wants.contains(&t), REPO_BUDGET.saturating_sub(used)));
    out
}

/// Every readable document in a folder a person added (a Confluence
/// export, say), clipped to [`SOURCE_BUDGET`].
pub fn gather_extra(dir: &Path) -> Vec<Source> {
    let mut out: Vec<Source> = Vec::new();
    let push = pusher(SOURCE_BUDGET);
    let label = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "documents".into());
    let walker = ignore::WalkBuilder::new(dir).hidden(true).build().flatten();
    for e in walker.filter(|e| e.path().is_file()).take(200) {
        if let Some(t) = read_text(e.path()) {
            let rel = e.path().strip_prefix(dir).unwrap_or(e.path()).to_string_lossy().replace('\\', "/");
            push(format!("{label}:{rel}"), t, &mut out);
        }
    }
    out
}

/// Every repo's sources, each within its own budget, then `extra`.
pub fn gather(repos: &[(String, PathBuf)], extra: Option<&Path>) -> Vec<Source> {
    let roster = crate::people::roster_of(repos.iter().map(|(_, r)| r.as_path()));
    let mut out: Vec<Source> = repos.iter().flat_map(|(n, r)| gather_repo(n, r, &roster)).collect();
    if let Some(dir) = extra {
        out.extend(gather_extra(dir));
    }
    out
}

// A page Ken may write: missing, or a template nobody has filled in.
pub fn may_draft(existing: Option<&str>) -> bool {
    existing.is_none_or(|t| t.contains("{{"))
}

pub fn prompt(page: &str, purpose: &str, template: Option<&str>, sources: &[Source], today: &str) -> String {
    let mut s = format!(
        "You are drafting one page of a team's wiki, `{page}`, for a person to review. The page is: {purpose}.\n\n\
         Rules:\n\
         - Use only what the sources below say. Where they say nothing, write `(not in the sources yet)` rather than guess.\n\
         - Cite sources inline as their labels in backticks, e.g. `ken:README.md`, once at the end of each paragraph, list item or table row, for every source it used.\n\
         - Frontmatter: keep the template's keys; set `status: draft`; set `updated: {today}`; do NOT write a `verified:` line (a person verifies it later); list every source you used under `sources:` as its label.\n\
         - Plain words for a reader who has not seen the code. Tables where the template has them.\n\
         - A source labelled `(what this repo is for, in the team's words)` is the team's own description of that repo: use it to know what each repo is and how it is used.\n\
         - A source labelled `repo:FILE:line` is the part of FILE that starts at that line; cite it by that label.\n\
         - People: write an owner, a role or a decider only where CODEOWNERS, a `people/` file or a repo's own docs name one. Never write a role nobody stated. The recent contributors from git say who has committed lately, nothing more. Never speculate whether two names are one person.\n\
         - Reply with the finished page only, in Markdown, starting with `---`. No preamble, no code fences around it.\n\n"
    );
    match template {
        Some(t) => s.push_str(&format!("TEMPLATE (fill it in, replacing every {{{{…}}}}):\n{t}\n\n")),
        None => s.push_str("There is no template; use frontmatter with title, status, updated and sources, then sections that fit the page.\n\n"),
    }
    s.push_str("SOURCES:\n");
    for src in sources {
        s.push_str(&format!("\n=== {} ===\n{}\n", src.label, src.text));
    }
    s
}

/// The reply as a page: fences stripped, `status: draft` forced, any
/// `verified:` line dropped (only a person sets it).
pub fn finish(reply: &str) -> Result<String> {
    let mut t = reply.trim();
    if let Some(rest) = t.strip_prefix("```markdown").or_else(|| t.strip_prefix("```md")).or_else(|| t.strip_prefix("```")) {
        t = rest.trim_start().strip_suffix("```").unwrap_or(rest).trim();
    }
    if !t.starts_with("---") {
        return Err(Error::Other("the draft did not start with its frontmatter".into()));
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
        return Err(Error::Other("the draft's frontmatter was never closed".into()));
    }
    Ok(out)
}

// --- The paths a drafted page names, checked against the checkout. Seven
// of the eight stale claims of the 2026-10-06 dry run repeated a path a doc
// named and the code no longer had (`utils/CombatCalculator`, `data/*.json`). --

/// File extensions that make a backticked name a file a repo may track.
const FILE_EXTS: &[&str] = &[
    "md", "java", "kt", "kts", "gradle", "rs", "toml", "json", "jsonc", "ts", "tsx", "js", "mjs", "cjs", "jsx", "svelte", "vue",
    "py", "go", "cs", "csproj", "sln", "c", "h", "cpp", "hpp", "yml", "yaml", "xml", "properties", "sh", "ps1", "cmd", "bat",
    "txt", "sql", "html", "css", "scss", "lock", "cfg", "ini", "ui", "lang", "png", "svg", "csv",
];

/// Whether the last segment of `p` ends in a file extension of [`FILE_EXTS`].
fn has_file_ext(p: &str) -> bool {
    let last = p.trim_end_matches('/').rsplit('/').next().unwrap_or(p);
    last.rsplit_once('.').is_some_and(|(stem, ext)| !stem.is_empty() && FILE_EXTS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// Whether a path segment stands for many: it has `*`, `<name>` or `{name}`.
fn is_wild(seg: &str) -> bool {
    seg.contains(['*', '<', '{'])
}

/// `src/x.rs:12` and `src/x.rs:12-30` without their lines.
fn strip_line(p: &str) -> &str {
    match p.rsplit_once(':') {
        Some((head, tail)) if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit() || c == '-') => head,
        _ => p,
    }
}

/// A relative path a repo could hold: not absolute or above the repo, not a
/// command, flag or variable, not cut short with `...`, and nothing a path
/// in prose never has (spaces, quotes, `=`, brackets, backslashes).
fn plain_path(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with("..")
        && !p.contains("...")
        && !p.contains('…')
        && !p.starts_with(['/', '~', '$', '%', '-', '#', '@', '!', '+'])
        && !p.contains(|c: char| c.is_whitespace() || "\\=()\"',|;&?^[]".contains(c))
        && !p.contains("//")
}

/// One backticked token as a repo path: `(Some(repo), path)` for a
/// `repo:path` (or `repo@sha:path`) citation, its `:line` dropped;
/// `(None, path)` for a token with a `/` or a file extension. None for
/// anything else: a command, a URL, a word.
fn as_path(token: &str, repos: &[&str]) -> Option<(Option<String>, String)> {
    let t = token.trim().trim_end_matches(['.', ',', ';', ')']);
    for r in repos {
        let Some(rest) = t.strip_prefix(r) else { continue };
        let path = if let Some(p) = rest.strip_prefix(':') {
            p
        } else if let Some((_, p)) = rest.strip_prefix('@').and_then(|p| p.split_once(':')) {
            p
        } else {
            continue;
        };
        let path = strip_line(path);
        return (!path.starts_with('(') && plain_path(path)).then(|| (Some(r.to_string()), path.to_string()));
    }
    if t.contains(':') {
        return None;
    }
    let t = t.strip_prefix("./").unwrap_or(t);
    (plain_path(t) && (t.contains('/') || has_file_ext(t))).then(|| (None, t.to_string()))
}

/// The repo paths a page names, in order: each `repo:path` of its
/// `sources:`, and each backticked token outside code fences that
/// [`as_path`] reads as a path. `repos` are the repo names a citation may
/// start with.
pub fn named_paths(page: &str, repos: &[&str]) -> Vec<(Option<String>, String)> {
    let mut names: Vec<&str> = repos.to_vec();
    // `Game-Tools` before `Game`, so a longer name is never cut short.
    names.sort_by_key(|n| std::cmp::Reverse(n.len()));
    let mut out: Vec<(Option<String>, String)> = Vec::new();
    let (mut in_front, mut fence) = (false, false);
    for (i, line) in page.lines().enumerate() {
        let t = line.trim();
        if t == "---" && (i == 0 || in_front) {
            in_front = i == 0;
            continue;
        }
        if in_front {
            if let Some(item) = t.strip_prefix("- ") {
                if let Some((Some(r), p)) = as_path(item.trim_matches(['"', '\'']), &names) {
                    out.push((Some(r), p));
                }
            }
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
            continue;
        }
        if fence {
            continue;
        }
        for token in line.split('`').skip(1).step_by(2) {
            if let Some(p) = as_path(token, &names) {
                out.push(p);
            }
        }
    }
    let mut seen = HashSet::new();
    out.retain(|p| seen.insert(p.clone()));
    out
}

/// One path a page names that no repo tracks.
#[derive(Debug, Clone, PartialEq)]
pub struct MissingPath {
    pub path: String,
    /// The repo the page cites it in, when it says (`repo:path`).
    pub repo: Option<String>,
    /// Where a file or folder of the same name is now, as `repo:path`; at
    /// most five, empty when no repo has one.
    pub now: Vec<String>,
}

/// One repo's tracked files, split into segments.
struct Tracked {
    name: String,
    root: PathBuf,
    files: Vec<Vec<String>>,
    /// Every folder name anywhere in the repo.
    folders: HashSet<String>,
}

/// Whether segment `s` matches pattern segment `pat`, where `*`, `<name>`
/// and `{name}` stand for any run of characters.
fn seg_match(pat: &str, s: &str) -> bool {
    let mut p = String::new();
    let mut skip: Option<char> = None;
    for c in pat.chars() {
        match (skip, c) {
            (Some(end), c) if c == end => skip = None,
            (Some(_), _) => {}
            (None, '<') => {
                skip = Some('>');
                p.push('*');
            }
            (None, '{') => {
                skip = Some('}');
                p.push('*');
            }
            (None, c) => p.push(c),
        }
    }
    let parts: Vec<&str> = p.split('*').collect();
    if parts.len() == 1 {
        return p == s;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !s.starts_with(first) || s.len() < first.len() + last.len() || !s.ends_with(last) {
        return false;
    }
    let mut rest = &s[first.len()..s.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(j) => rest = &rest[j + part.len()..],
            None => return false,
        }
    }
    true
}

/// A file name without its extension: what a doc calls a class or module.
fn stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(s, _)| s)
}

impl Tracked {
    /// Whether the repo has `path`: a tracked file that is it or ends with
    /// `/` and it (docs often name a path from a source root), or, for a
    /// path with no extension, a folder whose path is it or ends so, or a
    /// file that is it once its extension is dropped
    /// (`systems/combat/SRDamageFilterSystem` is the `.java` file).
    fn has(&self, path: &str, is_file: bool) -> bool {
        let pat: Vec<&str> = path.trim_end_matches('/').split('/').collect();
        let tail_matches = |f: &[String], last: &dyn Fn(&str) -> &str| {
            f.len() >= pat.len()
                && f[f.len() - pat.len()..].iter().zip(&pat).enumerate().all(|(i, (s, p))| {
                    let s = if i + 1 == pat.len() { last(s) } else { s.as_str() };
                    seg_match(p, s)
                })
        };
        self.files.iter().any(|f| {
            if is_file {
                tail_matches(f, &|s| s)
            } else {
                f[..f.len() - 1].windows(pat.len()).any(|w| w.iter().zip(&pat).all(|(s, p)| seg_match(p, s)))
                    || (!path.ends_with('/') && tail_matches(f, &stem))
            }
        })
    }

    /// Whether `path` is in the checkout though git does not track it: on
    /// disk, or a path git ignores (a build output, a local file).
    fn untracked(&self, path: &str) -> bool {
        if path.split('/').any(is_wild) {
            return false;
        }
        if self.root.join(path).exists() {
            return true;
        }
        let mut cmd = Command::new("git");
        crate::proc::quiet(&mut cmd)
            .args(["check-ignore", "-q", "--no-index", "--", path])
            .current_dir(&self.root)
            .status()
            .is_ok_and(|s| s.success())
    }

    /// Where the repo has a file (or, for a folder or a pattern, a folder)
    /// named as the last plain segment of `path`: at most five paths.
    fn lookup(&self, path: &str, is_file: bool) -> Vec<String> {
        let segs: Vec<&str> = path.trim_end_matches('/').split('/').collect();
        let (name, as_file) = match segs.last() {
            Some(l) if !is_wild(l) => (*l, is_file),
            _ => match segs.iter().rev().skip(1).find(|s| !is_wild(s)) {
                Some(s) => (*s, false),
                None => return Vec::new(),
            },
        };
        let mut found: BTreeSet<String> = BTreeSet::new();
        for f in &self.files {
            let last = f.last().map(String::as_str).unwrap_or_default();
            if last == name || (!as_file && !path.ends_with('/') && stem(last) == name && stem(last) != last) {
                found.insert(f.join("/"));
            }
            if !as_file {
                for i in (0..f.len() - 1).filter(|i| f[*i] == name) {
                    found.insert(format!("{}/", f[..=i].join("/")));
                }
            }
        }
        found.into_iter().take(5).collect()
    }
}

/// The files each of the team's repos tracks, to check the paths a drafted
/// page names against.
#[derive(Default)]
pub struct PathCheck {
    repos: Vec<Tracked>,
}

impl PathCheck {
    pub fn of(repos: &[(String, PathBuf)]) -> PathCheck {
        let repos = repos
            .iter()
            .filter_map(|(name, root)| {
                let files: Vec<Vec<String>> =
                    ls_files(root)?.iter().map(|f| f.split('/').map(str::to_string).collect()).collect();
                let folders = files.iter().flat_map(|f| f[..f.len() - 1].iter().cloned()).collect();
                Some(Tracked { name: name.clone(), root: root.clone(), files, folders })
            })
            .collect();
        PathCheck { repos }
    }

    /// The paths `page` names that no repo has, each with where a file or
    /// folder of its name is now. A cited path is looked for in the repo it
    /// cites, any other in every repo, `home` (the repo a Repo Map page is
    /// about) first. A path with no file extension is checked only when its
    /// first segment is a folder the repo has somewhere, so `owner/repo` and
    /// branch names are left alone.
    pub fn missing(&self, page: &str, home: Option<&str>) -> Vec<MissingPath> {
        if self.repos.is_empty() {
            return Vec::new();
        }
        let names: Vec<&str> = self.repos.iter().map(|r| r.name.as_str()).collect();
        let mut out = Vec::new();
        for (repo, path) in named_paths(page, &names) {
            let mut scope: Vec<&Tracked> = self.repos.iter().filter(|t| repo.as_ref().is_none_or(|r| *r == t.name)).collect();
            scope.sort_by_key(|t| Some(t.name.as_str()) != home);
            let is_file = has_file_ext(&path);
            if !is_file {
                let first = path.split('/').next().unwrap_or("");
                if !path.trim_end_matches('/').contains('/') || is_wild(first) || !scope.iter().any(|t| t.folders.contains(first)) {
                    continue;
                }
            }
            if scope.iter().any(|t| t.has(&path, is_file) || t.untracked(&path)) {
                continue;
            }
            let now: Vec<String> = scope
                .iter()
                .flat_map(|t| t.lookup(&path, is_file).into_iter().map(move |p| format!("{}:{p}", t.name)))
                .take(5)
                .collect();
            out.push(MissingPath { path, repo, now });
        }
        out
    }
}

/// The one call that corrects a drafted page's missing paths: each path,
/// and where the code has it now or that no repo does.
pub fn correction_prompt(page_path: &str, page: &str, missing: &[MissingPath]) -> String {
    let mut s = format!(
        "You drafted `{page_path}`, one page of a team's wiki. Ken checked every repo path the page names against the \
         files each repo tracks in git, and these are not there:\n\n"
    );
    for m in missing {
        let cited = m.repo.as_deref().map(|r| format!(" (cited in {r})")).unwrap_or_default();
        if m.now.is_empty() {
            s.push_str(&format!("- `{}`{cited}: no repo of the team has a file or folder of that name\n", m.path));
        } else {
            let at: Vec<String> = m.now.iter().map(|p| format!("`{p}`")).collect();
            s.push_str(&format!("- `{}`{cited}: a file or folder of that name is at {}\n", m.path, at.join(", ")));
        }
    }
    s.push_str(
        "\nCorrect the page: write each of these where the code has it now, choosing by what the page says about it; \
         where the code no longer has it, leave it out or say it is gone. Two exceptions, kept as they are: a path the \
         sources place outside these repos (another repo, the docs vault), and a file the sources say something makes \
         (a build, the server at run time, a person). Change nothing else. Reply with the whole corrected page, starting \
         with `---`. No preamble, no code fences around it.\n\nTHE PAGE:\n",
    );
    s.push_str(page);
    s
}

/// `text`, a page just drafted, with the paths it names checked: when any
/// is missing, ONE call to the model lists them with where the code has them
/// now and takes the corrected page back. Never a second call: a path still
/// wrong after it is for the person reading the draft. The page as drafted
/// when the call fails.
fn check_paths(
    page_path: &str,
    text: String,
    check: &PathCheck,
    report: &mut DraftReport,
    generate: &mut impl FnMut(&str) -> Result<String>,
) -> String {
    let home = check.repos.iter().find(|t| repo_page(&t.name) == page_path).map(|t| t.name.as_str());
    let missing = check.missing(&text, home);
    if missing.is_empty() {
        return text;
    }
    match generate(&correction_prompt(page_path, &text, &missing)).and_then(|r| finish(&r)) {
        Ok(fixed) => {
            let n = missing.len();
            report.corrected.push(format!("{page_path}: {n} path{}", if n == 1 { "" } else { "s" }));
            fixed
        }
        Err(_) => text,
    }
}

//// What a draft run did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftReport {
    pub drafted: Vec<String>,
    /// Pages a person already wrote, left alone.
    pub kept: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub sources: Vec<String>,
    /// Changes to pages a person keeps, filed for them to apply or discard.
    #[serde(default)]
    pub proposed: Vec<String>,
    /// Placeholders left in the wiki after the draft, as `page: placeholder`.
    #[serde(default)]
    pub to_fill: Vec<String>,
    /// Pages whose named paths the checkout did not have, sent back once
    /// to be corrected, as `page: n paths`.
    #[serde(default)]
    pub corrected: Vec<String>,
}

/// The template placeholders still in `wiki`'s pages, as `page: placeholder`.
/// A `verified:` date is a person's to set, and `Templates/` are copied from,
/// so neither counts.
pub fn placeholders_left(wiki: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let pages = crate::wikinew::TEMPLATE
        .iter()
        .map(|(p, _)| *p)
        .filter(|p| p.ends_with(".md") && !p.starts_with("Templates/"));
    for page in pages {
        let Ok(text) = fs::read_to_string(wiki.join(page)) else { continue };
        for line in text.lines().filter(|l| !l.trim_start().starts_with("verified:")) {
            let mut rest = line;
            while let Some(i) = rest.find("{{") {
                let Some(j) = rest[i..].find("}}") else { break };
                out.push(format!("{page}: {}", &rest[i..i + j + 2]));
                rest = &rest[i + j + 2..];
            }
        }
    }
    out.dedup();
    out
}

fn write_page(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    fs::write(path, text).map_err(|e| Error::io(path, e))
}

/// Each git repo's head on the branch drift measures, by name: what a draft
/// read, so its citations can say so.
fn repo_heads(repos: &[(String, PathBuf)]) -> HashMap<String, String> {
    repos
        .iter()
        .filter_map(|(name, root)| {
            // The commit the draft read: what is checked out, whichever branch.
            let mut cmd = std::process::Command::new("git");
            let out = crate::proc::quiet(&mut cmd).args(["rev-parse", "--short=12", "HEAD"]).current_dir(root).output().ok()?;
            let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
            (out.status.success() && !sha.is_empty()).then(|| (name.clone(), sha))
        })
        .collect()
}

/// Pin each code citation in a page's `sources:` to the commit its repo
/// was read at (`repo:path` → `repo@sha:path`). A draft dated today would
/// otherwise take in every commit made later that same day, and a change
/// right after drafting would never read as drift.
pub fn pin_sources(page: &str, heads: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(page.len() + 64);
    let mut in_front = false;
    let mut in_sources = false;
    for (i, line) in page.split_inclusive('\n').enumerate() {
        let bare = line.trim_end();
        if bare == "---" {
            in_front = i == 0;
            in_sources = false;
            out.push_str(line);
            continue;
        }
        if in_front && !line.starts_with(' ') && !line.starts_with('-') {
            in_sources = bare.starts_with("sources:");
        }
        let item = bare.trim_start().strip_prefix("- ");
        let pinned = item.filter(|_| in_front && in_sources).and_then(|item| {
            let quote = item.chars().next().filter(|q| *q == '"' || *q == '\'');
            let inner = quote.map_or(item, |q| item.trim_matches(q));
            let (repo, path) = inner.split_once(':')?;
            if repo.contains('@') || path.starts_with('(') {
                return None;
            }
            let sha = heads.get(repo)?;
            Some(line.replacen(&format!("{repo}:"), &format!("{repo}@{sha}:"), 1))
        });
        out.push_str(pinned.as_deref().unwrap_or(line));
    }
    out
}

/// Pin the sources of the pages just drafted (see [`pin_sources`]).
fn pin_drafted(wiki: &Path, drafted: &[String], repos: &[(String, PathBuf)]) -> Result<()> {
    let heads = repo_heads(repos);
    if heads.is_empty() {
        return Ok(());
    }
    for page in drafted {
        let path = wiki.join(page);
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let pinned = pin_sources(&text, &heads);
        if pinned != text {
            write_page(&path, &pinned)?;
        }
    }
    Ok(())
}

/// Draft each of `pages` the wiki does not already have into `report`, each
/// checked for paths the checkout does not have ([`check_paths`]).
#[allow(clippy::too_many_arguments)]
fn draft_pages(
    wiki: &Path,
    pages: &[(String, String)],
    sources: &[Source],
    today: &str,
    report: &mut DraftReport,
    generate: &mut impl FnMut(&str) -> Result<String>,
    check: &PathCheck,
) -> Result<()> {
    for (page, purpose) in pages {
        draft_page(wiki, page, purpose, None, sources, today, report, generate, check)?;
    }
    Ok(())
}

/// Draft `page` unless the wiki already has it, from the page itself while
/// it is a template, else from `blank` (a copy of a `Templates/` blank).
#[allow(clippy::too_many_arguments)]
fn draft_page(
    wiki: &Path,
    page: &str,
    purpose: &str,
    blank: Option<&str>,
    sources: &[Source],
    today: &str,
    report: &mut DraftReport,
    generate: &mut impl FnMut(&str) -> Result<String>,
    check: &PathCheck,
) -> Result<()> {
    let path = wiki.join(page);
    let existing = fs::read_to_string(&path).ok();
    if !may_draft(existing.as_deref()) {
        report.kept.push(page.to_string());
        return Ok(());
    }
    match generate(&prompt(page, purpose, existing.as_deref().or(blank), sources, today)).and_then(|r| finish(&r)) {
        Ok(text) => {
            let text = check_paths(page, text, check, report, generate);
            write_page(&path, &text)?;
            report.drafted.push(page.to_string());
        }
        Err(e) => report.failed.push((page.to_string(), e.to_string())),
    }
    Ok(())
}

/// A draft's result as the wiki keeps it, for the Team screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Drafted {
    pub title: String,
    pub at: i64,
    pub report: DraftReport,
}

/// Keep the draft's result (the latest one replaces the one before).
fn file_card(db: &mut Db, report: &DraftReport, title: &str, now: i64) -> Result<()> {
    let d = Drafted { title: title.to_string(), at: now, report: report.clone() };
    db.store_wiki_draft(&serde_json::to_string(&d).map_err(|e| Error::Other(e.to_string()))?)
}

/// The last draft's result, if there was one.
pub fn last_draft(db: &Db) -> Option<Drafted> {
    db.wiki_draft().ok().flatten().and_then(|j| serde_json::from_str(&j).ok())
}

/// One thing a draft left for a person, for the Team screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// `draft` (a page drafted, still to read) · `draft-failed` · `proposal`
    /// (a change to a kept page, to apply or discard) · `repo-removed`
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub path: String,
    /// A proposal's Review-store id, for Apply and Discard.
    pub item_id: Option<i64>,
}

/// Whether a page still says `status: draft`.
fn still_draft(wiki: &Path, page: &str) -> bool {
    let Ok(text) = fs::read_to_string(wiki.join(page)) else { return false };
    let mut lines = text.lines();
    lines.next().is_some_and(|l| l.trim_end() == "---")
        && lines
            .take_while(|l| l.trim_end() != "---")
            .any(|l| l.trim().strip_prefix("status:").is_some_and(|v| v.trim().trim_matches('"') == "draft"))
}

/// What the wiki's drafts left for a person: the pages drafted and not yet
/// read (still `status: draft`), the pages Ken could not draft and that are
/// still missing, the open proposals to pages a person keeps, and repos that
/// left the team while pages still cite them.
pub fn findings(wiki: &Path, db: &Db) -> Result<Vec<Finding>> {
    let mut out = Vec::new();
    if let Some(d) = last_draft(db) {
        for page in d.report.drafted.iter().filter(|p| still_draft(wiki, p)) {
            out.push(Finding {
                kind: "draft".into(),
                title: format!("{page} is a draft to read"),
                detail: "Ken drafted it from the team's repos. Read it against its sources, then set its status.".into(),
                path: page.clone(),
                item_id: None,
            });
        }
        for (page, why) in &d.report.failed {
            if may_draft(fs::read_to_string(wiki.join(page)).ok().as_deref()) {
                out.push(Finding {
                    kind: "draft-failed".into(),
                    title: format!("Ken could not draft {page}"),
                    detail: why.clone(),
                    path: page.clone(),
                    item_id: None,
                });
            }
        }
    }
    for it in db.list_open_review_items()? {
        if it.kind != PROPOSAL_KIND {
            continue;
        }
        let Some(p) = it.payload.as_deref().and_then(|j| serde_json::from_str::<Proposal>(j).ok()) else { continue };
        // A proposal from an ingest waits on its card on the Ingest screen.
        if p.from.is_some() {
            continue;
        }
        out.push(Finding { kind: "proposal".into(), title: it.title, detail: it.body, path: p.page, item_id: Some(it.id) });
    }
    for repo in removed_repos(db) {
        let pages = citing_pages(db, &repo)?;
        if pages.is_empty() {
            continue;
        }
        out.push(Finding {
            kind: "repo-removed".into(),
            title: format!("{repo} left the team: {} page{} cite it", pages.len(), if pages.len() == 1 { "" } else { "s" }),
            detail: format!(
                "Rewrite each without it, or retire it (`status: retired` with what replaced it): {}.",
                pages.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>().join(", ")
            ),
            path: pages[0].0.clone(),
            item_id: None,
        });
    }
    Ok(out)
}

/// Repos recorded as having left the team ([`record_removed_repo`]).
pub fn removed_repos(db: &Db) -> Vec<String> {
    db.removed_repos().ok().flatten().and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default()
}

fn team_pages() -> Vec<(String, String)> {
    PAGES.iter().map(|(p, u)| (p.to_string(), u.to_string())).collect()
}

/// Draft every page of [`PAGES`] the wiki does not already have, then file
/// keep the result (what was drafted and kept) for the Team screen.
pub fn draft(
    wiki: &Path,
    db: &mut Db,
    sources: &[Source],
    today: &str,
    now: i64,
    mut generate: impl FnMut(&str) -> Result<String>,
) -> Result<DraftReport> {
    let mut report = DraftReport { sources: sources.iter().map(|s| s.label.clone()).collect(), ..Default::default() };
    draft_pages(wiki, &team_pages(), sources, today, &mut report, &mut generate, &PathCheck::default())?;
    report.to_fill = placeholders_left(wiki);
    let title = format!("First wiki drafted: {} pages to read", report.drafted.len());
    file_card(db, &report, &title, now)?;
    Ok(report)
}

// --- The wiki at team scale: a Repo Map page per repo, then the team pages
// drafted from those pages, so twenty repos never overflow one prompt. ----

/// The section for what lives where in the code (the template's `Repo-Map/`).
pub const REPO_MAP: &str = "Repo-Map";

const REPO_PURPOSE: &str = "one repo's page in the Repo Map: what the repo is for (the team's own description first), \
    what lives where in it (its layout and entry points), which of its folders call which and the files most imported \
    (from the imports between folders), \
    who owns it (from CODEOWNERS, `people/` files and its docs) and who has committed recently (from the recent contributors), \
    how it is built, run and released (from its build files, `.ken/gates.json` and its brief's build sections), \
    and how it connects to the team's other repos";

/// A repo's page in the wiki: `Repo-Map/<name>.md`.
pub fn repo_page(name: &str) -> String {
    format!("{REPO_MAP}/{}.md", crate::workspace::member_leaf(name))
}

/// A repo's own pages beside its Repo Map page, by its kind, each from a
/// blank in `Templates/` and copied where `Templates/Index.md` says: (the
/// kind, the blank, where the copy goes with `{}` for the repo, the blank's
/// placeholders that are the repo's name, what the page is for). A code repo
/// gets its layers; a reference repo, which the team reads and never
/// changes, gets a dependency page and never an architecture page.
pub const KIND_PAGES: &[(crate::registry::RepoKind, &str, &str, &[&str], &str)] = &[
    (
        crate::registry::RepoKind::Code,
        "Templates/Repo-Architecture.md",
        "Conventions/Architecture-{}.md",
        &["{{Repo}}", "{{repo}}"],
        "one code repo's layers: each layer, the folder it lives in, what it holds and which layers it may call; \
         what must never call what, and why where a source says; every pair of folders the code has importing each other, as a known gap; \
         the checks that enforce a layer rule; and a mermaid diagram",
    ),
    (
        crate::registry::RepoKind::Reference,
        "Templates/Dependency.md",
        "Platform/{}.md",
        &["{{Dependency Name}}", "{{dependency}}"],
        "one reference repo the team reads and never changes: what it is, the version pinned and where the pin is, \
         where its source, docs and data live, and its licence; its quirks and update procedure only where a source says",
    ),
];

/// The pages of [`KIND_PAGES`] a repo of `kinds` gets: (page, blank, purpose).
pub fn kind_pages(name: &str, kinds: &[crate::registry::RepoKind]) -> Vec<(String, &'static str, &'static str)> {
    use crate::registry::RepoKind;
    let leaf = crate::workspace::member_leaf(name);
    KIND_PAGES
        .iter()
        .filter(|(k, ..)| kinds.contains(k) && !(*k == RepoKind::Code && kinds.contains(&RepoKind::Reference)))
        .map(|(_, blank, at, _, purpose)| (at.replace("{}", leaf), *blank, *purpose))
        .collect()
}

/// The blank a repo's page is drafted from: the wiki's own copy of it, else
/// the bundled one, with the repo's name filled in.
fn blank_for(wiki: &Path, blank: &str, name: &str) -> String {
    let text = fs::read_to_string(wiki.join(blank))
        .ok()
        .or_else(|| crate::wikinew::TEMPLATE.iter().find(|(p, _)| *p == blank).map(|(_, t)| t.to_string()))
        .unwrap_or_default();
    let leaf = crate::workspace::member_leaf(name);
    let names = KIND_PAGES.iter().find(|(_, b, ..)| *b == blank).map_or(&[][..], |(_, _, _, n, _)| *n);
    names.iter().fold(text, |t, p| t.replace(p, leaf))
}

/// The Repo Map's index: a table of every repo page with what it is for.
/// Written by Ken, not the model: it is a list, and a list has no facts to
/// get wrong.
pub fn repo_map_index(repos: &[(String, String)], today: &str) -> String {
    let mut s = format!(
        "---\ntitle: \"Repo Map\"\naliases: [\"Repo-Map\", \"what lives where\"]\ntags: [moc]\nupdated: {today}\n---\n\n\
         # Repo Map\n\nWhat lives where in the code, one page per repo. Open the repo's page before searching its files.\n\n\
         | repo | what it is for |\n|---|---|\n"
    );
    for (name, description) in repos {
        s.push_str(&index_row(name, description));
    }
    s
}

fn index_row(name: &str, description: &str) -> String {
    let leaf = crate::workspace::member_leaf(name);
    let d = description.split_whitespace().collect::<Vec<_>>().join(" ").replace('|', "/");
    format!("| [[{leaf}]] | {} |\n", if d.is_empty() { "(not said yet)" } else { &d })
}

/// What a drafted Repo Map page says its repo is for: the first sentence
/// of its first plain paragraph (past the frontmatter, the title, the
/// drafted-by line, headings, tables, lists and quotes), citations taken
/// out. Tools had no README, so on 2026-10-06 the index and START-HERE said
/// "(not said yet)" while its own page said what it is.
pub fn page_purpose(page: &str) -> Option<String> {
    let body = page
        .strip_prefix("---\n")
        .and_then(|rest| rest.find("\n---").map(|i| &rest[i + 4..]))
        .unwrap_or(page);
    let mut para: Vec<&str> = Vec::new();
    for line in body.lines().map(str::trim) {
        let plain = !line.is_empty()
            && !line.starts_with(['#', '|', '-', '*', '+', '>', '!', '`', '<', '['])
            && !line.starts_with(|c: char| c.is_ascii_digit())
            && !line.starts_with("Drafted by Ken");
        if plain {
            para.push(line);
        } else if !para.is_empty() {
            break;
        }
    }
    // Backticked `repo:path` citations are for checking, not for a summary.
    let joined = para.join(" ");
    let mut text = String::new();
    for (i, part) in joined.split('`').enumerate() {
        let citation = i % 2 == 1 && part.contains(':') && !part.contains(' ');
        if !citation {
            text.push_str(&if i % 2 == 1 { format!("`{part}`") } else { part.to_string() });
        }
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ").replace("**", "");
    let sentence = match text.find(". ") {
        Some(i) => &text[..=i],
        None => text.as_str(),
    };
    let sentence = sentence.trim();
    if sentence.is_empty() {
        return None;
    }
    Some(if sentence.chars().count() > 220 { format!("{}…", sentence.chars().take(219).collect::<String>()) } else { sentence.to_string() })
}

/// What a repo is for, for the Repo Map index and START-HERE: what the team
/// said at set-up, else what its drafted Repo Map page says ([`page_purpose`]).
fn describe_repo(wiki: &Path, name: &str, root: &Path) -> String {
    crate::registry::description_of(root)
        .filter(|d| !d.trim().is_empty())
        .or_else(|| fs::read_to_string(wiki.join(repo_page(name))).ok().and_then(|p| page_purpose(&p)))
        .unwrap_or_default()
}

/// START-HERE's repo rows that set-up left as "(not said yet)", filled in
/// from `described` once a repo's purpose is known. Only those exact rows:
/// a row a person wrote is theirs. Whether anything changed.
fn fill_start_here(wiki: &Path, described: &[(String, String)]) -> Result<bool> {
    let path = wiki.join("START-HERE.md");
    let Ok(text) = fs::read_to_string(&path) else { return Ok(false) };
    let mut out = String::with_capacity(text.len());
    let mut changed = false;
    for line in text.split_inclusive('\n') {
        let row = described.iter().find(|(n, d)| !d.trim().is_empty() && line.trim_end() == format!("| `{n}` | (not said yet) |"));
        match row {
            Some((n, d)) => {
                let d = d.split_whitespace().collect::<Vec<_>>().join(" ").replace('|', "/");
                out.push_str(&format!("| `{n}` | {d} |\n"));
                changed = true;
            }
            None => out.push_str(line),
        }
    }
    if changed {
        write_page(&path, &out)?;
    }
    Ok(changed)
}

/// The repo pages and descriptions every team page is drafted from, each
/// repo's share of [`SOURCE_BUDGET`] equal, then `extra`. What only one page
/// needs is in [`page_sources`].
pub fn team_sources(wiki: &Path, wiki_name: &str, repos: &[(String, PathBuf)], extra: &[Source]) -> Vec<Source> {
    let share = (SOURCE_BUDGET * 2 / 3) / repos.len().max(1);
    let mut out = Vec::new();
    for (name, root) in repos {
        if let Some(d) = crate::registry::description_of(root) {
            out.push(Source { label: format!("{name}:(what this repo is for, in the team's words)"), text: clip(&d, 2_000) });
        }
        let page = repo_page(name);
        if let Some(t) = read_text(&wiki.join(&page)) {
            out.push(Source { label: format!("{wiki_name}:{page}"), text: clip(&t, share) });
        }
    }
    let used: usize = out.iter().map(|s| s.text.len()).sum();
    let mut left = SOURCE_BUDGET.saturating_sub(used);
    for s in extra {
        if left == 0 {
            break;
        }
        let text = clip(&s.text, left.min(PER_FILE));
        left = left.saturating_sub(text.len());
        out.push(Source { label: s.label.clone(), text });
    }
    out
}

/// The brief topics each team page reads, beside the repo pages.
fn page_topics(page: &str) -> &'static [Topic] {
    match page {
        "Current/Project.md" => &[Topic::Project],
        "Current/Feature-Status.md" => &[Topic::Project, Topic::Releases],
        "Current/Team.md" | "Current/Who-Does-What.md" => &[Topic::People],
        "Conventions/Architecture.md"
        | "Conventions/Code.md"
        | "Conventions/Data-and-Config.md"
        | "Conventions/Registries.md"
        | "Platform/Index.md"
        | "Reference/Systems.md"
        | "Reference/Config-Map.md" => &[Topic::Architecture],
        "Conventions/Testing.md" | "Reference/Build-and-Run.md" => &[Topic::Build],
        "Work/Releases.md" => &[Topic::Releases],
        "Reference/Vocabulary.md" => &[Topic::Vocabulary],
        _ => &[],
    }
}

/// Split `budget` between repos that would read `sizes`: an equal share
/// each, and what a repo needs less than its share goes to the others. A
/// strict equal share gave the mod's 40,000 characters of architecture the
/// same 15,000 as a repo with 1,000.
fn fair_shares(sizes: &[usize], budget: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|i| sizes[*i]);
    let mut out = vec![0; sizes.len()];
    let mut left = budget;
    for (k, i) in order.into_iter().enumerate() {
        out[i] = sizes[i].min(left / (sizes.len() - k));
        left -= out[i];
    }
    out
}

/// `sources` in order while `budget` lasts, the last one clipped to fit.
fn within(sources: Vec<Source>, budget: usize) -> Vec<Source> {
    let mut out = Vec::new();
    let mut used = 0;
    for s in sources {
        if used >= budget {
            break;
        }
        let text = clip(&s.text, budget - used);
        used += text.len();
        out.push(Source { label: s.label, text });
    }
    out
}

/// What one team page reads from the repos themselves, beside the repo
/// pages, within [`PAGE_BUDGET`] shared fairly between the repos
/// ([`fair_shares`]): for Releases the version tags, what landed since the
/// newest and each version's own notes; for Team and Who Does What the
/// CODEOWNERS, `people/` files (the wiki's too) and the recent contributors;
/// for the architecture page the import map; for Project the newest version
/// tags; then for every page the brief sections whose topic serves it
/// ([`page_topics`]). `roster` is the team's ([`team_roster`]).
pub fn page_sources(
    page: &str,
    wiki: &Path,
    wiki_name: &str,
    repos: &[(String, PathBuf)],
    roster: &[crate::people::Person],
) -> Vec<Source> {
    let people = matches!(page, "Current/Team.md" | "Current/Who-Does-What.md");
    let mut per_repo: Vec<Vec<Source>> = Vec::new();
    for (name, root) in repos {
        let mut mine: Vec<Source> = Vec::new();
        let push = pusher_of(PAGE_BUDGET, SECTION_MAX.max(LAYOUT_BUDGET));
        let used = |v: &[Source]| v.iter().map(|s| s.text.len()).sum::<usize>();
        match page {
            "Work/Releases.md" => {
                if let Some(t) = git_tags(root) {
                    push(format!("{name}:(version tags, newest first)"), t, &mut mine);
                }
                if let Some((label, log)) = git_unreleased(root) {
                    push(format!("{name}:{label}"), clip(&log, PER_FILE), &mut mine);
                }
                let left = PAGE_BUDGET.saturating_sub(used(&mine));
                mine.extend(release_notes(name, root, usize::MAX, left));
            }
            "Current/Project.md" => {
                if let Some(t) = git_tags(root) {
                    let newest: Vec<&str> = t.lines().take(5).collect();
                    push(format!("{name}:(version tags, newest first)"), newest.join("\n"), &mut mine);
                }
            }
            "Conventions/Architecture.md" => {
                let reference = crate::registry::index_of(root).0.contains(&crate::registry::RepoKind::Reference);
                if !reference {
                    if let Some(d) = folder_imports(root) {
                        push(format!("{name}:(imports between folders, from the code)"), d, &mut mine);
                    }
                }
            }
            _ if people => {
                for f in ["CODEOWNERS", ".github/CODEOWNERS", "docs/CODEOWNERS"] {
                    if let Ok(t) = fs::read_to_string(root.join(f)) {
                        push(format!("{name}:{f}"), t, &mut mine);
                    }
                }
                for s in people_files(name, root) {
                    push(s.label, s.text, &mut mine);
                }
                if let Some(a) = recent_contributors(root, roster) {
                    push(format!("{name}:(recent contributors by folder, last 90 days, from git)"), a, &mut mine);
                }
            }
            _ => {}
        }
        let topics = page_topics(page);
        let left = PAGE_BUDGET.saturating_sub(used(&mine));
        mine.extend(sections_for(name, &briefs(root), |t| topics.contains(&t), left));
        per_repo.push(mine);
    }
    let sizes: Vec<usize> = per_repo.iter().map(|v| v.iter().map(|s| s.text.len()).sum()).collect();
    let mut out: Vec<Source> =
        per_repo.into_iter().zip(fair_shares(&sizes, PAGE_BUDGET)).flat_map(|(v, share)| within(v, share)).collect();
    if people {
        out.extend(people_files(wiki_name, wiki));
    }
    out
}

/// Every label once, in the order first seen.
fn dedup_labels(labels: &mut Vec<String>) {
    let mut seen = HashSet::new();
    labels.retain(|l| seen.insert(l.clone()));
}

/// Pass one: each repo's Repo Map page and its pages by kind
/// ([`kind_pages`]), from that repo alone.
fn draft_repo_pages(
    wiki: &Path,
    repos: &[(String, PathBuf)],
    roster: &[crate::people::Person],
    today: &str,
    report: &mut DraftReport,
    generate: &mut impl FnMut(&str) -> Result<String>,
    check: &PathCheck,
) -> Result<()> {
    for (name, root) in repos {
        let mut pages = vec![(repo_page(name), REPO_PURPOSE, None)];
        for (page, blank, purpose) in kind_pages(name, &crate::registry::kind_of(root)) {
            pages.push((page, purpose, Some(blank_for(wiki, blank, name))));
        }
        let mut sources: Option<Vec<Source>> = None;
        for (page, purpose, blank) in pages {
            // A page a person keeps is not drafted, so its sources are not read.
            if !may_draft(fs::read_to_string(wiki.join(&page)).ok().as_deref()) {
                report.kept.push(page);
                continue;
            }
            if sources.is_none() {
                let read = gather_repo(name, root, roster);
                report.sources.extend(read.iter().map(|s| s.label.clone()));
                sources = Some(read);
            }
            let sources = sources.as_deref().unwrap_or_default();
            draft_page(wiki, &page, purpose, blank.as_deref(), sources, today, report, generate, check)?;
        }
    }
    Ok(())
}

/// Draft each team page of `pages` from the sources every page shares and
/// its own ([`page_sources`]), read only for a page that will be drafted.
#[allow(clippy::too_many_arguments)]
fn draft_team_pages(
    wiki: &Path,
    wiki_name: &str,
    pages: &[(String, String)],
    common: &[Source],
    repos: &[(String, PathBuf)],
    roster: &[crate::people::Person],
    today: &str,
    report: &mut DraftReport,
    generate: &mut impl FnMut(&str) -> Result<String>,
    check: &PathCheck,
) -> Result<()> {
    for (page, purpose) in pages {
        let mut sources = common.to_vec();
        if may_draft(fs::read_to_string(wiki.join(page)).ok().as_deref()) {
            sources.extend(page_sources(page, wiki, wiki_name, repos, roster));
            report.sources.extend(sources.iter().map(|s| s.label.clone()));
        }
        draft_pages(wiki, &[(page.clone(), purpose.clone())], &sources, today, report, generate, check)?;
    }
    Ok(())
}

/// The team's roster: the `people/` files of the wiki and of `repos`, so a
/// team repo is read wherever it sits, and a wiki that holds the team's
/// folders too.
fn team_roster(wiki: &Path, repos: &[(String, PathBuf)]) -> Vec<crate::people::Person> {
    crate::people::roster_of(std::iter::once(wiki).chain(repos.iter().map(|(_, r)| r.as_path())))
}

/// Rebuild the team's list of the rulings its code cites
/// ([`crate::teamnew::write_cited_in_code`]) from the code repos of `repos`,
/// in the team repo: the wiki when it holds the decisions log, else the
/// first of `repos` that does. Nothing to do without a log.
fn cite_rulings(wiki: &Path, repos: &[(String, PathBuf)], today: &str) -> Result<()> {
    use crate::registry::RepoKind;
    let roots = std::iter::once(wiki).chain(repos.iter().map(|(_, r)| r.as_path()));
    let Some(team) = roots.into_iter().find(|r| r.join(crate::ingest::TEAM_DECISIONS).is_file()) else { return Ok(()) };
    let code: Vec<(String, PathBuf)> = repos
        .iter()
        .filter(|(_, r)| {
            let kinds = crate::registry::kind_of(r);
            kinds.contains(&RepoKind::Code) && !kinds.contains(&RepoKind::Reference)
        })
        .cloned()
        .collect();
    crate::teamnew::write_cited_in_code(team, &code, today).map(|_| ())
}

/// Draft a team's wiki: a Repo Map page per repo (each from that repo
/// alone) and its pages by kind, the Repo Map index, then the team pages of
/// [`PAGES`] from the repo pages and descriptions, and the team repo's
/// rulings cited in code ([`cite_rulings`]). `repos` are the team's repos,
/// the wiki itself left out. The result is kept for the Team screen.
#[allow(clippy::too_many_arguments)]
pub fn draft_team(
    wiki: &Path,
    wiki_name: &str,
    db: &mut Db,
    repos: &[(String, PathBuf)],
    extra: Option<&Path>,
    today: &str,
    now: i64,
    mut generate: impl FnMut(&str) -> Result<String>,
) -> Result<DraftReport> {
    let mut report = DraftReport::default();
    let check = PathCheck::of(repos);
    let roster = team_roster(wiki, repos);
    draft_repo_pages(wiki, repos, &roster, today, &mut report, &mut generate, &check)?;
    let described: Vec<(String, String)> = repos.iter().map(|(n, r)| (n.clone(), describe_repo(wiki, n, r))).collect();
    let index = format!("{REPO_MAP}/Index.md");
    let index_path = wiki.join(&index);
    if may_draft(fs::read_to_string(&index_path).ok().as_deref()) {
        write_page(&index_path, &repo_map_index(&described, today))?;
        report.drafted.push(index);
    }
    fill_start_here(wiki, &described)?;
    let extra = extra.map(gather_extra).unwrap_or_default();
    let common = team_sources(wiki, wiki_name, repos, &extra);
    draft_team_pages(wiki, wiki_name, &team_pages(), &common, repos, &roster, today, &mut report, &mut generate, &check)?;
    cite_rulings(wiki, repos, today)?;
    dedup_labels(&mut report.sources);
    pin_drafted(wiki, &report.drafted, repos)?;
    report.to_fill = placeholders_left(wiki);
    let title = format!("First wiki drafted: {} pages to read", report.drafted.len());
    file_card(db, &report, &title, now)?;
    Ok(report)
}

// --- A repo added later: its page is new, so Ken drafts it; the pages a
// person already keeps get a proposed change, never a write. -------------

/// Review kind of a proposed change to one page.
pub const PROPOSAL_KIND: &str = "page-proposal";

/// A change to one page, held until a person applies it. `base` is the page
/// as Ken read it; the change applies only while the page still reads so.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub page: String,
    pub base: String,
    pub proposed: String,
    /// The repo the page is in, when not the one whose Review holds the card
    /// (a ticket in the team repo, proposed from a note in the wiki).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// A decisions-log entry to append, worked out against the log as it is
    /// when applied: rulings from one note then apply in any order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub append: Option<RulingEntry>,
    /// The ingested note this came from, so undoing the ingest withdraws it.
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

/// Pages a new repo changes, and what each should gain.
const ON_ADD: &[(&str, &str)] = &[
    ("Conventions/Architecture.md", "work the new repos into the repo table, what may call what, and the diagram"),
    ("Current/Who-Does-What.md", "add who owns each new repo and who to ask"),
    ("Current/Project.md", "say what the new repos add to the project, only if they change what it is or does"),
    (
        "Reference/Vocabulary.md",
        "add a row for each trap the new repos bring: a word they use for something the team already names differently, \
         a word with two meanings, a rename; never a word that means what it says",
    ),
];

pub fn update_prompt(page: &str, change: &str, existing: &str, sources: &[Source], today: &str) -> String {
    let mut s = format!(
        "You are proposing a change to one page of a team's wiki, `{page}`, because repos were added to the team. \
         A person keeps this page; they will see your change as a diff and apply it or not. The change: {change}.\n\n\
         Rules:\n\
         - Change only what the new repos change. Keep every other line exactly as it is, in the same order.\n\
         - Use only what the sources below say, and cite each new fact inline as its label in backticks.\n\
         - Set `updated: {today}` in the frontmatter; leave any `verified:` line as it is.\n\
         - If the new repos change nothing on this page, reply exactly NO CHANGE.\n\
         - Otherwise reply with the whole page, starting with `---`. No preamble, no code fences around it.\n\n\
         THE PAGE NOW:\n{existing}\n\nSOURCES (the new repos):\n"
    );
    for src in sources {
        s.push_str(&format!("\n=== {} ===\n{}\n", src.label, src.text));
    }
    s
}

/// The model's reply as a proposed page, or `None` for no change.
pub fn finish_update(reply: &str, existing: &str) -> Result<Option<String>> {
    let mut t = reply.trim();
    if t.eq_ignore_ascii_case("NO CHANGE") {
        return Ok(None);
    }
    if let Some(rest) = t.strip_prefix("```markdown").or_else(|| t.strip_prefix("```md")).or_else(|| t.strip_prefix("```")) {
        t = rest.trim_start().strip_suffix("```").unwrap_or(rest).trim();
    }
    if existing.starts_with("---") && !t.starts_with("---") {
        return Err(Error::Other("the proposed page lost its frontmatter".into()));
    }
    let page = format!("{t}\n");
    Ok((page.trim() != existing.trim()).then_some(page))
}

fn file_proposal(db: &mut Db, p: &Proposal, added: &[String], now: i64) -> Result<()> {
    let names = added.join(", ");
    let title = format!("Proposed: {} with {names}", p.page);
    let body = format!(
        "Ken read {names} and proposes this change to {}, a page a person keeps. Apply it to write it, or discard it. \
         It applies only while the page still reads as it did when Ken proposed it.",
        p.page
    );
    file_page_proposal(db, p, &title, &body, now)
}

/// Store one proposed page change, to apply or discard (on Team, or on its
/// ingest card when an ingest proposed it).
pub fn file_page_proposal(db: &mut Db, p: &Proposal, title: &str, body: &str, now: i64) -> Result<()> {
    let payload = serde_json::to_string(p).map_err(|e| Error::Other(e.to_string()))?;
    db.insert_review_item(PROPOSAL_KIND, title, body, &p.page, Some(&payload), now)?;
    Ok(())
}

/// Repos joined the team: draft their Repo Map pages, then for each page of
/// [`ON_ADD`] and the Repo Map index, draft it when it is still missing or a
/// template, or file a proposed change when a person keeps it. `all` is
/// every repo of the team now, the added ones included.
#[allow(clippy::too_many_arguments)]
pub fn draft_added(
    wiki: &Path,
    wiki_name: &str,
    db: &mut Db,
    added: &[(String, PathBuf)],
    all: &[(String, PathBuf)],
    today: &str,
    now: i64,
    mut generate: impl FnMut(&str) -> Result<String>,
) -> Result<DraftReport> {
    let mut report = DraftReport::default();
    let names: Vec<String> = added.iter().map(|(n, _)| n.clone()).collect();
    let check = PathCheck::of(all);
    // The roster from every repo of the team: the team repo may not be one
    // of those added.
    let roster = team_roster(wiki, all);
    draft_repo_pages(wiki, added, &roster, today, &mut report, &mut generate, &check)?;

    // The index: a new one lists every repo; a kept one gains a row per new repo.
    let index = format!("{REPO_MAP}/Index.md");
    let existing = fs::read_to_string(wiki.join(&index)).ok();
    match existing.as_deref().filter(|t| !may_draft(Some(t))) {
        None => {
            let listed: Vec<(String, String)> = all.iter().map(|(n, r)| (n.clone(), describe_repo(wiki, n, r))).collect();
            write_page(&wiki.join(&index), &repo_map_index(&listed, today))?;
            report.drafted.push(index);
        }
        Some(text) => {
            let mut proposed = text.trim_end().to_string();
            proposed.push('\n');
            let mut changed = false;
            for (n, r) in added {
                if !text.contains(&format!("[[{}]]", crate::workspace::member_leaf(n))) {
                    proposed.push_str(&index_row(n, &describe_repo(wiki, n, r)));
                    changed = true;
                }
            }
            if changed {
                let p = Proposal::new(index.clone(), text.to_string(), proposed);
                file_proposal(db, &p, &names, now)?;
                report.proposed.push(index);
            }
        }
    }

    let common = team_sources(wiki, wiki_name, added, &[]);
    for (page, change) in ON_ADD {
        let existing = fs::read_to_string(wiki.join(page)).ok();
        if may_draft(existing.as_deref()) {
            // Still missing or a template: draft it whole, from every repo.
            let purpose =
                PAGES.iter().find(|(p, _)| p == page).map(|(_, u)| u.to_string()).unwrap_or_else(|| change.to_string());
            let everything = team_sources(wiki, wiki_name, all, &[]);
            let pages = [(page.to_string(), purpose)];
            draft_team_pages(wiki, wiki_name, &pages, &everything, all, &roster, today, &mut report, &mut generate, &check)?;
            continue;
        }
        let mut sources = common.clone();
        sources.extend(page_sources(page, wiki, wiki_name, added, &roster));
        report.sources.extend(sources.iter().map(|s| s.label.clone()));
        let existing = existing.unwrap_or_default();
        match generate(&update_prompt(page, change, &existing, &sources, today)).and_then(|r| finish_update(&r, &existing)) {
            Ok(Some(proposed)) => {
                file_proposal(db, &Proposal::new(page.to_string(), existing, proposed), &names, now)?;
                report.proposed.push(page.to_string());
            }
            Ok(None) => report.kept.push(page.to_string()),
            Err(e) => report.failed.push((page.to_string(), e.to_string())),
        }
    }
    cite_rulings(wiki, all, today)?;
    dedup_labels(&mut report.sources);
    pin_drafted(wiki, &report.drafted, all)?;
    let title =
        format!("{} added to the wiki: {} drafted, {} proposed", names.join(", "), report.drafted.len(), report.proposed.len());
    file_card(db, &report, &title, now)?;
    Ok(report)
}

// --- Which repos a wiki covers: a wiki belongs to a team, and covers the
// team's repos. -----------------------------------------------------------

/// A workspace member as the wiki sees it: its name, folder, kinds and team.
#[derive(Debug, Clone, PartialEq)]
pub struct TeamMember {
    pub name: String,
    pub root: PathBuf,
    pub kind: Vec<crate::registry::RepoKind>,
    pub team: Option<String>,
}

impl TeamMember {
    fn is_wiki(&self) -> bool {
        self.kind.contains(&crate::registry::RepoKind::Wiki)
    }
}

/// The repos a wiki drafts from: the other members on its team, wikis left
/// out. A wiki with no team covers every member that has no team.
pub fn team_repos(wiki: &str, members: &[TeamMember]) -> Vec<(String, PathBuf)> {
    let Some(w) = members.iter().find(|m| m.name == wiki) else { return Vec::new() };
    members
        .iter()
        .filter(|m| m.name != wiki && !m.is_wiki() && m.team == w.team)
        .map(|m| (m.name.clone(), m.root.clone()))
        .collect()
}

/// The wiki that covers `member`: the wiki on its team (the first by name
/// when a team has two).
pub fn wiki_for<'a>(member: &str, members: &'a [TeamMember]) -> Option<&'a TeamMember> {
    let m = members.iter().find(|m| m.name == member)?;
    let mut wikis: Vec<&TeamMember> = members.iter().filter(|w| w.is_wiki() && w.name != member && w.team == m.team).collect();
    wikis.sort_by(|a, b| a.name.cmp(&b.name));
    wikis.into_iter().next()
}

// --- A repo removed from the team: the pages that still cite it. -----------

/// The wiki pages that cite `repo`: each page with the citations that name
/// it (its `sources:`), and the repo's own Repo Map page. Read from the index.
pub fn citing_pages(db: &Db, repo: &str) -> Result<Vec<(String, Vec<String>)>> {
    let leaf = crate::workspace::member_leaf(repo);
    let own = repo_page(repo);
    let mut out = Vec::new();
    for page in db.page_paths()?.into_iter().filter(|p| p.ends_with(".md")) {
        let Some(meta) = db.page_meta(&page)? else { continue };
        let cites: Vec<String> = meta
            .sources
            .iter()
            .filter(|s| {
                matches!(crate::drift::classify(s), crate::drift::Citation::Code(c) if c.repo.eq_ignore_ascii_case(leaf))
            })
            .map(|s| s.trim().trim_matches('"').to_string())
            .collect();
        if !cites.is_empty() || page.eq_ignore_ascii_case(&own) {
            out.push((page, cites));
        }
    }
    Ok(out)
}

/// Record that a repo left the team, when pages still cite it, so the Team
/// screen lists them for a person to rewrite or retire ([`findings`]).
/// Nothing is edited. Returns how many pages cite it; `None` when none do.
pub fn record_removed_repo(db: &mut Db, repo: &str) -> Result<Option<usize>> {
    let pages = citing_pages(db, repo)?;
    if pages.is_empty() {
        return Ok(None);
    }
    let mut repos = removed_repos(db);
    if !repos.iter().any(|r| r == repo) {
        repos.push(repo.to_string());
        db.store_removed_repos(&serde_json::to_string(&repos).map_err(|e| Error::Other(e.to_string()))?)?;
    }
    Ok(Some(pages.len()))
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
/// `wiki` is the repo whose Review holds the card; `p.root` wins when set.
pub fn apply(wiki: &Path, p: &Proposal) -> std::result::Result<(), ApplyError> {
    let root = p.root.as_deref().map(Path::new).unwrap_or(wiki);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gather_reads_readmes_docs_layout_and_an_extra_folder_within_budget() {
        let repo = tempfile::tempdir().unwrap();
        fs::write(repo.path().join("README.md"), "# Game\nA tactics game.\n").unwrap();
        fs::create_dir_all(repo.path().join("docs")).unwrap();
        fs::write(repo.path().join("docs/save.md"), "Saves are region files.\n").unwrap();
        fs::create_dir_all(repo.path().join("src/world")).unwrap();
        fs::create_dir_all(repo.path().join("node_modules/x")).unwrap();
        let extra = tempfile::tempdir().unwrap();
        fs::write(extra.path().join("Confluence-Home.txt"), "Our roadmap: ship in Q4.\n").unwrap();

        let s = gather(&[("game".into(), repo.path().to_path_buf())], Some(extra.path()));
        let labels: Vec<&str> = s.iter().map(|s| s.label.as_str()).collect();
        assert!(labels.contains(&"game:README.md"), "{labels:?}");
        assert!(labels.contains(&"game:docs/save.md"));
        let layout = s.iter().find(|s| s.label == "game:(layout)").unwrap();
        assert!(layout.text.contains("src/") && layout.text.contains("  world/") && !layout.text.contains("node_modules"));
        assert!(s.iter().any(|s| s.label.ends_with(":Confluence-Home.txt") && s.text.contains("Q4")));
    }

    #[test]
    fn a_draft_pins_its_code_citations_to_the_commit_it_read() {
        let heads: HashMap<String, String> = [("app".to_string(), "0a1b2c3d4e5f".to_string())].into();
        let page = "---\ntitle: Arch\nsources:\n  - app:README.md\n  - \"app:src/main.rs:12\"\n  - app:(layout)\n  - app@1234abcd:old.rs\n  - other:x.md\n  - \"[[Save]]\"\nlens: arch\n---\n# Arch\nSee `app:README.md`.\n";
        let pinned = pin_sources(page, &heads);
        assert_eq!(
            pinned,
            "---\ntitle: Arch\nsources:\n  - app@0a1b2c3d4e5f:README.md\n  - \"app@0a1b2c3d4e5f:src/main.rs:12\"\n  - app:(layout)\n  - app@1234abcd:old.rs\n  - other:x.md\n  - \"[[Save]]\"\nlens: arch\n---\n# Arch\nSee `app:README.md`.\n",
            "only sources: entries of known repos, and never the text"
        );
        assert_eq!(pin_sources(&pinned, &heads), pinned, "pinning twice changes nothing");
    }

    #[test]
    fn only_missing_or_untouched_template_pages_are_drafted() {
        assert!(may_draft(None));
        assert!(may_draft(Some("---\ntitle: \"Team\"\n---\n| {{role}} | {{name}} |\n")));
        assert!(!may_draft(Some("---\ntitle: Team\n---\nAna leads.\n")));
    }

    #[test]
    fn a_draft_is_marked_draft_and_never_verified() {
        let page = finish("```\n---\ntitle: Project\nstatus: current\nverified: 2026-09-24\nsources:\n  - game:README.md\n---\n# Project\n```").unwrap();
        assert!(page.contains("status: draft") && !page.contains("verified:") && page.contains("game:README.md"));
        assert!(finish("Here you go").is_err());
    }

    #[test]
    fn a_code_repo_gets_its_layers_page_and_a_reference_repo_a_dependency_page() {
        use crate::registry::RepoKind;
        let pages = |kinds: &[RepoKind]| kind_pages("team/Game", kinds).into_iter().map(|(p, b, _)| (p, b)).collect::<Vec<_>>();
        let arch = ("Conventions/Architecture-Game.md".to_string(), "Templates/Repo-Architecture.md");
        assert_eq!(pages(&[RepoKind::Code]), vec![arch]);
        assert_eq!(pages(&[RepoKind::Reference]), vec![("Platform/Game.md".to_string(), "Templates/Dependency.md")]);
        assert_eq!(pages(&[RepoKind::Code, RepoKind::Reference]).len(), 1, "a reference repo never gets an architecture page");
        assert!(pages(&[RepoKind::Team]).is_empty() && pages(&[]).is_empty());

        // The blank, its repo named, is the template while the page is missing.
        let wiki = tempfile::tempdir().unwrap();
        let blank = blank_for(wiki.path(), "Templates/Repo-Architecture.md", "team/Game");
        assert!(blank.contains("title: \"Game Architecture\"") && !blank.contains("{{Repo}}") && blank.contains("{{layer name}}"));
        let dep = blank_for(wiki.path(), "Templates/Dependency.md", "Engine");
        assert!(dep.contains("# Engine\n") && dep.contains("[[{{Repo}} Architecture]]"), "a code repo's placeholder stays");
        let sources = vec![Source { label: "Game:README.md".into(), text: "A game.".into() }];
        let mut report = DraftReport::default();
        let mut sent = String::new();
        let mut generate = |p: &str| {
            sent = p.to_string();
            Ok("---\ntitle: Game Architecture\n---\n# Game Architecture\n".to_string())
        };
        let page = "Conventions/Architecture-Game.md";
        draft_page(wiki.path(), page, "layers", Some(&blank), &sources, "2026-10-06", &mut report, &mut generate, &PathCheck::default())
            .unwrap();
        assert!(sent.contains("TEMPLATE (fill it in") && sent.contains("# Game Architecture"));
        assert!(!sent.contains("Drafted by Ken on"), "no byline");
        assert_eq!(report.drafted, vec![page]);
    }

    #[test]
    fn a_wiki_is_drafted_around_what_a_person_wrote() {
        let wiki = tempfile::tempdir().unwrap();
        fs::create_dir_all(wiki.path().join("Current")).unwrap();
        fs::write(wiki.path().join("Current/Team.md"), "---\ntitle: Team\n---\nAna leads; Ben reviews.\n").unwrap();
        fs::write(wiki.path().join("Current/Project.md"), "---\ntitle: \"Project\"\n---\n{{one paragraph}}\n").unwrap();
        let sources = vec![Source { label: "game:README.md".into(), text: "A tactics game.".into() }];
        let mut db = Db::open_in_memory().unwrap();
        let mut prompts: Vec<String> = Vec::new();
        let report = draft(wiki.path(), &mut db, &sources, "2026-09-24", 5, |p| {
            prompts.push(p.to_string());
            Ok("---\ntitle: Drafted\nsources:\n  - game:README.md\n---\n# Drafted\nDrafted by Ken.\n".into())
        })
        .unwrap();
        assert_eq!(report.kept, vec!["Current/Team.md"]);
        let others: Vec<&str> = PAGES.iter().map(|(p, _)| *p).filter(|p| *p != "Current/Team.md").collect();
        assert_eq!(report.drafted, others);
        assert!(prompts[0].contains("{{one paragraph}}"), "the template page's own text is the template");
        assert!(prompts.iter().all(|p| p.contains("=== game:README.md ===")));
        assert_eq!(fs::read_to_string(wiki.path().join("Current/Team.md")).unwrap(), "---\ntitle: Team\n---\nAna leads; Ben reviews.\n");
        assert!(fs::read_to_string(wiki.path().join("Conventions/Architecture.md")).unwrap().contains("status: draft"));
        let kept = last_draft(&db).unwrap();
        assert!(kept.title.starts_with("First wiki drafted") && kept.report.kept == vec!["Current/Team.md"]);
        assert!(db.open_review_item_of_kind(REVIEW_KIND).unwrap().is_none(), "no Review item");
        let found = findings(wiki.path(), &db).unwrap();
        assert!(found.iter().any(|f| f.kind == "draft" && f.path == "Conventions/Architecture.md"), "{found:?}");
    }

    #[test]
    fn the_draft_reads_which_folders_call_which_from_the_code() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for (p, t) in [
            ("app/api/users.py", "from app.services.users import find\n\ndef route():\n    return find()\n"),
            ("app/api/teams.py", "from app.services.users import find\nfrom app.services import teams\n"),
            ("app/services/users.py", "from app.db.repo import load\n\ndef find():\n    return load()\n"),
            ("app/services/teams.py", "def list_teams():\n    return []\n"),
            ("app/services/__init__.py", ""),
            ("app/db/repo.py", "from app.services.teams import list_teams\n\ndef load():\n    return 1\n"),
            ("README.md", "# App\n"),
        ] {
            fs::create_dir_all(root.join(p).parent().unwrap()).unwrap();
            fs::write(root.join(p), t).unwrap();
        }
        let git = |args: &[&str]| assert!(Command::new("git").args(args).current_dir(root).output().unwrap().status.success());
        git(&["init", "-q"]);
        git(&["add", "-A"]);
        let deps = folder_imports(root).expect("imports between folders");
        assert_eq!(
            deps,
            "app/api -> app/services (3)\napp/db -> app/services (1)\napp/services -> app/db (1)\n\n\
             import each other (a -> b / b -> a):\napp/db <-> app/services (1 / 1)\n\n\
             most imported files (how many files import each):\napp/services/users.py (2)"
        );
        let labels: Vec<String> = gather_repo("app", root, &[]).into_iter().map(|s| s.label).collect();
        assert!(labels.contains(&"app:(imports between folders, from the code)".to_string()), "{labels:?}");
    }

    #[test]
    fn release_notes_read_what_landed_since_the_newest_tag() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            let out = Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        };
        git(&["init", "-q"]);
        let commit = |msg: &str| {
            fs::write(root.join("log.txt"), msg).unwrap();
            git(&["add", "-A"]);
            git(&["commit", "-q", "-m", msg]);
        };
        commit("first cut");
        let (label, log) = git_unreleased(root).unwrap();
        assert_eq!(label, "(recent changes, no tags)");
        assert!(log.ends_with("first cut"));

        git(&["tag", "v1"]);
        commit("add the Dallas market");
        commit("fix the brief export");
        let (label, log) = git_unreleased(root).unwrap();
        assert_eq!(label, "(changes since v1)");
        let subjects: Vec<&str> = log.lines().map(|l| l.split_once(' ').unwrap().1).collect();
        assert_eq!(subjects, vec!["fix the brief export", "add the Dallas market"]);
    }

    #[test]
    fn what_the_draft_could_not_fill_is_named_for_a_person() {
        let wiki = tempfile::tempdir().unwrap();
        crate::wikinew::create(wiki.path(), "Payments", &[], "2026-09-28").unwrap();
        let left = placeholders_left(wiki.path());
        assert!(left.contains(&"START-HERE.md: {{what a newcomer loses before finding it}}".to_string()), "{left:?}");
        assert!(left.contains(&"CLAUDE.md: {{a trap that costs time on this team, and the page that holds it}}".to_string()));
        assert!(!left.iter().any(|l| l.contains("{{Domain}}")), "Ken names the Domain section itself");
        assert!(!left.iter().any(|l| l.starts_with("Templates/")), "{left:?}");
        // Every page's `updated:` is dated; a date in a table or a count is a person's.
        for (page, _) in crate::wikinew::TEMPLATE.iter().filter(|(p, _)| p.ends_with(".md") && !p.starts_with("Templates/")) {
            let text = fs::read_to_string(wiki.path().join(page)).unwrap();
            assert!(!text.lines().any(|l| l.starts_with("updated:") && l.contains("{{")), "{page}");
        }
    }

    fn page(title: &str) -> String {
        format!("---\ntitle: {title}\nsources:\n  - x\n---\n# {title}\nDrafted by Ken.\n")
    }

    fn repo(dir: &Path, name: &str, readme: &str) -> (String, PathBuf) {
        let root = dir.join(name);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("README.md"), readme).unwrap();
        (name.to_string(), root)
    }

    #[test]
    fn a_team_wiki_is_drafted_repo_by_repo_then_from_the_repo_pages() {
        let d = tempfile::tempdir().unwrap();
        let wiki = d.path().join("Wiki");
        fs::create_dir_all(&wiki).unwrap();
        let repos = vec![repo(d.path(), "Game", "# Game\nThe client.\n"), repo(d.path(), "Tools", "# Tools\nThe editor.\n")];
        let mut db = Db::open_in_memory().unwrap();
        let mut prompts: Vec<String> = Vec::new();
        let report = draft_team(&wiki, "Wiki", &mut db, &repos, None, "2026-09-25", 5, |p| {
            prompts.push(p.to_string());
            Ok(page("Drafted"))
        })
        .unwrap();
        assert_eq!(&report.drafted[..3], &["Repo-Map/Game.md", "Repo-Map/Tools.md", "Repo-Map/Index.md"]);
        assert!(report.drafted.contains(&"Conventions/Architecture.md".to_string()));
        // Pass one reads one repo each.
        assert!(prompts[0].contains("=== Game:README.md ===") && !prompts[0].contains("Tools:README.md"));
        assert!(prompts[1].contains("=== Tools:README.md ===") && !prompts[1].contains("Game:README.md"));
        // Pass two reads the repo pages, and of the raw repos only the brief
        // sections that serve the page: the README's opening says what the
        // project is, so Project gets it and the architecture page does not.
        let team = &prompts[2];
        assert!(team.contains("=== Wiki:Repo-Map/Game.md ===") && team.contains("=== Wiki:Repo-Map/Tools.md ==="), "{team}");
        let arch = prompts.iter().find(|p| p.contains("`Conventions/Architecture.md`")).unwrap();
        assert!(team.contains("=== Game:README.md ===") && !arch.contains("=== Game:README.md ==="));
        let index = fs::read_to_string(wiki.join("Repo-Map/Index.md")).unwrap();
        assert!(index.contains("| [[Game]] | (not said yet) |") && index.contains("| [[Tools]] |"));
    }

    #[test]
    fn every_repo_gets_its_own_budget_however_many_there_are() {
        let d = tempfile::tempdir().unwrap();
        let big = "word ".repeat(40_000);
        let repos: Vec<(String, PathBuf)> = (0..20).map(|i| repo(d.path(), &format!("r{i}"), &big)).collect();
        let s = gather(&repos, None);
        assert!(s.iter().any(|s| s.label == "r19:README.md"), "the last repo is still read");
    }

    #[test]
    fn an_added_repo_gets_its_page_and_kept_pages_get_proposals() {
        let d = tempfile::tempdir().unwrap();
        let wiki = d.path().join("Wiki");
        fs::create_dir_all(wiki.join("Repo-Map")).unwrap();
        fs::create_dir_all(wiki.join("Current")).unwrap();
        fs::create_dir_all(wiki.join("Conventions")).unwrap();
        let index = "---\ntitle: Repo Map\n---\n| repo | what it is for |\n|---|---|\n| [[Game]] | The client. |\n";
        fs::write(wiki.join("Repo-Map/Index.md"), index).unwrap();
        let arch = "---\ntitle: Architecture\n---\nGame calls the server.\n";
        fs::write(wiki.join("Conventions/Architecture.md"), arch).unwrap();
        fs::write(wiki.join("Current/Who-Does-What.md"), "---\ntitle: Who\n---\nAna owns Game.\n").unwrap();
        fs::write(wiki.join("Current/Project.md"), "---\ntitle: Project\n---\nA game.\n").unwrap();
        let game = repo(d.path(), "Game", "# Game\n");
        let tools = repo(d.path(), "Tools", "# Tools\nThe level editor.\n");
        let mut db = Db::open_in_memory().unwrap();
        let report = draft_added(&wiki, "Wiki", &mut db, &[tools.clone()], &[game, tools], "2026-09-25", 9, |p| {
            Ok(if p.contains("`Repo-Map/Tools.md`") {
                page("Tools")
            } else if p.contains("`Conventions/Architecture.md`") {
                "---\ntitle: Architecture\n---\nGame calls the server. Tools writes levels the Game loads.\n".into()
            } else if p.contains("`Reference/Vocabulary.md`") {
                page("Vocabulary")
            } else {
                "NO CHANGE".into()
            })
        })
        .unwrap();
        assert!(report.drafted.contains(&"Repo-Map/Tools.md".to_string()), "{report:?}");
        assert!(report.drafted.contains(&"Reference/Vocabulary.md".to_string()), "a missing page is drafted whole");
        assert_eq!(report.proposed, vec!["Repo-Map/Index.md", "Conventions/Architecture.md"]);
        assert_eq!(report.kept, vec!["Current/Who-Does-What.md", "Current/Project.md"]);
        assert_eq!(fs::read_to_string(wiki.join("Conventions/Architecture.md")).unwrap(), arch, "a kept page is never written");

        let items = db.list_open_review_items().unwrap();
        let mut props: Vec<Proposal> = items
            .iter()
            .filter(|i| i.kind == PROPOSAL_KIND)
            .map(|i| serde_json::from_str(i.payload.as_deref().unwrap()).unwrap())
            .collect();
        props.sort_by(|a, b| b.page.cmp(&a.page)); // Repo-Map/Index.md, then Conventions/…
        assert_eq!(props.len(), 2);
        assert!(props[0].proposed.ends_with("| [[Game]] | The client. |\n| [[Tools]] | (not said yet) |\n"));

        // Applied while the page reads as it did; refused once it changed.
        apply(&wiki, &props[1]).unwrap();
        assert!(fs::read_to_string(wiki.join("Conventions/Architecture.md")).unwrap().contains("Tools writes levels"));
        fs::write(wiki.join("Repo-Map/Index.md"), format!("{index}| [[Server]] | added by hand |\n")).unwrap();
        assert_eq!(apply(&wiki, &props[0]), Err(ApplyError::Changed));
    }

    #[test]
    fn an_update_reply_of_no_change_or_the_same_page_proposes_nothing() {
        let page = "---\ntitle: A\n---\nText.\n";
        assert_eq!(finish_update("NO CHANGE", page).unwrap(), None);
        assert_eq!(finish_update(page, page).unwrap(), None);
        assert!(finish_update("Sure, here it is", page).is_err());
        assert!(finish_update("---\ntitle: A\n---\nText and more.", page).unwrap().is_some());
    }
    #[test]
    fn a_removed_repo_lists_the_pages_that_cite_it() {
        let d = tempfile::tempdir().unwrap();
        let wiki = d.path().join("Wiki");
        fs::create_dir_all(wiki.join("Platform")).unwrap();
        fs::create_dir_all(wiki.join("Repo-Map")).unwrap();
        fs::write(wiki.join("Platform/Save.md"), "---\nsources:\n  - Tools:src/save.rs:3\n  - Game:src/x.rs\n---\n# Save\n").unwrap();
        fs::write(wiki.join("Platform/Combat.md"), "---\nsources:\n  - Game:src/hit.rs\n---\n# Combat\n").unwrap();
        fs::write(wiki.join("Repo-Map/Tools.md"), "---\ntitle: Tools\n---\n# Tools\n").unwrap();
        let project = crate::project::Project::create(&wiki, "Wiki").unwrap();
        let mut db = Db::open_in_memory().unwrap();
        crate::scan::scan(&project, &mut db).unwrap();

        let mut pages = citing_pages(&db, "Tools").unwrap();
        pages.sort();
        assert_eq!(
            pages,
            vec![
                ("Platform/Save.md".to_string(), vec!["Tools:src/save.rs:3".to_string()]),
                ("Repo-Map/Tools.md".to_string(), vec![]),
            ]
        );
        assert_eq!(record_removed_repo(&mut db, "Tools").unwrap(), Some(2));
        assert_eq!(record_removed_repo(&mut db, "Tools").unwrap(), Some(2));
        assert_eq!(removed_repos(&db), vec!["Tools"]);
        let found = findings(&wiki, &db).unwrap();
        let gone = found.iter().find(|f| f.kind == "repo-removed").unwrap();
        assert!(gone.title.starts_with("Tools left the team: 2 pages") && gone.detail.contains("Platform/Save.md"), "{gone:?}");
        assert_eq!(record_removed_repo(&mut db, "Nobody").unwrap(), None);
    }

    #[test]
    fn a_wiki_covers_its_teams_repos_and_a_repo_finds_its_teams_wiki() {
        use crate::registry::RepoKind;
        let m = |name: &str, kind: RepoKind, team: Option<&str>| TeamMember {
            name: name.into(),
            root: PathBuf::from(name),
            kind: vec![kind],
            team: team.map(String::from),
        };
        let members = vec![
            m("Realms-Wiki", RepoKind::Wiki, Some("Realms")),
            m("Game", RepoKind::Code, Some("Realms")),
            m("Server", RepoKind::Code, Some("Realms")),
            m("Ops-Wiki", RepoKind::Wiki, Some("Ops")),
            m("Infra", RepoKind::Code, Some("Ops")),
            m("Scratch", RepoKind::Code, None),
        ];
        let names = |v: Vec<(String, PathBuf)>| v.into_iter().map(|(n, _)| n).collect::<Vec<_>>();
        assert_eq!(names(team_repos("Realms-Wiki", &members)), vec!["Game", "Server"]);
        assert_eq!(names(team_repos("Ops-Wiki", &members)), vec!["Infra"]);
        assert_eq!(wiki_for("Server", &members).map(|w| w.name.as_str()), Some("Realms-Wiki"));
        assert_eq!(wiki_for("Scratch", &members), None, "no team wiki, no update");
    }

    /// Run git in `root` as a test identity; panics when git fails.
    fn git_in(root: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn write(root: &Path, files: &[(&str, &str)]) {
        for (p, t) in files {
            fs::create_dir_all(root.join(p).parent().unwrap()).unwrap();
            fs::write(root.join(p), t).unwrap();
        }
    }

    /// Item 1: a brief is read by section, and each page gets the sections
    /// that serve it, however deep in the brief they are.
    #[test]
    fn a_brief_is_read_by_section_and_each_page_gets_its_sections() {
        let filler = "Weapons are swords and bows.\n".repeat(400); // ~11,600 characters
        let claude = format!(
            "# Game\n\nA tactics game for Hytale.\n\n## Weapon Types\n\n{filler}\n## Build & Run\n\n```\n## not a heading\n./gradlew build\n```\n\n\
             ## Key Architectural Patterns\n\nSystems may call services; services never call systems.\n\n\
             ### Wiring order\n\nInstallers run in order.\n\n## Common Gotchas\n\nEnergy is called Stamina in the engine.\n\n\
             ## Team\n\nAna decides balance.\n"
        );
        let sections = brief_sections(&claude);
        let headings: Vec<&str> = sections.iter().map(|s| s.heading.as_str()).collect();
        assert_eq!(headings, vec!["", "Weapon Types", "Build & Run", "Key Architectural Patterns", "Common Gotchas", "Team"]);
        assert!(sections[3].text.contains("### Wiring order"), "a ### stays inside its section");
        assert!(sections[2].text.contains("## not a heading"), "never split inside a code fence");
        let topics: Vec<Topic> = headings.iter().map(|h| topic_of(h)).collect();
        assert_eq!(
            topics,
            vec![Topic::Project, Topic::Other, Topic::Build, Topic::Architecture, Topic::Vocabulary, Topic::People]
        );
        assert_eq!(topic_of("Project Structure"), Topic::Architecture);
        assert_eq!(topic_of("Who decides"), Topic::People);
        assert_eq!(topic_of("Wholesale prices"), Topic::Other, "a whole word, not a prefix of one");

        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("Game");
        write(&root, &[("CLAUDE.md", &claude)]);
        let line_of = |h: &str| claude.lines().position(|l| l == h).unwrap() + 1;
        let arch = format!("Game:CLAUDE.md:{}", line_of("## Key Architectural Patterns"));

        // The repo page: building, structure and what it is first, then the rest.
        let labels: Vec<String> = gather_repo("Game", &root, &[]).into_iter().map(|s| s.label).collect();
        let at = |l: &str| labels.iter().position(|x| x == l).unwrap_or_else(|| panic!("{l} in {labels:?}"));
        let build = format!("Game:CLAUDE.md:{}", line_of("## Build & Run"));
        let weapons = format!("Game:CLAUDE.md:{}", line_of("## Weapon Types"));
        assert!(at(&build) < at(&weapons) && at(&arch) < at(&weapons), "{labels:?}");
        assert!(labels.contains(&"Game:(headings of its briefs, with their lines)".to_string()));

        // A team page: only what serves it.
        let wiki = d.path().join("Wiki");
        let repos = vec![("Game".to_string(), root.clone())];
        let of = |page: &str| -> Vec<Source> { page_sources(page, &wiki, "Wiki", &repos, &[]) };
        let architecture = of("Conventions/Architecture.md");
        assert!(architecture.iter().any(|s| s.label == arch && s.text.contains("services never call systems")), "{architecture:?}");
        assert!(!architecture.iter().any(|s| s.text.contains("Ana decides")));
        assert!(of("Reference/Vocabulary.md").iter().any(|s| s.text.contains("called Stamina")));
        assert!(of("Current/Team.md").iter().any(|s| s.text.contains("Ana decides")));
        assert!(of("Current/Project.md").iter().any(|s| s.label == "Game:CLAUDE.md" && s.text.contains("tactics game")));

        // A repo that needs less than an equal share leaves the rest to the others.
        assert_eq!(fair_shares(&[40_000, 1_000, 8_000, 500], 30_000), vec![20_500, 1_000, 8_000, 500]);
        assert_eq!(fair_shares(&[40_000, 40_000], 30_000), vec![15_000, 15_000]);
    }

    /// Item 2: the build files are read, and a README once whatever its case.
    #[test]
    fn the_build_files_are_read_and_a_readme_once() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let gradle = format!("plugins {{ java }}\n{}tasks.register(\"runServer\") {{\n}}\n", "// setting\n".repeat(80));
        write(
            root,
            &[
                ("README.md", "# Tools\nThe authoring suite.\n"),
                ("package.json", r#"{"name": "sr-tools", "scripts": {"start": "pnpm --filter shell tauri dev", "test": "vitest"}}"#),
                ("build.gradle.kts", &gradle),
                ("settings.gradle.kts", "rootProject.name = \"mod\"\n"),
                ("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n\n[dependencies]\nserde = \"1\"\n\n[[bin]]\nname = \"sr-jar\"\n"),
                ("pyproject.toml", "[project]\nname = \"x\"\n\n[project.scripts]\nsr-log = \"tools.log:main\"\n"),
                ("Makefile", "CC = cc\n# build it\nbuild: deps\n\tcargo build --release\n\ntest:\n\tcargo test\n.PHONY: build\n"),
                (".ken/gates.json", "{\"gates\": {\"G2\": {\"tests\": \"./gradlew build --offline\"}}}"),
                ("INSTALL.md", "# Install\nRun tool.cmd.\n"),
            ],
        );
        assert_eq!(top_files(root, &["README.md", "readme.md", "Readme.md"]).len(), 1, "one file, read once");
        let s = gather_repo("tools", root, &[]);
        let text = |label: &str| s.iter().find(|x| x.label == label).map(|x| x.text.clone()).unwrap_or_else(|| panic!("{label}"));
        assert_eq!(s.iter().filter(|x| x.label.to_lowercase().starts_with("tools:readme")).count(), 1);
        assert!(text("tools:package.json").contains("start: pnpm --filter shell tauri dev"));
        let g = text("tools:build.gradle.kts");
        assert!(g.contains("plugins { java }") && g.contains("82: tasks.register(\"runServer\")"), "{g}");
        assert!(text("tools:settings.gradle.kts").contains("rootProject.name"));
        let cargo = text("tools:Cargo.toml");
        assert!(cargo.contains("[workspace]") && cargo.contains("[[bin]]") && !cargo.contains("serde"), "{cargo}");
        let py = text("tools:pyproject.toml");
        assert!(py.contains("sr-log") && !py.contains("name = \"x\""));
        let make = text("tools:Makefile");
        assert!(make.contains("build: deps\n\tcargo build --release\n") && make.contains("test:") && !make.contains("CC = cc"), "{make}");
        assert!(text("tools:.ken/gates.json").contains("--offline"));
        assert!(text("tools:INSTALL.md").contains("tool.cmd"));
        assert!(text("tools:(layout)").contains(".ken/"), "the gates folder is in the layout");
    }

    /// Item 3: the paths a page names are checked against what git tracks,
    /// and the page goes back once, with where the code has each now.
    #[test]
    fn paths_a_page_names_are_checked_and_corrected_once() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("game");
        write(
            &root,
            &[
                ("README.md", "# Game\n"),
                (".gitignore", "build/\n"),
                ("src/main/java/dev/app/services/combat/CombatCalculator.java", "class CombatCalculator {}\n"),
                ("src/main/java/dev/app/utils/SrLog.java", "class SrLog {}\n"),
                ("src/main/resources/data/equipment/WeaponBases.json", "{}\n"),
            ],
        );
        git_in(&root, &["init", "-q"]);
        git_in(&root, &["add", "-A"]);
        let check = PathCheck::of(&[("game".to_string(), root.clone())]);
        let page = "---\ntitle: Game\nsources:\n  - game:README.md\n  - game:docs/OLD.md\n  - game:(layout)\n---\n# Game\n\
            Damage maths is `utils/CombatCalculator.java` (`utils/CombatCalculator`); configs are `data/*.json` and `data/equipment/`.\n\
            The calculator is `services/combat/CombatCalculator.java`, the class `services/combat/CombatCalculator` \
            (`game:README.md:3`). The repo is `Stingbro/game` on branch `claude/brave-otter`; the jar is \
            `build/libs/game.jar`; `gone/Nothing.java` was removed; see `src/.../SrLog.java`.\n\
            ```\nsrc/never/Checked.java\n```\n";
        let missing = check.missing(page, Some("game"));
        let paths: Vec<&str> = missing.iter().map(|m| m.path.as_str()).collect();
        assert_eq!(paths, vec!["docs/OLD.md", "utils/CombatCalculator.java", "utils/CombatCalculator", "data/*.json", "gone/Nothing.java"]);
        assert_eq!(missing[0].repo.as_deref(), Some("game"));
        let calculator = "game:src/main/java/dev/app/services/combat/CombatCalculator.java";
        assert_eq!(missing[1].now, vec![calculator]);
        assert_eq!(missing[2].now, vec![calculator], "a class named without its extension");
        assert_eq!(missing[3].now, vec!["game:src/main/resources/data/"], "a pattern's folder");
        assert!(missing[4].now.is_empty());

        let calls: std::cell::RefCell<Vec<String>> = Default::default();
        let mut generate = |p: &str| -> Result<String> {
            calls.borrow_mut().push(p.to_string());
            Ok("---\ntitle: Game\n---\n# Game\nStill names `gone/Nothing.java`.\n".into())
        };
        let mut report = DraftReport::default();
        let fixed = check_paths("Repo-Map/game.md", finish(page).unwrap(), &check, &mut report, &mut generate);
        let first = calls.borrow()[0].clone();
        assert_eq!(calls.borrow().len(), 1, "one correction call, never a loop");
        assert!(first.contains("- `utils/CombatCalculator.java`: a file or folder of that name is at `game:src/main/java/dev/app/services/combat/CombatCalculator.java`"), "{first}");
        assert!(first.contains("- `gone/Nothing.java`: no repo of the team has a file or folder of that name"));
        assert!(fixed.contains("status: draft") && fixed.contains("Still names"));
        assert_eq!(report.corrected, vec!["Repo-Map/game.md: 5 paths"]);
        let clean = "---\ntitle: Game\n---\nSee `src/main/resources/data/equipment/WeaponBases.json`.\n".to_string();
        assert_eq!(check_paths("x.md", clean.clone(), &check, &mut report, &mut generate), clean);
        assert_eq!(calls.borrow().len(), 1, "a clean page costs no call");
    }

    /// Item 4: the import map names folders below the source root, so a
    /// Java package tree is not one folder, and a cycle shows.
    #[test]
    fn the_import_map_names_folders_from_the_source_root() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let b = "src/main/java/dev/app";
        write(
            root,
            &[
                (&format!("{b}/App.java"), "package dev.app;\nimport dev.app.systems.combat.Hit;\nclass App {}\n"),
                (&format!("{b}/services/combat/Calc.java"), "package dev.app.services.combat;\nimport dev.app.systems.combat.Hit;\nclass Calc {}\n"),
                (&format!("{b}/systems/combat/Hit.java"), "package dev.app.systems.combat;\nimport dev.app.services.combat.Calc;\nclass Hit {}\n"),
                (&format!("{b}/systems/spawn/Spawn.java"), "package dev.app.systems.spawn;\nimport dev.app.services.loot.Loot;\nclass Spawn {}\n"),
                (&format!("{b}/services/loot/Loot.java"), "package dev.app.services.loot;\nclass Loot {}\n"),
            ],
        );
        git_in(root, &["init", "-q"]);
        git_in(root, &["add", "-A"]);
        let map = folder_imports(root).expect("an import map");
        assert!(map.contains(&format!("{b}/services/combat -> {b}/systems/combat (1)")), "{map}");
        assert!(map.contains(&format!("{b} -> {b}/systems/combat (1)")), "a file at the root is its own folder: {map}");
        assert!(map.contains(&format!("{b}/services <-> {b}/systems (1 / 2)")), "one level down: {map}");
        assert!(map.contains(&format!("{b}/services/combat <-> {b}/systems/combat (1 / 1)")), "two levels down: {map}");
        assert!(!map.contains("src/main/java/dev ->"), "never one folder for the whole tree: {map}");
    }

    /// Item 5: only version tags count, all of them, and a changelog is split
    /// by version.
    #[test]
    fn releases_read_version_tags_only_and_each_versions_notes() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        git_in(root, &["init", "-q"]);
        let commit = |msg: &str| {
            fs::write(root.join("log.txt"), msg).unwrap();
            git_in(root, &["add", "-A"]);
            git_in(root, &["commit", "-q", "-m", msg]);
        };
        commit("first cut");
        for i in 1..42 {
            git_in(root, &["tag", &format!("v0.0.{i}")]);
        }
        commit("ship 0.0.42");
        git_in(root, &["tag", "v0.0.42"]);
        commit("add duels");
        git_in(root, &["tag", "archive/ui-faction-2026-09-30"]);
        commit("fix the crossbow");
        let (label, log) = git_unreleased(root).unwrap();
        assert_eq!(label, "(changes since v0.0.42)", "the newest version tag, never the newer archive tag");
        assert!(log.contains("add duels") && log.contains("fix the crossbow") && !log.contains("ship 0.0.42"));
        let tags = git_tags(root).unwrap();
        assert_eq!(tags.lines().count(), 42, "every version tag");
        assert!(!tags.contains("archive/"));

        let changelog = "# Changelog\n\nIntro.\n\n## [Unreleased]\n\n- duels\n\n## 0.0.15 — 2026-09-25\n\n### Added\n\n- realms\n\n## v0.0.14\n\n- polygon zones\n";
        let v = version_sections(changelog);
        let heads: Vec<&str> = v.iter().map(|s| s.heading.as_str()).collect();
        assert_eq!(heads, vec!["[Unreleased]", "0.0.15 — 2026-09-25", "v0.0.14"]);
        assert!(v[1].text.contains("### Added") && v[1].text.contains("realms") && !v[1].text.contains("polygon"));
        write(root, &[("CHANGELOG.md", changelog)]);
        let notes = page_sources("Work/Releases.md", root, "Wiki", &[("game".to_string(), root.to_path_buf())], &[]);
        let labels: Vec<&str> = notes.iter().map(|s| s.label.as_str()).collect();
        assert!(labels.contains(&"game:CHANGELOG.md:5") && labels.contains(&"game:CHANGELOG.md:9") && labels.contains(&"game:CHANGELOG.md:15"), "{labels:?}");
        assert!(labels.contains(&"game:(version tags, newest first)") && labels.contains(&"game:(changes since v0.0.42)"));
    }

    /// Item 6: the layout shows the dot-folders that matter, every entry of a
    /// big folder, and folders further down.
    #[test]
    fn the_layout_shows_meaningful_dot_folders_and_deeper_data() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let mut files: Vec<(String, String)> = (0..20).map(|i| (format!("packages/p{i:02}/package.json"), "{}".to_string())).collect();
        files.push((".ken/gates.json".into(), "{}".into()));
        files.push((".github/workflows/ci.yml".into(), "on: push\n".into()));
        files.push((".claude/settings.json".into(), "{}".into()));
        files.push(("src/main/resources/data/mobs/t1/Skeleton.json".into(), "{}".into()));
        let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        write(root, &refs);
        git_in(root, &["init", "-q"]);
        git_in(root, &["add", "-A"]);
        let text = layout(root, &ls_files(root).unwrap());
        assert!(text.contains(".ken/\n  gates.json") && text.contains(".github/\n  workflows/"), "{text}");
        assert!(!text.contains(".claude"), "{text}");
        assert!(text.contains("  p19/"), "no 15-entry cap: {text}");
        assert!(text.contains("src/main/resources/data/mobs/\n") && text.contains("src/main/resources/data/mobs/t1/\n"), "{text}");
    }

    /// Item 7: git adds only recent hands, by email, without bots, in UTF-8.
    #[test]
    fn recent_contributors_are_people_by_email_without_bots() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        git_in(root, &["init", "-q"]);
        let mut n = 0;
        let mut commit = |who: &str, file: &str| {
            n += 1;
            write(root, &[(file, &n.to_string())]);
            git_in(root, &["add", "-A"]);
            git_in(root, &["commit", "-q", "--author", who, "-m", &format!("change {n}")]);
        };
        commit("Ádám Liszkai <adam@example.com>", "data/spawns/a.json");
        commit("Ádám Liszkai <adam@example.com>", "data/spawns/b.json");
        commit("Chris <14892384+Stingbro@users.noreply.github.com>", "data/spawns/c.json");
        commit("Stingbro <99+stingbro@users.noreply.github.com>", "src/x.rs");
        commit("Chris <14892384+Stingbro@users.noreply.github.com>", "src/y.rs");
        commit("dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>", "src/z.rs");
        commit("Claude <noreply@anthropic.com>", "src/w.rs");
        commit("Hytale Sync Bot <sync@hytale.example>", "src/v.rs");
        commit("Cursor Agent <cursoragent@cursor.com>", "README.md");
        let rows = recent_contributors(root, &[]).unwrap();
        assert!(rows.contains("data/: Ádám Liszkai (2), Chris (1)"), "UTF-8 names, most first: {rows}");
        assert!(rows.contains("src/: Chris (2)\n"), "one person per GitHub id, under the name used most: {rows}");
        assert!(!rows.contains("(top-level files)"), "a bot's only folder has no row: {rows}");
        for bot in ["dependabot", "Claude", "Sync Bot", "Cursor"] {
            assert!(!rows.contains(bot), "{bot}: {rows}");
        }
        assert!(is_bot("github-actions[bot]", "41898282+github-actions[bot]@users.noreply.github.com"));
        assert!(is_bot("Copilot", "x@y") && !is_bot("Abbot Smith", "abbot@example.com"));

        let p = prompt("Current/Team.md", "the team", None, &[], "2026-10-06");
        assert!(p.contains("Never write a role nobody stated") && p.contains("Never speculate whether two names are one person"));
        write(root, &[("people/ana.md", "# Ana\nDecides balance.\n"), ("people/PERSON.md", "{{name}}\n"), ("CODEOWNERS", "data/ @ana\n")]);
        let s = page_sources("Current/Who-Does-What.md", root, "Wiki", &[("game".to_string(), root.to_path_buf())], &[]);
        let labels: Vec<&str> = s.iter().map(|x| x.label.as_str()).collect();
        assert!(labels.contains(&"game:CODEOWNERS") && labels.contains(&"game:people/ana.md"), "{labels:?}");
        assert!(labels.contains(&"game:(recent contributors by folder, last 90 days, from git)"));
        assert!(!labels.iter().any(|l| l.ends_with("PERSON.md")), "the template to copy is not a person");
    }

    /// One person who commits as three identities is counted once, under the
    /// name the roster gives them, and a roster person is never a bot.
    #[test]
    fn recent_contributors_count_a_roster_person_once() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        git_in(root, &["init", "-q"]);
        let mut n = 0;
        let mut commit = |who: &str, file: &str| {
            n += 1;
            write(root, &[(file, &n.to_string())]);
            git_in(root, &["add", "-A"]);
            git_in(root, &["commit", "-q", "--author", who, "-m", &format!("change {n}")]);
        };
        commit("Chris <14892384+Stingbro@users.noreply.github.com>", "src/a.rs");
        commit("Stingbro <nonameisavalibleatthemomment@gmail.com>", "src/b.rs");
        commit("AlpahSignalAI <alpha.signal.ai@gmail.com>", "src/c.rs");
        commit("Ana Ruiz <ana@example.com>", "src/d.rs");
        commit("Mabel Bot <mabel@example.com>", "docs/e.md");
        commit("Hytale Sync Bot <sync@hytale.example>", "docs/f.md");

        // No roster: as before, one person per address.
        let rows = recent_contributors(root, &[]).unwrap();
        assert!(rows.contains("src/: AlpahSignalAI (1), Ana Ruiz (1), Chris (1)"), "{rows}");
        assert!(!rows.contains("docs/"), "{rows}");

        write(
            root,
            &[
                (
                    "people/chris.md",
                    "---
id: chris
name: Chris Lee
emails: [14892384+Stingbro@users.noreply.github.com, nonameisavalibleatthemomment@gmail.com, alpha.signal.ai@gmail.com]
aliases: [Stingbro, AlpahSignalAI]
---
",
                ),
                ("people/mabel.md", "---
id: mabel
name: Mabel Bot
---
"),
            ],
        );
        let roster = crate::people::roster(root);
        assert_eq!(roster.len(), 2);
        let rows = recent_contributors(root, &roster).unwrap();
        assert!(rows.contains("src/: Chris Lee (3), Ana Ruiz (1)
"), "three identities, one person: {rows}");
        assert!(!rows.contains("AlpahSignalAI") && !rows.contains("Stingbro"), "{rows}");
        assert!(rows.contains("docs/: Mabel Bot (1)
"), "the roster beats the bot list: {rows}");
        assert!(!rows.contains("Sync Bot"), "a bot not on the roster stays out: {rows}");
    }

    /// Item 8: the index and START-HERE say what a repo is for once its page
    /// does, where set-up had nothing.
    #[test]
    fn a_repo_with_no_description_takes_its_purpose_from_its_page() {
        let tools_page = "---\ntitle: Tools\nsources:\n  - Tools:CLAUDE.md\n---\n\n# Tools\n\nDrafted by Ken on 2026-10-06 from the sources listed; not yet verified by a person.\n\n\
            ## What it is for\n\nIn the team's own words, this repo is \"the authoring suite\": the desktop tools the team uses. \
            It is a pnpm workspace. `Tools:CLAUDE.md`\n";
        assert_eq!(
            page_purpose(tools_page).as_deref(),
            Some("In the team's own words, this repo is \"the authoring suite\": the desktop tools the team uses.")
        );
        assert_eq!(page_purpose("---\ntitle: X\n---\n# X\nDrafted by Ken.\n"), None);

        let d = tempfile::tempdir().unwrap();
        let wiki = d.path().join("Wiki");
        let covered = vec![crate::wikinew::Covered { name: "Tools".into(), ..Default::default() }];
        crate::wikinew::create(&wiki, "Realms", &covered, "2026-10-06").unwrap();
        assert!(fs::read_to_string(wiki.join("START-HERE.md")).unwrap().contains("| `Tools` | (not said yet) |"));
        let repos = vec![repo(d.path(), "Tools", "# Tools\n")];
        let mut db = Db::open_in_memory().unwrap();
        draft_team(&wiki, "Wiki", &mut db, &repos, None, "2026-10-06", 5, |p| {
            Ok(if p.contains("`Repo-Map/Tools.md`") { tools_page.to_string() } else { page("Drafted") })
        })
        .unwrap();
        let index = fs::read_to_string(wiki.join("Repo-Map/Index.md")).unwrap();
        assert!(index.contains("| [[Tools]] | In the team's own words, this repo is \"the authoring suite\""), "{index}");
        let start = fs::read_to_string(wiki.join("START-HERE.md")).unwrap();
        assert!(start.contains("| `Tools` | In the team's own words") && !start.contains("(not said yet)"), "{start}");
    }
}
