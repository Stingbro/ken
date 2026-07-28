//! ken-mcp — stdio MCP server over ken-core. Hand-rolled JSON-RPC 2.0:
//! one JSON object per line on stdout, stdin read line-by-line, nothing
//! but protocol on stdout (diagnostics go to stderr). Read-only on the
//! SQLite index by construction — the server never writes anything.
//!
//! Scoping: `ken-mcp --project <path>` locks every tool to that project;
//! unscoped, the project tools take a required `project` argument matched
//! against Ken's registry.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use uuid::Uuid;

use ken_core::db::Db;
use ken_core::features;
use ken_core::profiler::{self, ProjectProfile};
use ken_core::project::Project;
use ken_core::registry::{self, Registry};
use ken_core::routing::{self, MemberHits, MemberInfo, MemberStatus, RouteReason};
use ken_core::search::Source;
use ken_core::settings::AppSettings;
use ken_core::workspace_kg_db::WorkspaceKgDb;

/// The protocol revision we implement. Newer revisions we know to be
/// wire-compatible with our subset are echoed back on request.
const PROTOCOL_VERSION: &str = "2024-11-05";
const KNOWN_PROTOCOL_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18"];

/// Cap on `read_document` output.
const MAX_DOCUMENT_BYTES: usize = 200 * 1024;

struct Server {
    base_dir: PathBuf,
    /// Root the server is locked to (`--project <path>`), if any.
    scoped: Option<PathBuf>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut scoped = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--project" => match args.next() {
                Some(path) => scoped = Some(PathBuf::from(path)),
                None => {
                    eprintln!("ken-mcp: --project needs a path");
                    std::process::exit(2);
                }
            },
            "--version" => {
                eprintln!("ken-mcp {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            other => {
                eprintln!("ken-mcp: unknown argument {other:?}\nusage: ken-mcp [--project <path>]");
                std::process::exit(2);
            }
        }
    }

    let base_dir = match registry::default_base_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("ken-mcp: {e}");
            std::process::exit(1);
        }
    };
    let mut server = Server { base_dir, scoped };

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if let Some(reply) = handle_line(&mut server, &line) {
            let mut out = stdout.lock();
            if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
                break; // client hung up
            }
        }
    }
}

/// One line in, at most one line out. Malformed input is answered (it has
/// no id to be a notification) and never kills the loop.
fn handle_line(server: &mut Server, line: &str) -> Option<String> {
    if line.trim().is_empty() {
        return None;
    }
    let reply = match serde_json::from_str::<Value>(line) {
        Ok(request) => handle_request(server, &request)?,
        Err(e) => rpc_error(&Value::Null, -32700, &format!("parse error: {e}")),
    };
    Some(reply.to_string())
}

/// Route one decoded JSON-RPC message. `None` means "no reply" — the rule
/// for notifications (no id), including unknown ones.
fn handle_request(server: &mut Server, request: &Value) -> Option<Value> {
    let Some(obj) = request.as_object() else {
        return Some(rpc_error(&Value::Null, -32600, "request must be a JSON object"));
    };
    let id = obj.get("id").cloned();
    let is_notification = matches!(id, None | Some(Value::Null));
    let id = id.unwrap_or(Value::Null);

    let Some(method) = obj.get("method").and_then(|m| m.as_str()) else {
        if is_notification {
            return None;
        }
        return Some(rpc_error(&id, -32600, "missing method"));
    };
    let params = obj.get("params").cloned().unwrap_or(Value::Null);

    let reply = match method {
        "initialize" => rpc_result(&id, initialize_result(&params)),
        "ping" => rpc_result(&id, json!({})),
        "tools/list" => rpc_result(&id, json!({ "tools": tool_definitions(server) })),
        "tools/call" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match name {
                "search_knowledge" | "read_document" | "list_documents" | "list_projects"
                | "kg_search" | "semantic_search" | "route_query" => {
                    let outcome = call_tool(server, name, &args);
                    rpc_result(&id, tool_content(outcome))
                }
                _ => rpc_error(&id, -32602, &format!("unknown tool: {name:?}")),
            }
        }
        _ => {
            if is_notification {
                return None; // e.g. notifications/initialized
            }
            rpc_error(&id, -32601, &format!("method not found: {method}"))
        }
    };
    if is_notification {
        return None;
    }
    Some(reply)
}

fn initialize_result(params: &Value) -> Value {
    let requested = params
        .get("protocolVersion")
        .and_then(|v| v.as_str())
        .unwrap_or(PROTOCOL_VERSION);
    let version = if KNOWN_PROTOCOL_VERSIONS.contains(&requested) {
        requested
    } else {
        PROTOCOL_VERSION
    };
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "ken", "version": env!("CARGO_PKG_VERSION") }
    })
}

