//! An end-to-end evaluation run on a real folder of repos, through the same
//! ken-core calls the app makes: set up a team with a new wiki, index,
//! embed, extract entities, draft the first wiki with Claude, build the
//! workspace graph, ask curated questions, change a file and check drift,
//! and exercise `.kenignore`. One phase per run, so each can be checked.
//!
//! ```text
//! set KEN_DATA_DIR=C:\ken-eval\data      (required; never the app's own data)
//! set KEN_EVAL_TEAM=Payments             (the team; its new wiki is <team>-Wiki)
//! set KEN_EVAL_EXTRA=Notes               (optional: a notes folder for the draft)
//! cargo run --release -p ken-core --example eval_run -- <phase> <parent> [args]
//! ```
//!
//! Phases: setup · index · embed · wiki · extract [minutes] · kg · ask
//! <questions.tsv> · drift · drift-change <member> <file> · ignore <member>
//! · model <member> · sql <member> <query> · status. Each prints a Markdown
//! report on stdout.
//!
//! It refuses to run without `KEN_DATA_DIR`, so it can never write into the
//! app's own data.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ken_core::db::Db;
use ken_core::federation::{self, FederationLlm};
use ken_core::local_llm::{self, Priority};
use ken_core::project::Project;
use ken_core::registry::{IndexState, Registry};
use ken_core::routing::{self, MemberDbHandle, MemberInfo};
use ken_core::runner::CancelToken;
use ken_core::workspace::{MemberStatus, Workspace};
use ken_core::workspace_kg_db::WorkspaceKgDb;
use ken_core::{assistant, drift, engine, knowledge_model, scan, setup, wikidraft, wikinew, Error, Result};

/// The team's name (`KEN_EVAL_TEAM`, default `Team`); its new wiki is
/// `<team>-Wiki` beside the repos.
fn team() -> String {
    std::env::var("KEN_EVAL_TEAM").unwrap_or_else(|_| "Team".into())
}

fn wiki_name() -> String {
    format!("{}-Wiki", team())
}

/// A notes folder handed to the first draft as extra material
/// (`KEN_EVAL_EXTRA`, a folder name beside the repos), if any.
fn extra_folder() -> Option<String> {
    std::env::var("KEN_EVAL_EXTRA").ok().filter(|s| !s.is_empty())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: eval_run <phase> <parent> [args]");
        std::process::exit(2);
    }
    let Some(base) = std::env::var_os("KEN_DATA_DIR").map(PathBuf::from) else {
        eprintln!("set KEN_DATA_DIR to a scratch folder first");
        std::process::exit(2);
    };
    let parent = PathBuf::from(&args[1]);
    local_llm::init(base.clone());
    let started = Instant::now();
    let out = match args[0].as_str() {
        "setup" => phase_setup(&base, &parent),
        "index" => phase_index(&base, &parent),
        "embed" => phase_embed(&base, &parent),
        "wiki" => phase_wiki(&base, &parent),
        "extract" => phase_extract(&base, &parent, args.get(2).and_then(|m| m.parse().ok()).unwrap_or(30)),
        "map" => phase_map(&base, &parent),
        "add-repo" => phase_add_repo(&base, &parent, &args[2]),
        "sync" => phase_sync(&base, &parent, &args[2]),
        "chat" => phase_chat(&base, &parent, Path::new(args.get(2).map(String::as_str).unwrap_or("questions.tsv"))),
        "kg" => phase_kg(&base, &parent),
        "ask" => phase_ask(&base, &parent, Path::new(args.get(2).map(String::as_str).unwrap_or("questions.tsv"))),
        "look" => phase_look(&base, &parent, Path::new(args.get(2).map(String::as_str).unwrap_or("questions.tsv"))),
        "drift" => phase_drift(&base, &parent),
        "drift-change" => phase_drift_change(&base, &parent, &args[2], &args[3]),
        "ignore" => phase_ignore(&base, &parent, &args[2]),
        "status" => phase_status(&base, &parent),
        "sql" => phase_sql(&base, &parent, &args[2], &args[3]),
        "model" => phase_model(&base, &parent, &args[2]),
        "imports" => {
            let t = Instant::now();
            let deps = wikidraft::folder_imports(&parent.join(&args[2]));
            println!("# Imports between folders in `{}`\n\n```\n{}\n```\n\n_{:.1}s_", args[2], deps.as_deref().unwrap_or("(none)"), t.elapsed().as_secs_f64());
            Ok(())
        }
        other => Err(Error::Other(format!("unknown phase {other}"))),
    };
    match out {
        Ok(()) => println!("\n_{} took {:.1}s_", args[0], started.elapsed().as_secs_f64()),
        Err(e) => {
            eprintln!("FAILED: {e}");
            std::process::exit(1);
        }
    }
}

