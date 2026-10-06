//! Shattered Realms dry run (local, not committed): what set-up proposes on
//! a real folder, read only, and a confirmed workspace over clones with a
//! fresh wiki that never reads the team's existing wiki.
//!
//! ```text
//! set KEN_DATA_DIR=C:\ken-eval\data
//! cargo run --release -p ken-core --example sr_dryrun -- propose <folder>
//! cargo run --release -p ken-core --example sr_dryrun -- setup <parent>
//! ```

use std::path::{Path, PathBuf};

use ken_core::registry::{IndexState, Registry, RepoKind};
use ken_core::{setup, wikinew};

const TEAM: &str = "Shattered-Realms";
const REFERENCE: &[&str] = &["hytale-shared-source"];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let folder = PathBuf::from(&args[1]);
    let r = match args[0].as_str() {
        "propose" => propose(&folder).map(|_| ()),
        "setup" => {
            let base = PathBuf::from(std::env::var_os("KEN_DATA_DIR").expect("set KEN_DATA_DIR"));
            run_setup(&base, &folder)
        }
        _ => panic!("propose | setup"),
    };
    if let Err(e) = r {
        eprintln!("FAILED: {e}");
        std::process::exit(1);
    }
}

fn propose(folder: &Path) -> ken_core::Result<setup::Proposal> {
    let p = setup::propose(folder)?;
    println!("# Proposal for `{}`\n", folder.display());
    println!("{} repos · {} folders without git · teams {:?} · existing workspace {}\n", p.repos, p.without_git, p.teams, p.existing_workspace);
    println!("| member | include | kind | index | team | git | evidence | description |\n|---|---|---|---|---|---|---|---|");
    for r in &p.rows {
        println!(
            "| {} | {} | {:?} | {:?} | {:?} | {} | {} | {} |",
            r.member,
            r.include,
            r.kind,
            r.index,
            r.team,
            r.has_git,
            r.evidence.join(" · "),
            r.description.split_whitespace().collect::<Vec<_>>().join(" ")
        );
    }
    println!("\n## Ignore rows\n\n| pattern | state | ticked | fixed | reason | evidence |\n|---|---|---|---|---|---|");
    for i in &p.ignores {
        println!("| `{}` | {:?} | {} | {} | {} | {} |", i.pattern, i.state, i.ticked, i.fixed, i.reason, i.evidence);
    }
    Ok(p)
}

/// The kinds Chris assigned (2026-10-06): the three game repos are code,
/// the platform source is reference. Nothing else is included.
fn assigned(member: &str) -> Option<RepoKind> {
    match member {
        "Shattered-Realms" | "Shattered-Realms-Tools" | "skyboxeditor" => Some(RepoKind::Code),
        "hytale-shared-source" => Some(RepoKind::Reference),
        _ => None,
    }
}

fn run_setup(base: &Path, parent: &Path) -> ken_core::Result<()> {
    let p = propose(parent)?;
    let mut rows = p.rows.clone();
    println!("
## Kinds as assigned, set before any indexing
");
    for r in rows.iter_mut() {
        match assigned(&r.member) {
            Some(k) => {
                println!("- {}: proposed {:?}, set {:?}", r.member, r.kind, k);
                r.include = true;
                r.kind = vec![k];
                r.index = IndexState::Search;
                r.team = Some(TEAM.into());
            }
            None => r.include = false,
        }
    }
    let covered: Vec<wikinew::Covered> = rows
        .iter()
        .filter(|r| r.include)
        .map(|r| wikinew::Covered {
            name: r.member.clone(),
            description: r.description.clone(),
            kind: r.kind.clone(),
            path: r.path.clone(),
        })
        .collect();
    let taken: Vec<String> = rows.iter().map(|r| r.member.clone()).collect();
    let wiki_name = format!("{TEAM}-Wiki");
    // No team repo here: the docs repo holds the tickets too, as
    // Shattered-Realms-Docs does.
    let mut wiki = setup::create_wiki_with(&parent.join(&wiki_name), TEAM, &covered, &taken, "2026-10-06", true)?;
    wiki.team = Some(TEAM.into());
    rows.push(wiki);
    // Prefab block lists: 453,955 of the platform repo's 519,165 chunks
    // (measured 2026-10-05), 1,993 MB in 8,094 files. Left out before reading.
    let mut ignores = p.ignores.clone();
    ignores.push(setup::IgnoreRow {
        pattern: "hytale-shared-source/HytaleAssets/Server/Prefabs/".into(),
        state: IndexState::Off,
        reason: "Prefab block lists".into(),
        evidence: "1,993 MB in 8,094 files".into(),
        ticked: true,
        fixed: false,
    });
    let ws = setup::confirm(base, parent, TEAM, &rows, &ignores, "2026-10-06")?;
    println!("
## Confirmed `{}`
", ws.config.name);
    let reg = Registry::load(base)?;
    for m in &ws.members {
        let root = parent.join(&m.name);
        let e = reg.entry_at(&root);
        println!(
            "- **{}** kind={:?} index={:?} team={:?}",
            m.name,
            e.map(|e| e.kind.clone()).unwrap_or_default(),
            e.and_then(|e| e.index),
            e.and_then(|e| e.team.clone()),
        );
    }
    let ki = parent.join(".kenignore");
    println!("
## `.kenignore` written

```
{}
```", std::fs::read_to_string(&ki).unwrap_or_else(|_| "(none)".into()));
    Ok(())
}