fn rpc_result(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Tool results are text content; tool failures are `isError` content so
/// the calling model sees the explanation and can self-correct.
fn tool_content(outcome: Result<String, String>) -> Value {
    match outcome {
        Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
        Err(text) => json!({
            "content": [{ "type": "text", "text": text }],
            "isError": true
        }),
    }
}

/// kg-routing (task 3.4) adds `kg_search`/`semantic_search`/`route_query` to
/// this list, gated on the `kgRouting` flag (spec: "With kgRouting off
/// these tools SHALL be absent from the tool list"). `list_projects`
/// deliberately does NOT move behind that gate even though design.md names
/// it as one of kg-routing's four new tools — it already existed,
/// unconditionally, before this change (see the final report's
/// "Conflicts"), and removing it when the flag is off would violate task
/// 5.2's "MCP tool list byte-identical to pre-feature behavior". Its
/// *content* still gets flag-scoped enrichment — see `list_projects` below.
fn tool_definitions(server: &Server) -> Value {
    let project_arg = json!({
        "type": "string",
        "description": "Which Ken project to use — a project name or folder \
path from list_projects. Required unless the server was started locked to \
one project."
    });
    let mut tools = vec![
        json!({
            "name": "search_knowledge",
            "description": "Full-text search across a Ken project's indexed \
documents (notes, docs, spreadsheets, PDFs…). Returns ranked hits with the \
file path and a snippet; matched terms are shown in **bold**. All words must \
match; the last word may be a prefix.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search terms." },
                    "limit": { "type": "integer", "description": "Maximum hits to return (default 20)." },
                    "project": project_arg
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "read_document",
            "description": "Read one document from a Ken project by its \
project-relative path (as returned by search_knowledge or list_documents). \
Text files return their raw content (capped at 200 KB); binary formats \
(docx, xlsx, pptx, pdf, images) return the text Ken's indexer extracted.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Project-relative file path." },
                    "project": project_arg
                },
                "required": ["path"]
            }
        }),
        json!({
            "name": "list_documents",
            "description": "List a Ken project's indexed files — path, kind, \
size, and modification time — optionally only those under a folder.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "folder": { "type": "string", "description": "Project-relative folder to list (default: whole project)." },
                    "project": project_arg
                }
            }
        }),
        json!({
            "name": "list_projects",
            "description": "List the Ken projects registered on this machine \
— name, folder path, and whether the folder is currently available. When \
workspace routing (the kgRouting feature flag) is on, each project also \
shows its search readiness and, if it has one, its one-line \
`.ken/index-profile.json` summary.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
    ];

    if kg_routing_enabled(&AppSettings::load(&server.base_dir)) {
        tools.push(json!({
            "name": "kg_search",
            "description": "Search the workspace knowledge graph — entity \
names and summaries merged across every project's own knowledge model — and \
return kg:// ids. Use this to find out which project(s) know about a topic \
before calling semantic_search on one of them directly; for a one-shot \
answer where you don't need to see the entities first, call route_query \
instead. If the federatedKg flag is off, this returns an explanation rather \
than an error — semantic_search and route_query keep working without it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search terms — matched against entity names and summaries, case-insensitive." },
                    "limit": { "type": "integer", "description": "Maximum entities to return (default 50)." }
                },
                "required": ["query"]
            }
        }));
        tools.push(json!({
            "name": "semantic_search",
            "description": "Hybrid keyword+meaning search of one Ken \
project's indexed documents, ranked the same way Ken's own hybrid search \
ranks it. Prefer this over route_query when you already know which project \
to search (e.g. from list_projects, kg_search, or because the user named \
it) — it searches only that project and is faster. This ken-mcp server has \
no embedding model of its own, so there is no fresh query embedding: \
results are keyword (FTS) matches reranked by the hybrid merge logic, not \
true meaning-based matches, regardless of the project's own index \
maturity. Hits carry ken:// addresses.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search terms." },
                    "limit": { "type": "integer", "description": "Maximum hits to return (default 20)." },
                    "project": project_arg
                },
                "required": ["query"]
            }
        }));
        tools.push(json!({
            "name": "route_query",
            "description": "One-shot workspace search: figures out which \
project(s) are relevant — a named project first, then the workspace \
knowledge graph, then broadcasting to every project with a ready semantic \
index — searches each with the same hybrid ranking semantic_search uses, \
and returns one merged, ranked, cited list. Prefer semantic_search instead \
when you already know the target project; use this when you don't. Every \
hit carries a ken:// address, plus kg:// breadcrumbs for the entities that \
picked the target when the knowledge graph did the routing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search terms — also matched against project names for direct routing." },
                    "limit": { "type": "integer", "description": "Maximum merged hits to return (default 20)." }
                },
                "required": ["query"]
            }
        }));
    }

    Value::Array(tools)
}

// --- tools ---

fn call_tool(server: &Server, name: &str, args: &Value) -> Result<String, String> {
    match name {
        "list_projects" => list_projects(server),
        "kg_search" => kg_search(server, args),
        "semantic_search" => semantic_search(server, args),
        "route_query" => route_query(server, args),
        "search_knowledge" => {
            let query = require_str(args, "query")?;
            let limit = args
                .get("limit")
                .and_then(|l| l.as_u64())
                .map(|l| l.clamp(1, 200) as usize)
                .unwrap_or(20);
            let (project, note) = resolve_project(server, args)?;
            let db = open_index(server, &project)?;
            let hits = db
                .search(&query, limit)
                .map_err(|e| format!("search failed: {e}"))?;
            let mut out = note.unwrap_or_default();
            if hits.is_empty() {
                out.push_str(&format!(
                    "No matches for {query:?} in project \"{}\". Try fewer or \
different words — all terms must match.",
                    project.config.name
                ));
            } else {
                out.push_str(&format!(
                    "{} result{} for {query:?} in project \"{}\":\n",
                    hits.len(),
                    if hits.len() == 1 { "" } else { "s" },
                    project.config.name
                ));
                for (i, hit) in hits.iter().enumerate() {
                    let snippet = hit.snippet.replace("<mark>", "**").replace("</mark>", "**");
                    out.push_str(&format!("\n{}. {} — {}", i + 1, hit.rel_path, snippet));
                }
            }
            Ok(out)
        }
        "read_document" => {
            let path = require_str(args, "path")?;
            let (project, note) = resolve_project(server, args)?;
            // Validate before anything else so `..`/absolute paths are
            // refused outright, whatever the index says.
            let abs = project
                .resolve(&path)
                .map_err(|_| format!("{path:?} is outside the project — paths are relative to the project root and may not contain \"..\""))?;
            let db = open_index(server, &project)?;
            let row = db
                .get_file(&path)
                .map_err(|e| format!("index lookup failed: {e}"))?
                .ok_or_else(|| {
                    format!(
                        "{path:?} is not in project \"{}\"'s index. Use \
list_documents or search_knowledge to find valid paths.",
                        project.config.name
                    )
                })?;
            let mut out = note.unwrap_or_default();
            match row.kind.as_str() {
                "md" | "txt" | "code" => {
                    let bytes = std::fs::read(&abs)
                        .map_err(|e| format!("could not read {path:?}: {e}"))?;
                    let truncated = bytes.len() > MAX_DOCUMENT_BYTES;
                    let end = if truncated {
                        floor_char_boundary_at(&bytes, MAX_DOCUMENT_BYTES)
                    } else {
                        bytes.len()
                    };
                    out.push_str(&String::from_utf8_lossy(&bytes[..end]));
                    if truncated {
                        out.push_str("\n\n[truncated — file exceeds the 200 KB read limit]");
                    }
                }
                kind => {
                    let text = db
                        .get_text(&path)
                        .map_err(|e| format!("index lookup failed: {e}"))?
                        .filter(|t| !t.trim().is_empty())
                        .ok_or_else(|| match &row.error {
                            Some(reason) => format!(
                                "Ken could not extract text from {path:?} ({reason})."
                            ),
                            None => format!(
                                "{path:?} is a {kind} file with no extracted \
text in the index (it is indexed by name only)."
                            ),
                        })?;
                    out.push_str(&format!(
                        "[{kind} file — this is the text Ken's indexer \
extracted, not the original bytes]\n\n{text}"
                    ));
                }
            }
            Ok(out)
        }
        "list_documents" => {
            let folder = args
                .get("folder")
                .and_then(|f| f.as_str())
                .map(|f| f.trim_matches('/').to_string())
                .filter(|f| !f.is_empty());
            let (project, note) = resolve_project(server, args)?;
            let db = open_index(server, &project)?;
            let files = db
                .list_files()
                .map_err(|e| format!("index lookup failed: {e}"))?;
            let files: Vec<_> = match &folder {
                Some(f) => files
                    .into_iter()
                    .filter(|row| row.rel_path.starts_with(&format!("{f}/")))
                    .collect(),
                None => files,
            };
            let mut out = note.unwrap_or_default();
            let scope = match &folder {
                Some(f) => format!("under \"{f}\" in project \"{}\"", project.config.name),
                None => format!("in project \"{}\"", project.config.name),
            };
            if files.is_empty() {
                out.push_str(&format!("No indexed files {scope}."));
            } else {
                out.push_str(&format!(
                    "{} indexed file{} {scope} (path — kind, size, modified as unix seconds):\n",
                    files.len(),
                    if files.len() == 1 { "" } else { "s" },
                ));
                for row in files {
                    out.push_str(&format!(
                        "\n{} — {}, {} bytes, modified {}",
                        row.rel_path, row.kind, row.size, row.mtime
                    ));
                }
            }
            Ok(out)
        }
        _ => Err(format!("unknown tool: {name:?}")),
    }
}