fn today() -> String {
    // YYYY-MM-DD as the app writes it, in UTC: no time-zone crate needed.
    let secs = engine::now_epoch();
    let days = secs.div_euclid(86_400);
    // Civil-from-days (Howard Hinnant), UTC — close enough for a report.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}

fn members(parent: &Path) -> Result<Vec<(String, Project)>> {
    let ws = Workspace::open(parent)?;
    Ok(ws
        .members
        .into_iter()
        .filter_map(|m| match m.status {
            MemberStatus::Ok(p) => Some((m.name, p)),
            _ => None,
        })
        .collect())
}

fn short(s: &str, n: usize) -> String {
    let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > n {
        one.chars().take(n).collect::<String>() + "…"
    } else {
        one
    }
}

// ---------------------------------------------------------------- setup

fn phase_setup(base: &Path, parent: &Path) -> Result<()> {
    let prop = setup::propose(parent)?;
    println!("# Setup proposal for `{}`\n", parent.display());
    println!("{} repos, {} folders without git, teams proposed: {:?}\n", prop.repos, prop.without_git, prop.teams);
    println!("| member | include | kind | index | git | evidence | description |\n|---|---|---|---|---|---|---|");
    for r in &prop.rows {
        println!(
            "| {} | {} | {:?} | {:?} | {} | {} | {} |",
            r.member,
            r.include,
            r.kind,
            r.index,
            r.has_git,
            short(&r.evidence.join("; "), 90),
            short(&r.description, 70)
        );
    }
    println!("\n**Ignores proposed**\n");
    for i in &prop.ignores {
        println!(
            "- `{}` {:?} ticked={} fixed={} — {} ({})",
            i.pattern,
            i.state,
            i.ticked,
            i.fixed,
            i.reason,
            short(&i.evidence, 80)
        );
    }

    let mut rows = prop.rows.clone();
    for r in rows.iter_mut().filter(|r| r.include) {
        r.team = Some(team());
    }
    let covered: Vec<wikinew::Covered> = rows
        .iter()
        .filter(|r| r.include)
        .map(|r| wikinew::Covered { name: r.member.clone(), description: r.description.clone() })
        .collect();
    let taken: Vec<String> = rows.iter().map(|r| r.member.clone()).collect();
    let mut wiki = setup::create_wiki(&parent.join(wiki_name()), &team(), &covered, &taken, &today())?;
    wiki.team = Some(team());
    println!("\n**New wiki row**: {} {:?} {:?}", wiki.member, wiki.kind, wiki.index);
    rows.push(wiki);

    let ws = setup::confirm(base, parent, &team(), &rows, &prop.ignores, &today())?;
    println!("\n## Confirmed workspace `{}`\n", ws.config.name);
    let reg = Registry::load(base)?;
    for m in &ws.members {
        let root = parent.join(&m.name);
        let e = reg.entry_at(&root);
        let where_config = if root.join(".ken").join("project.json").exists() { "in repo (.ken/)" } else { "app data (outside)" };
        println!(
            "- **{}** kind={:?} index={:?} team={:?} config={}",
            m.name,
            e.map(|e| e.kind.clone()).unwrap_or_default(),
            e.map(|e| e.index),
            e.and_then(|e| e.team.clone()),
            where_config
        );
    }
    let ki = parent.join(".kenignore");
    println!("\n**`{}`**\n\n```\n{}\n```", ki.display(), std::fs::read_to_string(&ki).unwrap_or_else(|_| "(none written)".into()));
    Ok(())
}

// ---------------------------------------------------------------- index

fn tier_counts(base: &Path, id: uuid::Uuid) -> BTreeMap<i64, i64> {
    let conn = rusqlite::Connection::open(ken_core::db::db_path(base, id)).unwrap();
    let mut stmt = conn.prepare("SELECT tier, COUNT(*) FROM files GROUP BY tier").unwrap();
    stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
}

fn phase_index(base: &Path, parent: &Path) -> Result<()> {
    println!("# Index\n\n| member | kind | files in index | full / search-only | failed | chunks | queued for extraction | time |\n|---|---|---|---|---|---|---|---|");
    let reg = Registry::load(base)?;
    for (name, p) in members(parent)? {
        let t = Instant::now();
        let mut db = Db::open(base, p.config.id)?;
        let _ = db.refresh_stored_kinds();
        let stats = scan::scan(&p, &mut db)?;
        let _ = db.backfill_page_meta();
        let _ = db.backfill_page_links();
        let _ = db.backfill_code_map();
        let _ = db.backfill_extractions();
        let tiers = tier_counts(base, p.config.id);
        println!(
            "| {} | {:?} | {} | {} / {} | {} | {} | {} | {:.1}s |",
            name,
            reg.entry_at(&p.root).map(|e| e.kind.clone()).unwrap_or_default(),
            db.file_count()?,
            tiers.get(&0).copied().unwrap_or(0),
            tiers.get(&1).copied().unwrap_or(0),
            stats.failed,
            db.chunk_count()?,
            db.extractable_file_count()?,
            t.elapsed().as_secs_f64()
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- embed

fn phase_embed(base: &Path, parent: &Path) -> Result<()> {
    let mut emb = ken_core::embedder::installed_embedding_model()
        .ok_or_else(|| Error::Other("no embedding model installed under KEN_DATA_DIR/whisper".into()))?;
    println!("# Meaning index ({}, {} dims)\n\n| member | chunks | embedded | time |\n|---|---|---|---|", emb.model_id(), emb.dim());
    for (name, p) in members(parent)? {
        let t = Instant::now();
        let mut db = Db::open(base, p.config.id)?;
        let mut last = 0usize;
        let built = engine::rebuild_semantic_index(&p, &mut db, emb.as_mut(), &CancelToken::new(), |done, total| {
            if total > 0 && done * 10 / total > last {
                last = done * 10 / total;
                eprintln!("  {name}: {done}/{total}");
            }
        })?;
        println!("| {} | {} | {} | {:.1}s |", name, db.chunk_count()?, built, t.elapsed().as_secs_f64());
    }
    Ok(())
}

// ---------------------------------------------------------------- wiki

fn team_members(base: &Path, parent: &Path) -> Result<Vec<wikidraft::TeamMember>> {
    let reg = Registry::load(base)?;
    Ok(members(parent)?
        .into_iter()
        .map(|(name, p)| {
            let e = reg.entry_at(&p.root);
            wikidraft::TeamMember {
                name,
                root: p.root.clone(),
                kind: e.map(|e| e.kind.clone()).unwrap_or_default(),
                team: e.and_then(|e| e.team.clone()),
            }
        })
        .collect())
}

fn claude(root: &Path) -> Result<impl FnMut(&str) -> Result<String>> {
    let bin = ken_core::runner::discover_claude().ok_or_else(|| Error::Other("claude CLI not found".into()))?;
    let root = root.to_path_buf();
    Ok(move |prompt: &str| {
        let t = Instant::now();
        let out = assistant::oneshot(&bin, &root, prompt, Duration::from_secs(600), &CancelToken::new())?;
        eprintln!("  claude: {} chars in, {:.0}s", prompt.len(), t.elapsed().as_secs_f64());
        match out {
            assistant::OneshotOutcome::Completed(text) => Ok(text),
            other => Err(Error::Other(format!("claude: {other:?}"))),
        }
    })
}

fn phase_wiki(base: &Path, parent: &Path) -> Result<()> {
    let wiki = Project::open(&parent.join(wiki_name()))?;
    let team = team_members(base, parent)?;
    let repos = wikidraft::team_repos(&wiki_name(), &team);
    println!("# First wiki draft\n\nSources: {:?}\n", repos.iter().map(|r| &r.0).collect::<Vec<_>>());
    // A folder a person would hand over as extra material, such as the
    // team's own notes vault when setup counted it as a wiki (team_repos
    // leaves wikis out).
    let extra = extra_folder().map(|f| parent.join(f)).filter(|d| d.is_dir() && !repos.iter().any(|r| &r.1 == d));
    let mut db = Db::open(base, wiki.config.id)?;
    let generate = claude(&wiki.root)?;
    let report = wikidraft::draft_team(
        &wiki.root,
        &wiki_name(),
        &mut db,
        &repos,
        extra.as_deref(),
        &today(),
        engine::now_epoch(),
        generate,
    )?;
    println!("- drafted: {:?}\n- kept: {:?}\n- failed: {:?}\n- proposed: {:?}\n- still to fill: {:?}\n- sources used: {}", report.drafted, report.kept, report.failed, report.proposed, report.to_fill, report.sources.len());
    let stats = scan::scan(&wiki, &mut db)?;
    println!("- wiki rescanned: {} added, {} updated", stats.added, stats.updated);
    Ok(())
}

// ---------------------------------------------------------------- extract

/// Ken's chat on each question (or `KEN_EVAL_ONLY`), headless: the chat's
/// guide and Ken's MCP server over this evaluation's data, started in the
/// team wiki with every member readable. Found when an expected file is in
/// the answer (it cites `ken://<id>/<path>`).
fn phase_chat(base: &Path, parent: &Path, questions: &Path) -> Result<()> {
    let text = std::fs::read_to_string(questions).map_err(|e| Error::Other(format!("{}: {e}", questions.display())))?;
    let only: Vec<usize> =
        std::env::var("KEN_EVAL_ONLY").unwrap_or_default().split(',').filter_map(|n| n.trim().parse().ok()).collect();
    let binary = ken_core::runner::discover_claude().ok_or_else(|| Error::Other("Claude Code not found".into()))?;
    let mcp = std::env::current_exe().map_err(|e| Error::Other(e.to_string()))?.parent().and_then(|d| d.parent()).map(|d| d.join("ken-mcp.exe")).filter(|p| p.exists())
        .ok_or_else(|| Error::Other("ken-mcp.exe not built next to the harness".into()))?;
    let wiki = Project::open(&parent.join(wiki_name()))?;
    let cfg = base.join("chat-mcp").join("eval.json");
    std::fs::create_dir_all(cfg.parent().unwrap()).map_err(|e| Error::Other(e.to_string()))?;
    let config = serde_json::json!({"mcpServers": {"ken": {"command": mcp, "args": ["--project", wiki.root], "env": {"KEN_DATA_DIR": base}}}});
    std::fs::write(&cfg, config.to_string()).map_err(|e| Error::Other(e.to_string()))?;
    let dirs: Vec<PathBuf> = members(parent)?.into_iter().map(|(_, p)| p.root.clone()).collect();
    let cite: Vec<(String, uuid::Uuid)> = members(parent)?.into_iter().map(|(n, p)| (n, p.config.id)).collect();
    let (mut asked, mut found, mut not_there) = (0, 0, 0);
    println!("# Ken's chat\n\nstarted in `{}`, MCP `{}`\n", wiki_name(), mcp.display());
    for (qi, line) in text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).enumerate() {
        let n = qi + 1;
        if !only.is_empty() && !only.contains(&n) {
            continue;
        }
        let (q, expect) = line.split_once('\t').unwrap_or((line, ""));
        let expects: Vec<String> = expect.split('|').filter(|e| !e.is_empty()).map(|e| e.to_lowercase()).collect();
        let t = Instant::now();
        // As the app sends a workspace turn: how to link each repo's files.
        let prompt = match ken_core::chat::build_cite_preamble(&cite) {
            Some(c) => format!("{c}\n\n{q}"),
            None => q.to_string(),
        };
        let outcome = assistant::chat_oneshot(&binary, &wiki.root, &dirs, &cfg, &prompt, Duration::from_secs(300), &CancelToken::new())?;
        let answer = match outcome {
            assistant::OneshotOutcome::Completed(t) => t,
            other => format!("(no answer: {other:?})"),
        };
        let lower = answer.to_lowercase();
        let hit = expects.iter().any(|e| lower.contains(e));
        let missing = !hit && (lower.contains("isn't there") || lower.contains("not there") || lower.contains("could not find") || lower.contains("couldn't find"));
        asked += 1;
        found += usize::from(hit);
        not_there += usize::from(missing);
        println!(
            "## Q{n}. {q}\n\n{} · {:.0}s · expected `{expect}`\n\n{}\n",
            if hit { "FOUND" } else if missing { "SAID NOT THERE" } else { "MISSED" },
            t.elapsed().as_secs_f64(),
            short(&answer, 700)
        );
    }
    println!("## Score\n\n{found} of {asked} answers name an expected file; {not_there} said it was not there.");
    Ok(())
}

/// Wiki sync as the app runs it, against a bare remote in the evaluation's
/// own folder (`<parent>/../remotes/<member>.git`), never a real one: commit
/// all and push, then a teammate's pushed change pulled back.
fn phase_sync(base: &Path, parent: &Path, member: &str) -> Result<()> {
    use ken_core::sync;
    let p = Project::open(&parent.join(member))?;
    let kind = Registry::load(base)?.entry_at(&p.root).map(|e| e.kind.clone()).unwrap_or_default();
    let area = parent.parent().unwrap_or(parent);
    let remote = area.join("remotes").join(format!("{member}.git"));
    let git = |dir: &Path, args: &[&str]| -> std::result::Result<String, String> {
        let o = std::process::Command::new("git")
            .args(["-c", "user.name=Ken eval", "-c", "user.email=ken-eval@example.invalid"])
            .args(args)
            .current_dir(dir)
            .output()
            .map_err(|e| e.to_string())?;
        if o.status.success() { Ok(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { Err(String::from_utf8_lossy(&o.stderr).trim().to_string()) }
    };
    let (origin, branch) = sync::remote_and_branch(&p.root);
    if origin.is_none() {
        std::fs::create_dir_all(remote.parent().unwrap()).map_err(|e| Error::Other(e.to_string()))?;
        git(area, &["init", "-q", "--bare", &remote.to_string_lossy()]).map_err(Error::Other)?;
        git(&p.root, &["remote", "add", "origin", &remote.to_string_lossy()]).map_err(Error::Other)?;
    }
    let url = git(&p.root, &["remote", "get-url", "origin"]).map_err(Error::Other)?.replace('\\', "/");
    if !url.to_lowercase().starts_with(&area.to_string_lossy().replace('\\', "/").to_lowercase()) {
        return Err(Error::Other(format!("origin `{url}` is outside the evaluation folder; not pushing")));
    }
    let branch = branch.unwrap_or_else(|| "main".into());
    println!("# Sync `{member}`\n\nsync on by default for its kind {kind:?}: {}\nremote: `{url}` branch `{branch}`\n", sync::sync_auto(&p, &kind));
    sync::ensure_excludes(&p.root)?;
    // First push sets the upstream, as a person does once.
    let committed = sync::commit_all(&p.root)?;
    let first = git(&p.root, &["push", "-q", "-u", "origin", &branch]);
    println!("commit: {committed} · first push: {first:?}");
    let tracked = git(&remote, &["ls-tree", "-r", "--name-only", &branch]).unwrap_or_default();
    println!(
        "on the remote: {} files · .ken/project.json {} · staging {}",
        tracked.lines().count(),
        if tracked.lines().any(|l| l == ".ken/project.json") { "committed" } else { "MISSING" },
        if tracked.lines().any(|l| l.starts_with(".ken/.staging")) { "LEAKED" } else { "kept out" }
    );

    // A local edit, synced.
    let page = p.root.join("Current/Project.md");
    let mut text = std::fs::read_to_string(&page).map_err(|e| Error::Other(e.to_string()))?;
    text.push_str("\nNote from the evaluation's sync step.\n");
    std::fs::write(&page, &text).map_err(|e| Error::Other(e.to_string()))?;
    let committed = sync::commit_all(&p.root)?;
    println!("local edit: committed {committed}, push {:?}", sync::push(&p.root)?.eq_name());
    let subject = git(&remote, &["log", "-1", "--format=%s", &branch]).unwrap_or_default();
    println!("remote's newest commit: {subject:?}");

    // A teammate's change, pulled.
    let mate = area.join("remotes").join(format!("{member}-teammate"));
    let _ = std::fs::remove_dir_all(&mate);
    git(area, &["clone", "-q", &remote.to_string_lossy(), &mate.to_string_lossy()]).map_err(Error::Other)?;
    std::fs::write(mate.join("Current/Teammate.md"), "# Teammate\n\nAdded on another machine.\n").map_err(|e| Error::Other(e.to_string()))?;
    git(&mate, &["add", "-A"]).map_err(Error::Other)?;
    git(&mate, &["commit", "-q", "-m", "a teammate's page"]).map_err(Error::Other)?;
    git(&mate, &["push", "-q"]).map_err(Error::Other)?;
    let pulled = sync::pull(&p.root, |_| false)?;
    println!("pull: {} · teammate's page here: {}", match pulled {
        sync::PullOutcome::Clean => "clean".to_string(),
        sync::PullOutcome::Conflicts(c) => format!("{} conflicts", c.len()),
        sync::PullOutcome::Failed(e) => format!("failed: {e}"),
    }, p.root.join("Current/Teammate.md").exists());
    Ok(())
}

trait PushName {
    fn eq_name(&self) -> String;
}
impl PushName for ken_core::sync::PushOutcome {
    fn eq_name(&self) -> String {
        match self {
            ken_core::sync::PushOutcome::Pushed => "pushed".into(),
            ken_core::sync::PushOutcome::NoRemote => "no remote".into(),
            ken_core::sync::PushOutcome::Failed(e) => format!("failed: {e}"),
        }
    }
}

/// A repo joins the team later: `folder` (beside the others) is ticked at
/// set-up, confirmed onto the team, indexed, and the wiki gets its Repo Map
/// page drafted and proposed changes for the pages a person keeps.
fn phase_add_repo(base: &Path, parent: &Path, folder: &str) -> Result<()> {
    let prop = setup::propose(parent)?;
    let mut rows = prop.rows.clone();
    let Some(row) = rows.iter_mut().find(|r| r.member == folder) else {
        return Err(Error::Other(format!("{folder} is not a candidate beside the others")));
    };
    row.include = true;
    row.team = Some(team());
    println!("# `{folder}` joins the team\n\nproposed as {:?} {:?}", row.kind, row.index);
    setup::confirm(base, parent, &team(), &rows, &prop.ignores, &today())?;
    let added = Project::open(&parent.join(folder))?;
    let mut adb = Db::open(base, added.config.id)?;
    let stats = scan::scan(&added, &mut adb)?;
    println!("indexed: {} files", stats.added + stats.updated);

    let wiki = Project::open(&parent.join(wiki_name()))?;
    let all = wikidraft::team_repos(&wiki_name(), &team_members(base, parent)?);
    println!("team repos now: {:?}\n", all.iter().map(|r| &r.0).collect::<Vec<_>>());
    let mut db = Db::open(base, wiki.config.id)?;
    let report = wikidraft::draft_added(
        &wiki.root,
        &wiki_name(),
        &mut db,
        &[(folder.to_string(), added.root.clone())],
        &all,
        &today(),
        engine::now_epoch(),
        claude(&wiki.root)?,
    )?;
    println!("- drafted: {:?}\n- proposed: {:?}\n- kept (no change): {:?}\n- failed: {:?}", report.drafted, report.proposed, report.kept, report.failed);
    let conn = rusqlite::Connection::open(ken_core::db::db_path(base, wiki.config.id)).map_err(|e| Error::Other(e.to_string()))?;
    let mut stmt = conn
        .prepare("SELECT kind, title, body FROM review_items WHERE status != 'resolved' ORDER BY id DESC LIMIT 6")
        .map_err(|e| Error::Other(e.to_string()))?;
    let cards: Vec<(String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|e| Error::Other(e.to_string()))?
        .flatten()
        .collect();
    for (kind, title, body) in cards {
        println!("\n### card ({kind}): {title}\n\n{}", short(&body, 700));
    }
    Ok(())
}

/// Each member's knowledge map built by Claude, as the app does now: the
/// members whose files are read for entities (search-only repos have none).
fn phase_map(base: &Path, parent: &Path) -> Result<()> {
    let binary = ken_core::runner::discover_claude().ok_or_else(|| Error::Other("Claude Code not found".into()))?;
    println!("# Knowledge map (Claude)\n\n| member | files read | entities | edges | still waiting | seconds |\n|---|---|---|---|---|---|");
    for (name, p) in members(parent)? {
        let mut db = Db::open(base, p.config.id)?;
        let files = db.entity_tier_paths()?.len();
        if files == 0 {
            continue;
        }
        let t = Instant::now();
        match knowledge_model::build_knowledge_model(&binary, &p, &mut db, &today(), &CancelToken::new()) {
            Ok(_) => {
                let (ents, edges) = db.list_entities_with_edges()?;
                let waiting = db.waiting_extractions()?.len();
                println!("| {name} | {files} | {} | {} | {waiting} | {:.0} |", ents.len(), edges.len(), t.elapsed().as_secs_f64());
            }
            Err(e) => println!("| {name} | {files} | failed: {e} | | | {:.0} |", t.elapsed().as_secs_f64()),
        }
    }
    Ok(())
}

fn phase_extract(base: &Path, parent: &Path, minutes: u64) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(minutes * 60);
    let generate = |p: &str| local_llm::generate_json(p, Priority::Background);
    println!("# Entity extraction (local model, budget {minutes} min)\n\n| member | done / extractable | entities | edges | seconds per file |\n|---|---|---|---|---|");
    for (name, p) in members(parent)? {
        let mut db = Db::open(base, p.config.id)?;
        if db.extractable_file_count()? == 0 {
            continue;
        }
        let _ = db.requeue_errored_extractions();
        let mut files = 0;
        let t = Instant::now();
        while Instant::now() < deadline {
            match knowledge_model::process_next_pending_with_addendum(&mut db, &today(), engine::now_epoch(), &generate, "") {
                Ok(Some(path)) => {
                    files += 1;
                    eprintln!("  {name}: {path} ({:.0}s avg)", t.elapsed().as_secs_f64() / files as f64);
                }
                Ok(None) => break,
                Err(e) => {
                    eprintln!("  {name}: error {e}");
                    files += 1;
                }
            }
        }
        let (done, total) = db.extraction_coverage()?;
        let (ents, edges) = db.list_entities_with_edges()?;
        let per = if files > 0 { t.elapsed().as_secs_f64() / files as f64 } else { 0.0 };
        println!("| {} | {} / {} | {} | {} | {:.1} |", name, done, total, ents.len(), edges.len(), per);
    }
    Ok(())
}

// ---------------------------------------------------------------- kg

/// As the app does it: the batched judgements (same thing? how related?)
/// on Claude; no local generation.
struct LocalLlm {
    claude: Option<PathBuf>,
    root: PathBuf,
}

impl FederationLlm for LocalLlm {
    // No local generation (the app's policy): summaries fall back to the
    // longest member summary.
    fn complete(&self, _prompt: &str) -> Result<String> {
        Err(Error::Other("no local generation".into()))
    }

    fn judge(&self, prompt: &str) -> Result<String> {
        let Some(bin) = &self.claude else { return self.complete(prompt) };
        match assistant::oneshot(bin, &self.root, prompt, Duration::from_secs(300), &CancelToken::new())? {
            assistant::OneshotOutcome::Completed(text) => Ok(text),
            other => Err(Error::Other(format!("claude: {other:?}"))),
        }
    }
}

fn kg_members(base: &Path, parent: &Path) -> Result<Vec<(String, uuid::Uuid, Db)>> {
    let reg = Registry::load(base)?;
    let mut out: Vec<(String, uuid::Uuid, Db)> = members(parent)?
        .into_iter()
        .filter(|(_, p)| reg.entry_at(&p.root).is_some_and(|e| e.index.unwrap_or_else(|| IndexState::for_kind(&e.kind)) == IndexState::Entities))
        .filter_map(|(n, p)| Db::open_read_only(base, p.config.id).ok().map(|db| (n, p.config.id, db)))
        .collect();
    out.sort_by_key(|m| m.1);
    Ok(out)
}

fn phase_kg(base: &Path, parent: &Path) -> Result<()> {
    let ms = kg_members(base, parent)?;
    let fed: Vec<federation::Member> = ms.iter().map(|(_, id, db)| federation::Member { project_id: *id, db }).collect();
    let mut kg = WorkspaceKgDb::open(parent)?;
    let llm = LocalLlm { claude: ken_core::runner::discover_claude(), root: parent.to_path_buf() };
    let rep = federation::build_workspace_kg(&mut kg, &fed, Some(&llm), engine::now_epoch(), &CancelToken::new())?;
    let names: HashMap<String, &str> = ms.iter().map(|(n, id, _)| (id.to_string(), n.as_str())).collect();
    println!("# Workspace knowledge graph\n\nMembers: {:?}\n\n{rep:#?}\n", ms.iter().map(|m| &m.0).collect::<Vec<_>>());

    // Clusters: global entities that merged local entities from more than
    // one member, or several local names into one.
    let mut merged = Vec::new();
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    for g in kg.list_global_entities()? {
        *by_kind.entry(g.kind.clone()).or_default() += 1;
        let links = kg.list_links_for_global(g.id)?;
        let projects: std::collections::BTreeSet<_> = links.iter().map(|l| l.project_id.clone()).collect();
        let locals: std::collections::BTreeSet<_> = links.iter().map(|l| l.local_name.clone()).collect();
        if projects.len() > 1 || locals.len() > 1 {
            merged.push((g, links, projects.len()));
        }
    }
    println!("Global entities by kind: {by_kind:?}\n\n## Merged clusters ({})\n", merged.len());
    merged.sort_by_key(|m| std::cmp::Reverse(m.1.len()));
    for (g, links, np) in merged.iter().take(40) {
        let locals: Vec<String> = links
            .iter()
            .map(|l| format!("{} ({})", l.local_name, names.get(&l.project_id).copied().unwrap_or("?")))
            .collect();
        println!("- **{}** [{}] across {np} members ← {}", g.name, g.kind, locals.join(", "));
    }
    Ok(())
}

// ---------------------------------------------------------------- ask

/// `questions.tsv`: `question<TAB>expected path fragment[|another]`.
/// "Ask Ken to look" on each question (or the numbers in `KEN_EVAL_ONLY`,
/// e.g. `9,10,15`): the read-only fallback searches the members itself.
/// Found when a source it cites is an expected file.
fn phase_look(base: &Path, parent: &Path, questions: &Path) -> Result<()> {
    let text = std::fs::read_to_string(questions).map_err(|e| Error::Other(format!("{}: {e}", questions.display())))?;
    let only: Vec<usize> =
        std::env::var("KEN_EVAL_ONLY").unwrap_or_default().split(',').filter_map(|n| n.trim().parse().ok()).collect();
    let binary = ken_core::runner::discover_claude().ok_or_else(|| Error::Other("Claude Code not found".into()))?;
    let folders: Vec<(String, uuid::Uuid, PathBuf)> =
        members(parent)?.into_iter().map(|(n, p)| (n, p.config.id, p.root.clone())).collect();
    let dirs: Vec<PathBuf> = folders.iter().map(|(_, _, r)| r.clone()).collect();
    // The leads the app hands the look: what routed search ranked.
    let mut emb = ken_core::embedder::installed_embedding_model();
    let kg = WorkspaceKgDb::open(parent).ok();
    let ms: Vec<(String, Project, Db)> = members(parent)?
        .into_iter()
        .filter_map(|(n, p)| Db::open_read_only(base, p.config.id).ok().map(|db| (n, p, db)))
        .collect();
    let info: Vec<MemberInfo> = ms
        .iter()
        .map(|(n, p, db)| MemberInfo { project_id: p.config.id, name: n.clone(), index_ready: db.vec_available(), last_activity: 0 })
        .collect();
    let (mut asked, mut found, mut named_only, mut not_there) = (0, 0, 0, 0);
    println!("# Ask Ken to look\n");
    for (qi, line) in text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).enumerate() {
        let n = qi + 1;
        if !only.is_empty() && !only.contains(&n) {
            continue;
        }
        let (q, expect) = line.split_once('\t').unwrap_or((line, ""));
        let expects: Vec<String> = expect.split('|').filter(|e| !e.is_empty()).map(|e| e.to_lowercase()).collect();
        let t = Instant::now();
        let plan = routing::plan_route(q, &info, kg.as_ref());
        let query_vec = emb.as_mut().and_then(|e| e.embed_query(q).ok());
        let member_hits: Vec<routing::MemberHits> = plan
            .targets
            .iter()
            .filter_map(|id| ms.iter().find(|(_, p, _)| p.config.id == *id))
            .map(|(n, p, db)| routing::MemberHits {
                project_id: p.config.id,
                member_name: n.clone(),
                status: routing::MemberStatus::Searched,
                hits: routing::search_member(db, q, query_vec.as_deref(), 8).unwrap_or_default(),
            })
            .collect();
        let leads: Vec<String> =
            routing::merge_routed(&plan, &member_hits, 8).results.iter().map(|r| format!("{}/{}", r.member_name, r.path)).collect();
        let prompt = assistant::look_prompt(q, &folders, &leads);
        let outcome = assistant::look(&binary, &parent.join(&folders[0].0), &dirs, &prompt, Duration::from_secs(300), &CancelToken::new())?;
        let answer = match outcome {
            assistant::OneshotOutcome::Completed(t) => t,
            other => format!("(no answer: {other:?})"),
        };
        let parsed = ken_core::digest::parse_digest(&answer);
        // A cited ken://<id>/<path> as `member/path`, to match against.
        let cited: Vec<String> = parsed
            .sources
            .iter()
            .map(|s| {
                let rest = s.trim_start_matches("ken://");
                let (id, path) = rest.split_once('/').unwrap_or((rest, ""));
                let name = folders.iter().find(|(_, fid, _)| fid.to_string() == id).map_or(id, |(n, _, _)| n.as_str());
                format!("{name}/{}", path.split('#').next().unwrap_or(path))
            })
            .collect();
        let hit = cited.iter().any(|c| expects.iter().any(|e| c.to_lowercase().contains(e)));
        // Right file named in the answer, but not opened and cited.
        let named = !hit && expects.iter().any(|e| parsed.body.to_lowercase().contains(e));
        let says_missing = parsed.body.to_lowercase().contains("not there") || parsed.body.to_lowercase().contains("could not find");
        asked += 1;
        found += usize::from(hit);
        named_only += usize::from(named);
        not_there += usize::from(says_missing && !hit);
        println!(
            "## Q{n}. {q}\n\n{} · {:.0}s · expected `{expect}`\n\n{}\n\ncited: {cited:?}\n",
            if hit {
                "FOUND"
            } else if named {
                "NAMED, NOT CITED"
            } else if says_missing {
                "SAID NOT THERE"
            } else {
                "MISSED"
            },
            t.elapsed().as_secs_f64(),
            short(&parsed.body, 600)
        );
    }
    println!(
        "## Score\n\n{found} of {asked} cited an expected file; {named_only} more named it without citing it; {not_there} said it was not there."
    );
    Ok(())
}

fn phase_ask(base: &Path, parent: &Path, questions: &Path) -> Result<()> {
    let text = std::fs::read_to_string(questions).map_err(|e| Error::Other(format!("{}: {e}", questions.display())))?;
    let mut emb = ken_core::embedder::installed_embedding_model();
    let kg = WorkspaceKgDb::open(parent).ok();
    let ms: Vec<(String, Project, Db)> = members(parent)?
        .into_iter()
        .filter_map(|(n, p)| Db::open_read_only(base, p.config.id).ok().map(|db| (n, p, db)))
        .collect();
    let info: Vec<MemberInfo> = ms
        .iter()
        .map(|(n, p, db)| MemberInfo { project_id: p.config.id, name: n.clone(), index_ready: db.vec_available(), last_activity: 0 })
        .collect();
    let handles: Vec<MemberDbHandle> = ms
        .iter()
        .map(|(n, p, db)| MemberDbHandle { project_id: p.config.id, name: n, db, index_ready: db.vec_available() })
        .collect();

    let mut scores = (0usize, 0usize, 0usize, 0usize, 0usize);
    println!("# Curated questions\n");
    for (qi, line) in text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).enumerate() {
        let (q, expect) = line.split_once('\t').unwrap_or((line, ""));
        let expects: Vec<&str> = expect.split('|').filter(|e| !e.is_empty()).collect();
        let plan = routing::plan_route(q, &info, kg.as_ref());
        let targets: Vec<&str> = plan.targets.iter().filter_map(|id| info.iter().find(|m| m.project_id == *id)).map(|m| m.name.as_str()).collect();

        // Hybrid (as the app with meaning search on) and keyword-only.
        let t = Instant::now();
        // As the app's routed search: each target searched (kept to the kinds
        // in KEN_EVAL_TYPES, if set), then merged by relevance.
        let types = ken_core::contenttype::parse_filter(std::env::var("KEN_EVAL_TYPES").ok().as_deref());
        let query_vec = emb.as_mut().and_then(|e| e.embed_query(q).ok());
        let member_hits: Vec<routing::MemberHits> = plan
            .targets
            .iter()
            .filter_map(|id| handles.iter().find(|h| h.project_id == *id))
            .map(|h| routing::MemberHits {
                project_id: h.project_id,
                member_name: h.name.to_string(),
                status: routing::MemberStatus::Searched,
                hits: routing::search_member_of(h.db, q, query_vec.as_deref(), 8, &types).unwrap_or_default(),
            })
            .collect();
        let hybrid = routing::merge_routed(&plan, &member_hits, 8);
        let ms_h = t.elapsed().as_millis();
        let keyword: Vec<MemberHitsLite> = plan
            .targets
            .iter()
            .filter_map(|id| handles.iter().find(|h| h.project_id == *id))
            .map(|h| MemberHitsLite {
                member: h.name.to_string(),
                paths: routing::search_member(h.db, q, None, 8).unwrap_or_default().into_iter().map(|x| x.path).collect(),
            })
            .collect();

        let rank_of = |paths: &[String]| paths.iter().position(|p| expects.iter().any(|e| p.contains(e)));
        let hybrid_paths: Vec<String> = hybrid.results.iter().map(|r| format!("{}/{}", r.member_name, r.path)).collect();
        let kw_paths: Vec<String> = keyword.iter().flat_map(|k| k.paths.iter().map(move |p| format!("{}/{}", k.member, p))).collect();
        let hr = rank_of(&hybrid_paths);
        let kr = rank_of(&kw_paths);
        scores.0 += 1;
        if hr == Some(0) {
            scores.1 += 1;
        }
        if hr.is_some_and(|r| r < 5) {
            scores.2 += 1;
        }
        if kr == Some(0) {
            scores.3 += 1;
        }
        if kr.is_some_and(|r| r < 5) {
            scores.4 += 1;
        }
        println!(
            "## Q{}. {q}\n\nRoute: {:?} → {:?} · expected `{expect}` · hybrid rank {} · keyword rank {} · {ms_h} ms\n",
            qi + 1,
            match &plan.reason {
                routing::RouteReason::KgEntities(ids) => format!("graph ({} entities)", ids.len()),
                other => format!("{other:?}"),
            },
            targets,
            hr.map_or("—".to_string(), |r| (r + 1).to_string()),
            kr.map_or("—".to_string(), |r| (r + 1).to_string()),
        );
        for (i, r) in hybrid.results.iter().take(6).enumerate() {
            println!("{}. `{}/{}`{} ({:?}) — {}", i + 1, r.member_name, r.path, r.line.map(|l| format!(":{l}")).unwrap_or_default(), r.source, short(&r.snippet, 110));
        }
        println!();
    }
    let (n, h1, h5, k1, k5) = scores;
    println!("## Score over {n} questions\n\n| | hit@1 | hit@5 |\n|---|---|---|\n| hybrid (keyword + meaning, routed) | {h1} | {h5} |\n| keyword only | {k1} | {k5} |");
    Ok(())
}

struct MemberHitsLite {
    member: String,
    paths: Vec<String>,
}

// ---------------------------------------------------------------- drift

fn print_drift(label: &str, run: &drift::DriftRun, took: Duration) {
    println!(
        "### {label}\n\nexit {} · pages {} · code citations {} · cross-refs {} · business pages {} · measured {} · reused {} · {:.1}s",
        run.exit_code,
        run.pages_examined,
        run.code_citations,
        run.cross_references,
        run.business_pages,
        run.measured,
        run.reused,
        took.as_secs_f64()
    );
    if let Some(v) = &run.void_reason {
        println!("\nvoid: {v}");
    }
    println!("\nbranches: {:?}\n\nunmeasured: {:?}\n", run.branches, run.unmeasured);
    for m in &run.mismatches {
        println!("- {:?} **{}** `{}` — {}", m.severity, m.subject, m.citation, short(&m.detail, 140));
    }
    for (p, v) in &run.aged {
        println!("- aged **{p}** verified {v:?}");
    }
    println!();
}

fn phase_drift(base: &Path, parent: &Path) -> Result<()> {
    let wiki = Project::open(&parent.join(wiki_name()))?;
    let mut db = Db::open(base, wiki.config.id)?;
    scan::scan(&wiki, &mut db)?;
    println!("# Drift\n");
    for label in ["first sweep", "second sweep, nothing changed"] {
        let t = Instant::now();
        let run = drift::sweep(&wiki, &db, engine::now_epoch())?;
        print_drift(label, &run, t.elapsed());
    }
    Ok(())
}

/// Change `file` in `member` (append a line, commit), then sweep again.
fn phase_drift_change(base: &Path, parent: &Path, member: &str, file: &str) -> Result<()> {
    let repo = parent.join(member);
    let path = repo.join(file);
    let mut text = std::fs::read_to_string(&path).map_err(|e| Error::Other(format!("{}: {e}", path.display())))?;
    // A real change, not a comment (drift clears comment-only changes).
    text.push_str(if file.ends_with(".md") {
        "\nDeploys now need two approvals (changed by the Ken evaluation run).\n"
    } else {
        "\nKEN_EVAL_CHANGE = 1\n"
    });
    std::fs::write(&path, text).map_err(|e| Error::Other(e.to_string()))?;
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["-c", "user.name=Ken eval", "-c", "user.email=ken-eval@example.invalid"])
            .args(args)
            .output()
    };
    let _ = git(&["add", file]);
    let c = git(&["commit", "-q", "-m", "eval: change a cited file"]).map_err(|e| Error::Other(e.to_string()))?;
    println!("# Drift after changing `{member}/{file}`\n\ncommit: {}\n", String::from_utf8_lossy(&c.stderr).trim());
    // Drift measures what is merged upstream, so the change is pushed, as a
    // teammate's merge would land. Only to a remote that is a folder in the
    // evaluation's own area, never to a real one.
    let origin = git(&["remote", "get-url", "origin"]).map(|o| String::from_utf8_lossy(&o.stdout).trim().replace('\\', "/")).unwrap_or_default();
    let area = parent.parent().unwrap_or(parent).to_string_lossy().replace('\\', "/");
    if !origin.is_empty() && origin.to_lowercase().starts_with(&area.to_lowercase()) {
        let p = git(&["push", "-q", "origin", "HEAD"]).map_err(|e| Error::Other(e.to_string()))?;
        println!("pushed to `{origin}`: {}\n", if p.status.success() { "ok".into() } else { String::from_utf8_lossy(&p.stderr).trim().to_string() });
    } else {
        println!("not pushed: `{origin}` is outside `{area}`, so drift will not see this change until it is merged there\n");
    }
    let wiki = Project::open(&parent.join(wiki_name()))?;
    let db = Db::open(base, wiki.config.id)?;
    let t = Instant::now();
    let run = drift::sweep(&wiki, &db, engine::now_epoch())?;
    print_drift("sweep after the change", &run, t.elapsed());
    Ok(())
}