fn require_str(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("the {key:?} argument is required"))
}

fn list_projects(server: &Server) -> Result<String, String> {
    let registry = Registry::load(&server.base_dir)
        .map_err(|e| format!("could not read Ken's project registry: {e}"))?;
    let statuses = registry.statuses();
    if statuses.is_empty() {
        return Ok("No Ken projects registered on this machine yet — open a \
folder in the Ken app first."
            .to_string());
    }
    // Readiness + profile enrichment (task 3.1) is gated on `kgRouting` so a
    // flag-off caller's output is byte-identical to this tool's
    // pre-kg-routing behavior (spec 5.2) — see `tool_definitions`'s doc
    // comment on why `list_projects` itself stays unconditionally present
    // rather than moving behind the flag like the other three tools.
    let enrich = kg_routing_enabled(&AppSettings::load(&server.base_dir));
    let mut out = format!(
        "{} Ken project{}:\n",
        statuses.len(),
        if statuses.len() == 1 { "" } else { "s" }
    );
    for s in statuses {
        out.push_str(&format!(
            "\n{} — {}{}",
            s.entry.name,
            s.entry.path.display(),
            if s.available { "" } else { " (folder currently missing)" }
        ));
        if enrich && s.available {
            let readiness = match Db::open_read_only(&server.base_dir, s.entry.id) {
                Ok(db) if db.vec_available() => "keyword + semantic search ready",
                Ok(_) => "keyword search ready",
                Err(_) => "no index yet",
            };
            out.push_str(&format!(" — {readiness}"));
            if profiler::profile_path(&s.entry.path).is_file() {
                let summary = ProjectProfile::load(&s.entry.path).summary;
                let summary = summary.trim();
                if !summary.is_empty() {
                    out.push_str(&format!(" — \"{summary}\""));
                }
            }
        }
    }
    Ok(out)
}

/// Search the workspace knowledge graph (task 3.2). Delegates to the same
/// `WorkspaceKgDb` reads src-tauri's `workspace_kg_search` command uses
/// (D4: "delegate, never reimplement" as far as this layer allows — the
/// name/summary substring match + name-first sort is duplicated from that
/// command's body rather than shared, since no reusable ken-core function
/// for it exists to call instead; see the final report).
fn kg_search(server: &Server, args: &Value) -> Result<String, String> {
    let query = require_str(args, "query")?;
    let limit = args
        .get("limit")
        .and_then(|l| l.as_u64())
        .map(|l| l.clamp(1, 200) as usize)
        .unwrap_or(50);
    let app_settings = AppSettings::load(&server.base_dir);
    if !kg_routing_enabled(&app_settings) {
        return Err("kg_search requires the kgRouting feature flag, which is off.".into());
    }
    if !federated_kg_enabled(&app_settings) {
        return Ok("The workspace knowledge graph is off (enable the \
\"workspace\" and \"federatedKg\" feature flags in Ken's settings) — \
kg_search has nothing to search yet. semantic_search and route_query still \
work without it (route_query just skips straight to broadcasting)."
            .to_string());
    }
    let kg = WorkspaceKgDb::open(&server.base_dir)
        .map_err(|e| format!("could not open the workspace knowledge graph: {e}"))?;
    let q = query.trim().to_lowercase();
    let mut hits: Vec<_> = kg
        .list_global_entities()
        .map_err(|e| format!("knowledge graph read failed: {e}"))?
        .into_iter()
        .filter(|e| e.name.to_lowercase().contains(&q) || e.summary.to_lowercase().contains(&q))
        .collect();
    // Name matches rank above summary-only matches; stable by id within each
    // tier — same ranking as the src-tauri command this mirrors.
    hits.sort_by(|a, b| {
        let a_name = a.name.to_lowercase().contains(&q);
        let b_name = b.name.to_lowercase().contains(&q);
        b_name.cmp(&a_name).then(a.id.cmp(&b.id))
    });
    hits.truncate(limit);
    if hits.is_empty() {
        return Ok(format!("No workspace knowledge-graph entities match {query:?}."));
    }
    let mut out = format!(
        "{} entit{} match {query:?}:\n",
        hits.len(),
        if hits.len() == 1 { "y" } else { "ies" }
    );
    for h in hits {
        out.push_str(&format!("\nkg://{} — {} ({}): {}", h.id, h.name, h.kind, h.summary));
    }
    Ok(out)
}

/// Single-member hybrid search (task 3.3), delegating to
/// `routing::search_member` — the same FTS+KNN+merge_and_rerank composition
/// src-tauri's `hybrid_search` command uses, generalized for reuse
/// (routing.rs module doc). `query_vec` is unconditionally `None`: this
/// build has no embedder (`ken-core = { default-features = false }` in
/// Cargo.toml — ken-mcp is the FTS-only sidecar), and substituting
/// `FakeEmbedder` is the one thing that type's own doc says never to do (it
/// would silently return meaningless hash-based vectors against a real
/// semantic index). `search_member` degrades exactly as designed: keyword
/// (FTS) hits, reranked by hybrid search's own merge logic — honestly
/// short of true semantic search, whatever the project's own index
/// maturity is.
fn semantic_search(server: &Server, args: &Value) -> Result<String, String> {
    let query = require_str(args, "query")?;
    let limit = args
        .get("limit")
        .and_then(|l| l.as_u64())
        .map(|l| l.clamp(1, 200) as usize)
        .unwrap_or(20);
    if !kg_routing_enabled(&AppSettings::load(&server.base_dir)) {
        return Err("semantic_search requires the kgRouting feature flag, \
which is off. Use search_knowledge instead."
            .into());
    }
    let (project, note) = resolve_project(server, args)?;
    let db = open_index(server, &project)?;
    let hits = routing::search_member(&db, &query, None, limit).map_err(|e| format!("search failed: {e}"))?;
    let mut out = note.unwrap_or_default();
    if hits.is_empty() {
        out.push_str(&format!(
            "No matches for {query:?} in project \"{}\".",
            project.config.name
        ));
    } else {
        out.push_str(&format!(
            "{} result{} for {query:?} in project \"{}\" (keyword-ranked — \
this server has no query embedding model; see the tool description):\n",
            hits.len(),
            if hits.len() == 1 { "" } else { "s" },
            project.config.name
        ));
        for (i, hit) in hits.iter().enumerate() {
            out.push_str(&format!(
                "\n{}. {} [{}] {} — {}",
                i + 1,
                ken_address(project.config.id, &hit.path),
                source_label(hit.source),
                hit.path,
                hit.snippet
            ));
        }
    }
    Ok(out)
}

/// Route + fan-out + merge (task 3.4): `routing::plan_route`, then a
/// per-target `routing::search_member`, then `routing::merge_routed`.
///
/// This does NOT call `routing::execute_plan` even though that's the
/// ken-core composition of exactly these three steps — `execute_plan`
/// requires a live `&mut dyn Embedder` to call `.embed_query()`, and (per
/// `semantic_search`'s doc above) this build has none to give it; faking
/// one with `FakeEmbedder` is the one thing that module's doc says not to
/// do. So this is `execute_plan`'s own per-target loop, copied by hand with
/// `query_vec` unconditionally `None` — the "honest adaptation" its module
/// doc explicitly anticipates for a caller in ken-mcp's position, ending in
/// the same pure `merge_routed` call `execute_plan` itself makes.
fn route_query(server: &Server, args: &Value) -> Result<String, String> {
    let query = require_str(args, "query")?;
    let limit = args
        .get("limit")
        .and_then(|l| l.as_u64())
        .map(|l| l.clamp(1, 200) as usize)
        .unwrap_or(20);
    let app_settings = AppSettings::load(&server.base_dir);
    if !kg_routing_enabled(&app_settings) {
        return Err("route_query requires the kgRouting feature flag, which is off.".into());
    }
    let registry = Registry::load(&server.base_dir)
        .map_err(|e| format!("could not read Ken's project registry: {e}"))?;
    if registry.projects.is_empty() {
        return Ok("No Ken projects registered on this machine yet — open a \
folder in the Ken app first."
            .to_string());
    }

    // Open every registered, present project's index once — reused both to
    // build `plan_route`'s `MemberInfo` metadata and, for whichever targets
    // the plan picks, the actual search (avoids opening each DB twice).
    // Missing folders are excluded outright: not real routing candidates.
    let mut members: Vec<MemberInfo> = Vec::new();
    let mut dbs: HashMap<Uuid, Db> = HashMap::new();
    for entry in &registry.projects {
        if !entry.path.is_dir() {
            continue;
        }
        let (index_ready, last_activity) = match Db::open_read_only(&server.base_dir, entry.id) {
            Ok(db) => {
                let ready = db.vec_available();
                // `features/multi-project/README.md`: "recent activity ...
                // the member's most recent ingest completion timestamp" —
                // the existing `ingest_runs` table already tracks exactly
                // this (`Db::runs_with_status`), so no new state is needed.
                // Same computation src-tauri's `route_search` command uses
                // for the same field (lib.rs, `RouteMemberSnapshot`).
                let last_activity = db
                    .runs_with_status("fresh")
                    .ok()
                    .and_then(|rows| rows.iter().filter_map(|r| r.finished_at).max())
                    .unwrap_or(0);
                dbs.insert(entry.id, db);
                (ready, last_activity)
            }
            Err(_) => (false, 0),
        };
        members.push(MemberInfo {
            project_id: entry.id,
            name: entry.name.clone(),
            index_ready,
            last_activity,
        });
    }

    let kg = federated_kg_enabled(&app_settings)
        .then(|| WorkspaceKgDb::open(&server.base_dir).ok())
        .flatten();
    let plan = routing::plan_route(&query, &members, kg.as_ref());

    let mut member_hits: Vec<MemberHits> = Vec::with_capacity(plan.targets.len());
    for &project_id in &plan.targets {
        let Some(info) = members.iter().find(|m| m.project_id == project_id) else {
            member_hits.push(MemberHits {
                project_id,
                member_name: String::new(),
                status: MemberStatus::Unavailable,
                hits: Vec::new(),
            });
            continue;
        };
        if !info.index_ready {
            // Mirrors `execute_plan`: not-ready is skipped and reported, not
            // FTS-searched anyway (routing.rs D5 / spec "Fan-out hybrid
            // search"). `index_ready = db.vec_available()` (below) is
            // exactly the signal src-tauri's own `route_search` command
            // uses for the same field (see its doc comment, lib.rs ~6204:
            // "same readiness signal `hybrid_search` gates its KNN pass
            // on") — matched for cross-surface consistency, not derived
            // independently. Caveat worth flagging: `vec_available()` is a
            // per-connection "did the sqlite-vec extension load" flag
            // (`db.rs`'s `load_vec_extension`), not a per-project "were
            // this project's embeddings actually built" flag — it will
            // read `true` for most/every project's Db in a build where the
            // extension links successfully, regardless of that project's
            // own `semanticIndex` flag or embedding state. That's an
            // existing characteristic of the signal both callers share,
            // not something introduced here.
            member_hits.push(MemberHits {
                project_id,
                member_name: info.name.clone(),
                status: MemberStatus::IndexBuilding,
                hits: Vec::new(),
            });
            continue;
        }
        let Some(db) = dbs.get(&project_id) else {
            member_hits.push(MemberHits {
                project_id,
                member_name: info.name.clone(),
                status: MemberStatus::Unavailable,
                hits: Vec::new(),
            });
            continue;
        };
        match routing::search_member(db, &query, None, limit) {
            Ok(hits) => member_hits.push(MemberHits {
                project_id,
                member_name: info.name.clone(),
                status: MemberStatus::Searched,
                hits,
            }),
            Err(_) => member_hits.push(MemberHits {
                project_id,
                member_name: info.name.clone(),
                status: MemberStatus::Unavailable,
                hits: Vec::new(),
            }),
        }
    }

    let report = routing::merge_routed(&plan, &member_hits, limit);
    Ok(format_execution_report(&report))
}