// ---------------------------------------------------------------- ignore

fn phase_ignore(base: &Path, parent: &Path, member: &str) -> Result<()> {
    let p = Project::open(&parent.join(member))?;
    let mut db = Db::open(base, p.config.id)?;
    let before = tier_counts(base, p.config.id);
    let files_before = db.file_count()?;
    // Pick the two biggest top-level folders to demote.
    let conn = rusqlite::Connection::open(ken_core::db::db_path(base, p.config.id)).map_err(|e| Error::Other(e.to_string()))?;
    let mut stmt = conn
        .prepare("SELECT substr(rel_path, 1, instr(rel_path, '/') - 1) AS top, COUNT(*) c FROM files WHERE instr(rel_path, '/') > 0 GROUP BY top ORDER BY c DESC LIMIT 2")
        .map_err(|e| Error::Other(e.to_string()))?;
    let tops: Vec<(String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| Error::Other(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();
    let (ignore, search) = (&tops[0], &tops[1]);
    let ki = p.root.join(".kenignore");
    let old = std::fs::read_to_string(&ki).ok();
    let body = format!("# Ken evaluation\n{}/\n~{}/\n", ignore.0, search.0);
    std::fs::write(&ki, &body).map_err(|e| Error::Other(e.to_string()))?;
    let t = Instant::now();
    let stats = scan::scan(&p, &mut db)?;
    let after = tier_counts(base, p.config.id);
    let conn2 = rusqlite::Connection::open(ken_core::db::db_path(base, p.config.id)).map_err(|e| Error::Other(e.to_string()))?;
    let mut left = conn2
        .prepare("SELECT rel_path, status FROM files WHERE rel_path LIKE ?1 ORDER BY rel_path LIMIT 8")
        .map_err(|e| Error::Other(e.to_string()))?;
    let left: Vec<(String, String)> = left
        .query_map([format!("{}/%", ignore.0)], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| Error::Other(e.to_string()))?
        .flatten()
        .collect();
    println!("still indexed under `{}/` after ignoring it: {left:?}\n", ignore.0);
    println!(
        "# `.kenignore` in `{member}`\n\nWrote:\n```\n{body}```\n`{}/` had {} files, `{}/` had {}.\n\n| | files | full | search-only |\n|---|---|---|---|\n| before | {files_before} | {} | {} |\n| after | {} | {} | {} |\n\nrescan: {} removed, {} updated, {:.1}s",
        ignore.0,
        ignore.1,
        search.0,
        search.1,
        before.get(&0).copied().unwrap_or(0),
        before.get(&1).copied().unwrap_or(0),
        db.file_count()?,
        after.get(&0).copied().unwrap_or(0),
        after.get(&1).copied().unwrap_or(0),
        stats.removed,
        stats.updated,
        t.elapsed().as_secs_f64()
    );
    // A pattern in the workspace's own .kenignore, at the parent: does it
    // reach files inside a member?
    let pk = parent.join(".kenignore");
    let pold = std::fs::read_to_string(&pk).unwrap_or_default();
    std::fs::write(&pk, format!("{pold}\n# Ken evaluation: inside a member\n{member}/{}/\n", search.0)).map_err(|e| Error::Other(e.to_string()))?;
    let _ = std::fs::remove_file(&ki);
    let stats2 = scan::scan(&p, &mut db)?;
    let still: i64 = conn
        .query_row("SELECT COUNT(*) FROM files WHERE rel_path LIKE ?1", [format!("{}/%", search.0)], |r| r.get(0))
        .unwrap_or(-1);
    println!(
        "\nParent `.kenignore` line `{member}/{}/` (member `.kenignore` removed): {} files under `{}/` still indexed after rescan ({} removed, {} added).",
        search.0, still, search.0, stats2.removed, stats2.added
    );
    std::fs::write(&pk, pold).map_err(|e| Error::Other(e.to_string()))?;
    match old {
        Some(o) => std::fs::write(&ki, o).map_err(|e| Error::Other(e.to_string()))?,
        None => {
            let _ = std::fs::remove_file(&ki);
        }
    }
    scan::scan(&p, &mut db)?;
    Ok(())
}

// ---------------------------------------------------------------- model

/// The knowledge model built by Claude over the whole member (the app's
/// "refresh knowledge model"), instead of file by file on the local model.
fn phase_model(base: &Path, parent: &Path, member: &str) -> Result<()> {
    let p = Project::open(&parent.join(member))?;
    let mut db = Db::open(base, p.config.id)?;
    let bin = ken_core::runner::discover_claude().ok_or_else(|| Error::Other("claude CLI not found".into()))?;
    let t = Instant::now();
    let counts = knowledge_model::build_knowledge_model(&bin, &p, &mut db, &today(), &CancelToken::new())?;
    let (ents, edges) = db.list_entities_with_edges()?;
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for e in &ents {
        *kinds.entry(e.kind.clone()).or_default() += 1;
    }
    println!(
        "| {member} | {} entities · {} edges · {} events | stored {} / {} | {kinds:?} | {:.0}s |",
        counts.entities,
        counts.edges,
        counts.events,
        ents.len(),
        edges.len(),
        t.elapsed().as_secs_f64()
    );
    Ok(())
}

// ---------------------------------------------------------------- sql

/// A read-only query against one member's index, rows tab-separated.
fn phase_sql(base: &Path, parent: &Path, member: &str, query: &str) -> Result<()> {
    let p = Project::open(&parent.join(member))?;
    let conn = rusqlite::Connection::open_with_flags(ken_core::db::db_path(base, p.config.id), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| Error::Other(e.to_string()))?;
    let mut stmt = conn.prepare(query).map_err(|e| Error::Other(e.to_string()))?;
    let n = stmt.column_count();
    let rows = stmt
        .query_map([], |r| {
            Ok((0..n)
                .map(|i| r.get::<_, rusqlite::types::Value>(i).map(|v| format!("{v:?}")).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("	"))
        })
        .map_err(|e| Error::Other(e.to_string()))?;
    for row in rows.flatten() {
        println!("{row}");
    }
    Ok(())
}

// ---------------------------------------------------------------- status

fn phase_status(base: &Path, parent: &Path) -> Result<()> {
    println!("# Status\n\n| member | files | chunks | meaning index | extracted | entities |\n|---|---|---|---|---|---|");
    for (name, p) in members(parent)? {
        let db = Db::open_read_only(base, p.config.id)?;
        let (done, total) = db.extraction_coverage()?;
        println!(
            "| {} | {} | {} | {} | {}/{} | {} |",
            name,
            db.file_count()?,
            db.chunk_count()?,
            db.vec_available(),
            done,
            total,
            db.list_entities_with_edges()?.0.len()
        );
    }
    Ok(())
}