/// Renders a route_query [`routing::ExecutionReport`] as tool-call text
/// (this server has no structured-content channel — every tool returns
/// prose, matching the rest of this file's style).
fn format_execution_report(report: &routing::ExecutionReport) -> String {
    let reason = match &report.plan.reason {
        RouteReason::Named => "named project match".to_string(),
        RouteReason::KgEntities(ids) => format!(
            "knowledge-graph match ({} entit{})",
            ids.len(),
            if ids.len() == 1 { "y" } else { "ies" }
        ),
        RouteReason::Broadcast => "broadcast — no name or knowledge-graph match".to_string(),
    };
    let mut out = format!(
        "Route: {reason}, {} target project{}.\n",
        report.plan.targets.len(),
        if report.plan.targets.len() == 1 { "" } else { "s" }
    );
    out.push_str("Members: ");
    let member_lines: Vec<String> = report
        .member_status
        .iter()
        .map(|m| {
            let status = match m.status {
                MemberStatus::Searched => "searched",
                MemberStatus::IndexBuilding => "index-building",
                MemberStatus::Unavailable => "unavailable",
            };
            format!("{} ({status})", m.member_name)
        })
        .collect();
    out.push_str(&member_lines.join(", "));
    out.push('\n');

    if report.results.is_empty() {
        out.push_str("\nNo results.");
        return out;
    }
    for (i, hit) in report.results.iter().enumerate() {
        out.push_str(&format!(
            "\n{}. {} [{}, {}] — {}",
            i + 1,
            hit.address,
            hit.member_name,
            source_label(hit.source),
            hit.snippet
        ));
        if !hit.kg_breadcrumbs.is_empty() {
            out.push_str(&format!(" ({})", hit.kg_breadcrumbs.join(" ")));
        }
    }
    out
}

fn source_label(source: Source) -> &'static str {
    match source {
        Source::Keyword => "keyword",
        Source::Semantic => "semantic",
        Source::Both => "keyword+semantic",
    }
}

/// `ken://<project-id>/<rel-path>` — same format as `routing::search_member`'s
/// private sibling in ken-core (`fn ken_address`, not `pub`); duplicated
/// here for this one caller rather than exported (`features/multi-project/
/// README.md` "Cross-feature contracts": "Rel-paths are always forward-slash
/// normalized, on Windows too").
fn ken_address(project_id: Uuid, rel_path: &str) -> String {
    format!("ken://{project_id}/{}", rel_path.replace('\\', "/"))
}

/// Global-layer value of a flag: `settings.json`'s `features` map, else the
/// flag's registered default (false if unregistered — e.g. `kgRouting`
/// before its own registration lands; treating an unrecognized name as off
/// matches `effective_flag`'s behavior for the same case). Deliberately
/// global-only rather than `effective_flag` + a project: `workspace`,
/// `federatedKg`, and `kgRouting` are workspace-/global-scoped flags with no
/// single "the" project to ask when ken-mcp is unscoped — mirrors
/// src-tauri's own dedicated `workspace_enabled`/`federated_kg_enabled`
/// helpers (`lib.rs`), which read the same way for the same reason rather
/// than through `effective_flag`'s project layer.
fn global_flag(app_settings: &AppSettings, name: &str) -> bool {
    let default = features::flag(name).map(|f| f.default).unwrap_or(false);
    app_settings
        .features
        .get(name)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn workspace_enabled(app_settings: &AppSettings) -> bool {
    global_flag(app_settings, "workspace")
}

/// Mirrors src-tauri's `federated_kg_enabled` (proposal: federatedKg
/// "Requires the workspace flag").
fn federated_kg_enabled(app_settings: &AppSettings) -> bool {
    workspace_enabled(app_settings) && global_flag(app_settings, "federatedKg")
}

/// design.md: "kgRouting (workspace-level). Requires semanticIndex and
/// workspace". The `workspace` half is checked the same way
/// `federated_kg_enabled` checks it; `semanticIndex` is per-project, not a
/// single global toggle, so it isn't checked here — it instead shows up as
/// `MemberInfo::index_ready` gating which *members* `route_query` can
/// actually target, which is the only place a per-project flag can mean
/// anything for a workspace-wide gate.
fn kg_routing_enabled(app_settings: &AppSettings) -> bool {
    workspace_enabled(app_settings) && global_flag(app_settings, "kgRouting")
}

/// Which project does this call target? Scoped servers always answer with
/// their own; unscoped servers require the `project` argument.
fn resolve_project(server: &Server, args: &Value) -> Result<(Project, Option<String>), String> {
    let requested = args.get("project").and_then(|p| p.as_str()).map(str::trim);

    if let Some(root) = &server.scoped {
        let project = Project::open(root).map_err(|e| {
            format!("could not open the project this server is locked to ({}): {e}", root.display())
        })?;
        let note = requested.filter(|r| !r.is_empty()).map(|_| {
            format!(
                "Note: this server is locked to project \"{}\" — the \
\"project\" argument was ignored.\n\n",
                project.config.name
            )
        });
        return Ok((project, note));
    }

    let registry = Registry::load(&server.base_dir)
        .map_err(|e| format!("could not read Ken's project registry: {e}"))?;
    let names: Vec<String> = registry.projects.iter().map(|p| p.name.clone()).collect();
    let available = if names.is_empty() {
        "No projects are registered yet — open a folder in the Ken app first.".to_string()
    } else {
        format!("Available projects: {}.", names.join(", "))
    };

    let Some(requested) = requested.filter(|r| !r.is_empty()) else {
        return Err(format!(
            "This server is not locked to a project, so the \"project\" \
argument is required (a name or folder path). {available}"
        ));
    };

    let entry = registry
        .projects
        .iter()
        .find(|p| {
            p.name.eq_ignore_ascii_case(requested)
                || p.path == Path::new(requested)
                || same_canonical(&p.path, Path::new(requested))
        })
        .ok_or_else(|| format!("No Ken project matches {requested:?}. {available}"))?;
    let project = Project::open(&entry.path)
        .map_err(|e| format!("could not open project \"{}\": {e}", entry.name))?;
    Ok((project, None))
}

fn same_canonical(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn open_index(server: &Server, project: &Project) -> Result<Db, String> {
    Db::open_read_only(&server.base_dir, project.config.id).map_err(|_| {
        format!(
            "Project \"{}\" has no index yet — open it in the Ken app once \
so Ken can build it.",
            project.config.name
        )
    })
}

/// Largest byte offset ≤ `at` that is a UTF-8 character boundary.
fn floor_char_boundary_at(bytes: &[u8], at: usize) -> usize {
    let mut end = at.min(bytes.len());
    while end > 0 && end < bytes.len() && (bytes[end] & 0b1100_0000) == 0b1000_0000 {
        end -= 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;
    use ken_core::scan;

    /// A registered, indexed project in a tempdir KEN_DATA_DIR — the same
    /// shape the app leaves behind.
    struct Fixture {
        _base: tempfile::TempDir,
        _root: tempfile::TempDir,
        server: Server,
        root: PathBuf,
    }

    fn fixture(scoped: bool) -> Fixture {
        let base = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("notes")).unwrap();
        std::fs::write(
            root.path().join("notes/meeting.md"),
            "# Meeting\nConfirmed the billing cutover date slips to Sept 12.\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join("People.md"),
            "Priya Natarajan owns billing cutover with Marcus as backup.\n",
        )
        .unwrap();
        let project = Project::create(root.path(), "Atlas").unwrap();
        let mut db = Db::open(base.path(), project.config.id).unwrap();
        scan::scan(&project, &mut db).unwrap();
        let mut registry = Registry::default();
        registry.add(&project);
        registry.save(base.path()).unwrap();

        let root_path = root.path().to_path_buf();
        Fixture {
            server: Server {
                base_dir: base.path().to_path_buf(),
                scoped: scoped.then(|| root_path.clone()),
            },
            root: root_path,
            _base: base,
            _root: root,
        }
    }

    fn call(server: &mut Server, raw: &str) -> Option<Value> {
        handle_line(server, raw).map(|s| serde_json::from_str(&s).unwrap())
    }

    fn tool(server: &mut Server, name: &str, args: Value) -> (String, bool) {
        let req = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": name, "arguments": args }
        });
        let reply = handle_request(server, &req).unwrap();
        let result = &reply["result"];
        (
            result["content"][0]["text"].as_str().unwrap().to_string(),
            result["isError"].as_bool().unwrap_or(false),
        )
    }

    #[test]
    fn initialize_echoes_known_versions_only() {
        let mut fx = fixture(true);
        let reply = call(
            &mut fx.server,
            r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        )
        .unwrap();
        assert_eq!(reply["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(reply["result"]["serverInfo"]["name"], "ken");

        let reply = call(
            &mut fx.server,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2099-01-01"}}"#,
        )
        .unwrap();
        assert_eq!(reply["result"]["protocolVersion"], PROTOCOL_VERSION);
    }

    #[test]
    fn parse_tolerance_and_routing() {
        let mut fx = fixture(true);
        // Malformed line → -32700, and the server keeps answering.
        let reply = call(&mut fx.server, "this is not json").unwrap();
        assert_eq!(reply["error"]["code"], -32700);
        // Non-object JSON → -32600.
        let reply = call(&mut fx.server, "[1,2,3]").unwrap();
        assert_eq!(reply["error"]["code"], -32600);
        // Blank lines are ignored.
        assert!(call(&mut fx.server, "   ").is_none());
        // Notifications — known or unknown — never get replies.
        assert!(call(&mut fx.server, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
        assert!(call(&mut fx.server, r#"{"jsonrpc":"2.0","method":"no/such/notification"}"#).is_none());
        // Unknown method with an id → -32601.
        let reply = call(&mut fx.server, r#"{"jsonrpc":"2.0","id":7,"method":"resources/list"}"#).unwrap();
        assert_eq!(reply["error"]["code"], -32601);
        assert_eq!(reply["id"], 7);
        // Ping still works after all that.
        let reply = call(&mut fx.server, r#"{"jsonrpc":"2.0","id":8,"method":"ping"}"#).unwrap();
        assert_eq!(reply["result"], json!({}));
        // Unknown tool → invalid params, a protocol error.
        let reply = call(
            &mut fx.server,
            r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"write_document"}}"#,
        )
        .unwrap();
        assert_eq!(reply["error"]["code"], -32602);
    }

    #[test]
    fn tools_list_names_four_tools() {
        let mut fx = fixture(true);
        let reply = call(&mut fx.server, r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).unwrap();
        let tools = reply["result"]["tools"].as_array().unwrap();
        let names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            ["search_knowledge", "read_document", "list_documents", "list_projects"]
        );
        for t in tools {
            assert!(t["inputSchema"]["type"] == "object", "schema for {}", t["name"]);
        }
    }

    #[test]
    fn scoped_tools_work_and_ignore_project_arg() {
        let mut fx = fixture(true);
        let (text, is_err) = tool(&mut fx.server, "search_knowledge", json!({"query": "billing cutover"}));
        assert!(!is_err, "{text}");
        assert!(text.contains("notes/meeting.md"), "{text}");
        assert!(text.contains("**billing**"), "{text}");
        assert!(!text.contains("<mark>"), "{text}");

        // A supplied project arg is ignored with a note.
        let (text, is_err) =
            tool(&mut fx.server, "search_knowledge", json!({"query": "billing", "project": "Other"}));
        assert!(!is_err);
        assert!(text.contains("locked to project \"Atlas\""), "{text}");

        let (text, is_err) = tool(&mut fx.server, "read_document", json!({"path": "People.md"}));
        assert!(!is_err);
        assert!(text.contains("Priya Natarajan"), "{text}");

        let (text, is_err) = tool(&mut fx.server, "list_documents", json!({"folder": "notes"}));
        assert!(!is_err);
        assert!(text.contains("notes/meeting.md"), "{text}");
        assert!(!text.contains("People.md"), "{text}");
    }

    #[test]
    fn read_document_refuses_escapes() {
        let mut fx = fixture(true);
        for path in ["../etc/passwd", "/etc/passwd", "notes/../../etc/passwd"] {
            let (text, is_err) = tool(&mut fx.server, "read_document", json!({"path": path}));
            assert!(is_err, "{path} should be refused");
            assert!(text.contains("outside the project"), "{text}");
        }
        // And an unknown-but-safe path is a helpful index miss.
        let (text, is_err) = tool(&mut fx.server, "read_document", json!({"path": "nope.md"}));
        assert!(is_err);
        assert!(text.contains("not in project"), "{text}");
    }

    #[test]
    fn read_document_caps_large_files() {
        let mut fx = fixture(true);
        let big = "é".repeat(150 * 1024); // 300 KB of two-byte chars
        std::fs::write(fx.root.join("big.txt"), &big).unwrap();
        let project = Project::open(&fx.root).unwrap();
        let mut db = Db::open(&fx.server.base_dir, project.config.id).unwrap();
        scan::scan(&project, &mut db).unwrap();
        drop(db);

        let (text, is_err) = tool(&mut fx.server, "read_document", json!({"path": "big.txt"}));
        assert!(!is_err, "{}", &text[..200.min(text.len())]);
        assert!(text.contains("[truncated"), "no truncation note");
        assert!(text.len() < 210 * 1024, "way over cap: {}", text.len());
    }

    #[test]
    fn unscoped_requires_and_matches_project() {
        let mut fx = fixture(false);
        // Missing project → helpful error naming projects.
        let (text, is_err) = tool(&mut fx.server, "search_knowledge", json!({"query": "billing"}));
        assert!(is_err);
        assert!(text.contains("Available projects: Atlas"), "{text}");

        // Unknown project → same courtesy.
        let (text, is_err) =
            tool(&mut fx.server, "search_knowledge", json!({"query": "billing", "project": "Zeus"}));
        assert!(is_err);
        assert!(text.contains("No Ken project matches"), "{text}");

        // Match by name (case-insensitive) and by path.
        let (text, is_err) =
            tool(&mut fx.server, "search_knowledge", json!({"query": "billing", "project": "atlas"}));
        assert!(!is_err, "{text}");
        assert!(text.contains("notes/meeting.md"));
        let root = fx.root.to_string_lossy().to_string();
        let (text, is_err) =
            tool(&mut fx.server, "list_documents", json!({"project": root}));
        assert!(!is_err, "{text}");
        assert!(text.contains("People.md"));

        // list_projects works without any scoping.
        let (text, is_err) = tool(&mut fx.server, "list_projects", json!({}));
        assert!(!is_err);
        assert!(text.contains("Atlas"), "{text}");
    }

    #[test]
    fn missing_required_args_are_tool_errors() {
        let mut fx = fixture(true);
        let (text, is_err) = tool(&mut fx.server, "search_knowledge", json!({}));
        assert!(is_err);
        assert!(text.contains("\"query\" argument is required"), "{text}");
        let (text, is_err) = tool(&mut fx.server, "read_document", json!({}));
        assert!(is_err);
        assert!(text.contains("\"path\" argument is required"), "{text}");
    }

    #[test]
    fn char_boundary_floor_is_safe() {
        let bytes = "aé".as_bytes(); // 61 C3 A9
        assert_eq!(floor_char_boundary_at(bytes, 3), 3);
        assert_eq!(floor_char_boundary_at(bytes, 2), 1);
        assert_eq!(floor_char_boundary_at(bytes, 99), 3);
        assert_eq!(floor_char_boundary_at(b"", 5), 0);
    }

    // --- kg-routing (task 3.5) ---

    /// Builds a real (if tiny) semantic index with `FakeEmbedder` — the same
    /// production `engine::rebuild_semantic_index` src-tauri's semantic-index
    /// command uses, just with the deterministic no-model embedder tests get
    /// everywhere else in ken-core. Needed because `routing::MemberInfo::
    /// index_ready`/`execute_plan` gate *execution* on `db.vec_available()`
    /// (mirrored by `route_query` below), so an FTS-only fixture Db would
    /// make every route_query test see every member as `index-building` and
    /// return zero hits — not what task 3.5 needs to test the happy path.
    fn index_semantically(project: &Project, db: &mut Db) {
        let mut embedder = ken_core::embedder::FakeEmbedder::new();
        ken_core::engine::rebuild_semantic_index(
            project,
            db,
            &mut embedder,
            &ken_core::runner::CancelToken::new(),
            |_, _| {},
        )
        .unwrap();
    }

    /// Two registered, indexed (FTS + fake-semantic) projects sharing one
    /// base dir, unscoped, with `workspace`+`kgRouting` on — what the
    /// flag-on tool-visibility and per-project-isolation tests below need.
    /// Tempdirs are returned so the caller keeps them alive for the test's
    /// duration (same idiom as `Fixture`'s `_base`/`_root` fields).
    fn two_project_fixture() -> (tempfile::TempDir, tempfile::TempDir, tempfile::TempDir, Server) {
        let base = tempfile::tempdir().unwrap();
        let root_a = tempfile::tempdir().unwrap();
        let root_b = tempfile::tempdir().unwrap();
        std::fs::write(root_a.path().join("a.md"), "Atlas notes about the launch schedule.\n").unwrap();
        std::fs::write(root_b.path().join("b.md"), "Zeus notes about the launch schedule.\n").unwrap();
        let project_a = Project::create(root_a.path(), "Atlas").unwrap();
        let project_b = Project::create(root_b.path(), "Zeus").unwrap();
        let mut db_a = Db::open(base.path(), project_a.config.id).unwrap();
        scan::scan(&project_a, &mut db_a).unwrap();
        index_semantically(&project_a, &mut db_a);
        let mut db_b = Db::open(base.path(), project_b.config.id).unwrap();
        scan::scan(&project_b, &mut db_b).unwrap();
        index_semantically(&project_b, &mut db_b);
        let mut registry = Registry::default();
        registry.add(&project_a);
        registry.add(&project_b);
        registry.save(base.path()).unwrap();

        let mut settings = AppSettings::default();
        settings.features.insert("workspace".into(), true.into());
        settings.features.insert("kgRouting".into(), true.into());
        settings.save(base.path()).unwrap();

        let server = Server {
            base_dir: base.path().to_path_buf(),
            scoped: None,
        };
        (base, root_a, root_b, server)
    }

    #[test]
    fn kg_routing_tools_absent_and_erroring_when_flag_off() {
        let mut fx = fixture(true); // default settings — kgRouting unset, off
        let reply = call(&mut fx.server, r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).unwrap();
        let names: Vec<_> = reply["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            names,
            ["search_knowledge", "read_document", "list_documents", "list_projects"],
            "flag off must be byte-identical to pre-kg-routing tool list"
        );

        // Dispatch still recognizes the tool names (not "unknown tool") and
        // explains the flag rather than erroring opaquely.
        for name in ["kg_search", "semantic_search", "route_query"] {
            let (text, is_err) = tool(&mut fx.server, name, json!({"query": "billing"}));
            assert!(is_err, "{name}: {text}");
            assert!(text.contains("kgRouting"), "{name}: {text}");
        }
    }

    #[test]
    fn kg_routing_tools_appear_and_route_query_returns_a_named_plan_when_flag_on() {
        let (_base, _ra, _rb, mut server) = two_project_fixture();
        let reply = call(&mut server, r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#).unwrap();
        let tools = reply["result"]["tools"].as_array().unwrap();
        let names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            [
                "search_knowledge",
                "read_document",
                "list_documents",
                "list_projects",
                "kg_search",
                "semantic_search",
                "route_query"
            ]
        );
        for t in tools {
            assert_eq!(t["inputSchema"]["type"], "object", "schema for {}", t["name"]);
            assert!(t["description"].as_str().is_some_and(|d| !d.is_empty()), "{}", t["name"]);
        }

        // "Atlas" names a member, so this is a Named-tier plan targeting
        // only Atlas — no KG lookup, no broadcast.
        let (text, is_err) = tool(&mut server, "route_query", json!({"query": "Atlas launch schedule"}));
        assert!(!is_err, "{text}");
        assert!(text.starts_with("Route: named project match"), "{text}");
        assert!(text.contains("a.md"), "{text}");
        assert!(!text.contains("b.md"), "{text}");
    }

    #[test]
    fn semantic_search_with_explicit_project_touches_only_that_member() {
        let (_base, _ra, _rb, mut server) = two_project_fixture();
        let (text, is_err) = tool(
            &mut server,
            "semantic_search",
            json!({"query": "launch schedule", "project": "Atlas"}),
        );
        assert!(!is_err, "{text}");
        assert!(text.contains("project \"Atlas\""), "{text}");
        assert!(text.contains("a.md"), "{text}");
        assert!(!text.contains("b.md"), "{text}");
        assert!(text.contains("ken://"), "{text}");

        let (text, is_err) = tool(
            &mut server,
            "semantic_search",
            json!({"query": "launch schedule", "project": "Zeus"}),
        );
        assert!(!is_err, "{text}");
        assert!(text.contains("project \"Zeus\""), "{text}");
        assert!(text.contains("b.md"), "{text}");
        assert!(!text.contains("a.md"), "{text}");
    }

    #[test]
    fn kg_search_reports_federated_kg_off_without_erroring() {
        // two_project_fixture turns on workspace+kgRouting but not
        // federatedKg — kg_search should explain, not fail the call.
        let (_base, _ra, _rb, mut server) = two_project_fixture();
        let (text, is_err) = tool(&mut server, "kg_search", json!({"query": "anything"}));
        assert!(!is_err, "{text}");
        assert!(text.contains("federatedKg"), "{text}");
    }

    #[test]
    fn list_projects_enriches_only_when_kg_routing_is_on() {
        let mut fx = fixture(true);
        let (text_off, is_err) = tool(&mut fx.server, "list_projects", json!({}));
        assert!(!is_err);
        assert!(!text_off.contains("search ready"), "{text_off}");

        let mut settings = AppSettings::default();
        settings.features.insert("workspace".into(), true.into());
        settings.features.insert("kgRouting".into(), true.into());
        settings.save(&fx.server.base_dir).unwrap();

        let (text_on, is_err) = tool(&mut fx.server, "list_projects", json!({}));
        assert!(!is_err, "{text_on}");
        assert!(text_on.contains("search ready"), "{text_on}");
    }
}
