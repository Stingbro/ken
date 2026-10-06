//! Pure text chunking for the semantic index. No I/O, no database access —
//! this module only turns already-read file text into a list of [`Chunk`]s
//! according to an [`IndexProfile`]. Callers (the ingest pipeline, `db.rs`)
//! are responsible for reading files and persisting the result.
//!
//! Two chunking strategies:
//!
//! * **Prose** — heading/paragraph-aware. Markdown ATX headings (`# ...`)
//!   always start a new chunk; otherwise paragraphs (blank-line separated)
//!   are packed up to `target_tokens`, with `overlap_pct` of the previous
//!   chunk's tail carried into the next chunk so a search hit near a chunk
//!   boundary still has surrounding context.
//! * **Code** — line-based blocks of ~`target_tokens`, no overlap. Splitting
//!   mid-statement is acceptable; the goal is bounded, roughly-uniform chunks
//!   for embedding, not syntactic correctness.
//!
//! Token counts are estimated as `chars / 4` — cheap and stable. Exactness
//! doesn't matter here, only consistency between index time and any future
//! re-chunking. `content_hash` is an xxHash64 of the chunk text, used by
//! `db::upsert_chunks` to diff against the previous chunk set so an unchanged
//! file costs zero re-embeddings.

use serde::{Deserialize, Serialize};

/// Chunking strategy for a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkMode {
    /// Heading/paragraph-aware splitting with overlap. Markdown, plain text,
    /// extracted PDF text.
    Prose,
    /// Line-based fixed-size blocks, no overlap. Source code and other
    /// structured/dense text.
    Code,
    /// Never chunked or embedded. The file still keyword-searches (that's
    /// driven by kenignore's tier, not this), but produces zero semantic
    /// chunks — used by project-profiler pattern entries (e.g. `*.log`) to
    /// opt noisy/generated text out of embedding without excluding it
    /// entirely.
    Skip,
}

/// How a file should be split into chunks. Per-path selection lives in
/// [`IndexProfile::default_for`]; callers may also construct one directly to
/// override the default (e.g. a future user setting).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IndexProfile {
    pub mode: ChunkMode,
    /// Target chunk size, in estimated tokens (`chars / 4`).
    pub target_tokens: usize,
    /// Fraction (0.0-1.0) of `target_tokens` worth of trailing text from a
    /// chunk that is repeated at the start of the next chunk. Only applies
    /// to prose mode; code mode uses 0.0.
    pub overlap_pct: f32,
}

impl Default for IndexProfile {
    fn default() -> Self {
        IndexProfile {
            mode: ChunkMode::Prose,
            target_tokens: 350,
            overlap_pct: 0.15,
        }
    }
}

/// Prose-ish extensions: markdown, plain text, extracted PDF text. Module
/// level (not just local to `default_for`) so other modules can classify by
/// the same taxonomy instead of duplicating it — project-profiler's doc-ratio
/// scan and default chunking table reuse this exact list.
pub const PROSE_EXTS: &[&str] = &["md", "mdx", "txt", "pdf"];
/// Source-code and other structured/dense-text extensions. See [`PROSE_EXTS`].
pub const CODE_EXTS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts", "py", "go", "java", "c", "h", "cc",
    "cpp", "hpp", "cs", "rb", "php", "swift", "kt", "kts", "gradle", "properties", "sql", "sh",
    "bash", "ps1", "toml", "yaml", "yml", "json", "css", "scss", "html", "svelte", "vue", "ui",
    "lang",
];

impl IndexProfile {
    /// v1 default profile selection by file extension: prose-ish formats
    /// (markdown, plain text, extracted PDF text) get the prose profile;
    /// source-code extensions get the code profile; anything else falls back
    /// to prose defaults.
    pub fn default_for(rel_path: &str) -> IndexProfile {
        let ext = rel_path
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if PROSE_EXTS.contains(&ext.as_str()) {
            IndexProfile {
                mode: ChunkMode::Prose,
                target_tokens: 350,
                overlap_pct: 0.15,
            }
        } else if CODE_EXTS.contains(&ext.as_str()) {
            IndexProfile {
                mode: ChunkMode::Code,
                target_tokens: 500,
                overlap_pct: 0.0,
            }
        } else {
            IndexProfile::default()
        }
    }
}

/// One chunk of a file's text, ready to be persisted (`db::upsert_chunks`
/// assigns `id`/`path`/`tier`) and embedded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// 0-based position of this chunk within the file.
    pub seq: usize,
    pub text: String,
    /// Estimated token count (`text.len() / 4`, minimum 1).
    pub token_est: usize,
    /// xxHash64 of `text`, hex-encoded. Used for incremental diffing.
    pub content_hash: String,
    /// 1-based line in the file where this chunk's own content starts (not
    /// the overlap carried from the chunk before), so a hit can be cited as
    /// `repo:path:line`.
    pub line: usize,
}

/// Hard cap on chunks per file. Guards against pathological inputs
/// (generated/minified files, huge data dumps) blowing up chunk count and,
/// downstream, embedding cost.
const CHUNK_CAP: usize = 200;

/// Split `text` (the contents of `rel_path`) into chunks per `profile`.
/// Pure and infallible: empty/whitespace-only text yields no chunks.
pub fn chunk_file(rel_path: &str, text: &str, profile: &IndexProfile) -> Vec<Chunk> {
    if text.trim().is_empty() {
        return Vec::new();
    }

    let pieces = match profile.mode {
        ChunkMode::Prose => chunk_prose(text, profile),
        ChunkMode::Code => chunk_code(text, profile),
        ChunkMode::Skip => Vec::new(),
    };
    let mut pieces: Vec<(usize, String)> = pieces.into_iter().filter(|(_, p)| !p.trim().is_empty()).collect();
    pieces.truncate(CHUNK_CAP);
    if profile.mode == ChunkMode::Code && rel_path.to_ascii_lowercase().ends_with(".json") {
        pieces = with_json_headers(rel_path, text, pieces);
    }

    pieces
        .into_iter()
        .take(CHUNK_CAP)
        .enumerate()
        .map(|(seq, (line, text))| {
            let token_est = (text.len() / 4).max(1);
            let content_hash = hash_text(&text);
            Chunk {
                seq,
                text,
                token_est,
                content_hash,
                line,
            }
        })
        .collect()
}

fn hash_text(text: &str) -> String {
    let h = twox_hash::XxHash64::oneshot(0, text.as_bytes());
    format!("{h:016x}")
}

/// Split text into paragraph/heading blocks: blank lines separate
/// paragraphs; any line starting with `#` (markdown ATX heading) is always
/// its own block, regardless of surrounding blank lines. Each block carries
/// the 1-based line it starts on.
fn split_prose_blocks(text: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut current = String::new();
    let mut start = 0usize;

    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current.trim().is_empty() {
                blocks.push((start, current.trim().to_string()));
            }
            current.clear();
            continue;
        }
        if trimmed.starts_with('#') {
            if !current.trim().is_empty() {
                blocks.push((start, current.trim().to_string()));
            }
            current.clear();
            blocks.push((n, trimmed.to_string()));
            continue;
        }
        if current.is_empty() {
            start = n;
        } else {
            current.push('\n');
        }
        current.push_str(line);
    }
    if !current.trim().is_empty() {
        blocks.push((start, current.trim().to_string()));
    }
    blocks
}

/// Take the last `n` bytes of `s`, adjusted forward to the nearest char
/// boundary so the result is always valid UTF-8.
fn tail(s: &str, n: usize) -> String {
    if s.len() <= n {
        return s.to_string();
    }
    let mut start = s.len() - n;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    s[start..].to_string()
}

/// Chunks as (first line of the chunk's own content, text).
fn chunk_prose(text: &str, profile: &IndexProfile) -> Vec<(usize, String)> {
    let target_chars = (profile.target_tokens * 4).max(1);
    let overlap_chars = ((target_chars as f32) * profile.overlap_pct).round() as usize;

    let blocks = split_prose_blocks(text);
    let mut chunks: Vec<(usize, String)> = Vec::new();
    let mut current = String::new();
    // The line of the first block added since the last boundary; the
    // overlap tail carried into a chunk does not move it.
    let mut line: Option<usize> = None;

    for (block_line, block) in blocks {
        let is_heading = block.starts_with('#');
        let would_exceed = !current.is_empty() && current.len() + block.len() + 2 > target_chars;
        // Headings always start a fresh chunk (no merging a new section into
        // the tail of the previous one), but never against an empty buffer.
        let force_boundary = is_heading && !current.is_empty();

        if would_exceed || force_boundary {
            chunks.push((line.unwrap_or(block_line), current.clone()));
            line = None;
            current = if overlap_chars > 0 && !force_boundary {
                tail(&current, overlap_chars)
            } else {
                String::new()
            };
        }

        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(&block);
        line.get_or_insert(block_line);
    }
    if !current.trim().is_empty() {
        chunks.push((line.unwrap_or(1), current));
    }
    chunks
}

/// Chunks as (first line, text).
fn chunk_code(text: &str, profile: &IndexProfile) -> Vec<(usize, String)> {
    let target_chars = (profile.target_tokens * 4).max(1);
    let mut chunks: Vec<(usize, String)> = Vec::new();
    let mut current = String::new();
    let mut start = 1usize;

    for (i, line) in text.lines().enumerate() {
        if !current.is_empty() && current.len() + line.len() + 1 > target_chars {
            chunks.push((start, current.clone()));
            current.clear();
        }
        if current.is_empty() {
            start = i + 1;
        } else {
            current.push('\n');
        }
        current.push_str(line);
    }
    if !current.trim().is_empty() {
        chunks.push((start, current));
    }
    chunks
}

/// Keys whose string values go in a JSON chunk's header: what its numbers
/// belong to.
const JSON_NAME_KEYS: &[&str] = &["$Comment", "Description", "Name"];
/// Longest key outline in a JSON chunk's header, in bytes, cut between keys.
const JSON_KEYS_MAX: usize = 600;
/// Longest list of names in a JSON chunk's header, in bytes, cut between names.
const JSON_NAMES_MAX: usize = 300;

/// Each JSON chunk starts with a short header: the file's path, the key
/// paths it holds (`Weapons[].Tiers[].DamageMin`), and the `$Comment`,
/// `Description` and `Name` strings of it and of the objects it sits in,
/// then its raw text. On 2026-10-06 "base damage ranges for each weapon"
/// could not find data/equipment/WeaponBases.json, whose chunks are keys and
/// numbers only; the header says what the numbers are.
fn with_json_headers(rel_path: &str, text: &str, pieces: Vec<(usize, String)>) -> Vec<(usize, String)> {
    let starts: Vec<usize> = pieces.iter().map(|(line, _)| *line).collect();
    let outline = json_outline(text, &starts);
    pieces
        .into_iter()
        .zip(outline.at_start)
        .map(|((line, body), (enclosing, ancestor_names))| {
            let end = line + body.lines().count().saturating_sub(1);
            let in_chunk = |l: &usize| (line..=end).contains(l);
            let keys = std::iter::once(enclosing)
                .filter(|p| !p.is_empty())
                .chain(outline.keys.iter().filter(|(l, _)| in_chunk(l)).map(|(_, p)| p.clone()));
            let names = ancestor_names
                .into_iter()
                .chain(outline.names.iter().filter(|(l, _)| in_chunk(l)).map(|(_, n)| n.clone()));
            let mut header = format!("{rel_path}\nkeys: {}\n", joined_within(keys, " ", JSON_KEYS_MAX));
            let names = joined_within(names, " · ", JSON_NAMES_MAX);
            if !names.is_empty() {
                header.push_str(&names);
                header.push('\n');
            }
            (line, header + &body)
        })
        .collect()
}

/// Distinct `items` joined by `sep`, stopping before the one that would pass
/// `max` bytes.
fn joined_within(items: impl Iterator<Item = String>, sep: &str, max: usize) -> String {
    let mut out = String::new();
    let mut seen = std::collections::HashSet::new();
    for item in items {
        if item.is_empty() || !seen.insert(item.clone()) {
            continue;
        }
        if !out.is_empty() && out.len() + sep.len() + item.len() > max {
            break;
        }
        if !out.is_empty() {
            out.push_str(sep);
        }
        out.push_str(&item);
    }
    out
}

/// What [`json_outline`] reads from a JSON text.
#[derive(Debug, Default)]
struct JsonOutline {
    /// (line, key path) for every key, in order.
    keys: Vec<(usize, String)>,
    /// (line, value) for every string under a [`JSON_NAME_KEYS`] key.
    names: Vec<(usize, String)>,
    /// For each asked start line: the path of the container open there, and
    /// the names already read in the objects open there.
    at_start: Vec<(String, Vec<String>)>,
}

/// One open object or array while reading JSON.
struct JsonFrame {
    array: bool,
    /// The key this container is the value of.
    key: Option<String>,
    names: Vec<String>,
}

fn json_path(stack: &[JsonFrame], key: Option<&str>) -> String {
    let mut path = String::new();
    for frame in stack {
        if let Some(k) = &frame.key {
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(k);
        }
        if frame.array {
            path.push_str("[]");
        }
    }
    if let Some(k) = key {
        if !path.is_empty() {
            path.push('.');
        }
        path.push_str(k);
    }
    path
}

/// Read a JSON text's keys, line by line, without parsing it into values: a
/// file that is not quite JSON (comments, a trailing comma) still yields
/// what it can. `starts` are the 1-based lines whose surroundings are wanted,
/// in order.
fn json_outline(text: &str, starts: &[usize]) -> JsonOutline {
    let mut out = JsonOutline::default();
    let mut stack: Vec<JsonFrame> = Vec::new();
    // The key whose value comes next, and a string just read that is not yet
    // known to be a key or a value.
    let mut pending_key: Option<String> = None;
    let mut last_string: Option<(usize, String)> = None;
    let mut in_string = false;
    let mut escaped = false;
    let mut buf = String::new();
    let mut line = 1usize;
    let mut next_start = 0usize;
    let snapshot = |stack: &[JsonFrame], line: usize, next_start: &mut usize, at_start: &mut Vec<(String, Vec<String>)>| {
        while *next_start < starts.len() && starts[*next_start] <= line {
            at_start.push((json_path(stack, None), stack.iter().flat_map(|f| f.names.iter().cloned()).collect()));
            *next_start += 1;
        }
    };
    // A string that turned out to be a value: kept when its key is a name.
    let value = |stack: &mut Vec<JsonFrame>, pending_key: &mut Option<String>, last: Option<(usize, String)>, names: &mut Vec<(usize, String)>| {
        let key = pending_key.take();
        let (Some((l, s)), Some(k)) = (last, key) else { return };
        if JSON_NAME_KEYS.iter().any(|n| n.eq_ignore_ascii_case(&k)) && !s.trim().is_empty() {
            if let Some(top) = stack.last_mut() {
                top.names.push(s.clone());
            }
            names.push((l, s));
        }
    };
    snapshot(&stack, line, &mut next_start, &mut out.at_start);
    for c in text.chars() {
        if c == '\n' {
            line += 1;
            if !in_string {
                snapshot(&stack, line, &mut next_start, &mut out.at_start);
            }
        }
        if in_string {
            if escaped {
                escaped = false;
                buf.push(c);
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
                last_string = Some((line, std::mem::take(&mut buf)));
            } else {
                buf.push(c);
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                buf.clear();
            }
            ':' => {
                if let Some((l, key)) = last_string.take() {
                    if stack.last().is_some_and(|f| !f.array) {
                        out.keys.push((l, json_path(&stack, Some(&key))));
                    }
                    pending_key = Some(key);
                }
            }
            '{' | '[' => {
                last_string = None;
                stack.push(JsonFrame { array: c == '[', key: pending_key.take(), names: Vec::new() });
            }
            '}' | ']' => {
                value(&mut stack, &mut pending_key, last_string.take(), &mut out.names);
                stack.pop();
            }
            ',' => value(&mut stack, &mut pending_key, last_string.take(), &mut out.names),
            _ => {}
        }
    }
    // Start lines past the end (a text with no final newline).
    while out.at_start.len() < starts.len() {
        out.at_start.push((String::new(), Vec::new()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_for_picks_prose_for_markdown() {
        let p = IndexProfile::default_for("docs/readme.md");
        assert_eq!(p.mode, ChunkMode::Prose);
        assert_eq!(p.target_tokens, 350);
        assert!((p.overlap_pct - 0.15).abs() < f32::EPSILON);
    }

    #[test]
    fn default_for_picks_code_for_rust() {
        let p = IndexProfile::default_for("crates/ken-core/src/lib.rs");
        assert_eq!(p.mode, ChunkMode::Code);
        assert_eq!(p.target_tokens, 500);
        assert_eq!(p.overlap_pct, 0.0);
    }

    #[test]
    fn skip_mode_produces_no_chunks() {
        let profile = IndexProfile { mode: ChunkMode::Skip, target_tokens: 350, overlap_pct: 0.0 };
        assert!(chunk_file("noisy.log", "line one\nline two\nline three\n", &profile).is_empty());
    }

    #[test]
    fn default_for_falls_back_to_prose_for_unknown_extension() {
        let p = IndexProfile::default_for("weird_file.xyz123");
        assert_eq!(p, IndexProfile::default());
    }

    #[test]
    fn empty_text_yields_no_chunks() {
        let profile = IndexProfile::default();
        assert!(chunk_file("empty.md", "   \n\n  ", &profile).is_empty());
    }

    #[test]
    fn chunking_is_deterministic() {
        let text = "# Heading\n\nSome paragraph text that repeats a fair bit to build up length. "
            .repeat(20);
        let profile = IndexProfile::default_for("notes.md");
        let a = chunk_file("notes.md", &text, &profile);
        let b = chunk_file("notes.md", &text, &profile);
        assert_eq!(a, b);
        assert!(!a.is_empty());
    }

    #[test]
    fn prose_mode_respects_heading_boundaries() {
        let text = "Intro paragraph one.\n\nIntro paragraph two continues the thought.\n\n\
                     # Section Two\n\nBody of section two goes here.";
        let profile = IndexProfile {
            mode: ChunkMode::Prose,
            target_tokens: 350,
            overlap_pct: 0.15,
        };
        let chunks = chunk_file("doc.md", text, &profile);
        // The heading starts its own chunk rather than being folded into the
        // middle of the previous section's chunk.
        assert!(chunks
            .iter()
            .any(|c| c.text.trim_start().starts_with("# Section Two")));
        // And it is not glued onto the tail of the intro paragraphs either.
        assert!(!chunks[0].text.contains("# Section Two"));
    }

    #[test]
    fn prose_mode_produces_overlap_between_adjacent_chunks() {
        let mut text = String::new();
        for i in 0..30 {
            text.push_str(&format!(
                "Paragraph number {i} has some unique filler content padding it out.\n\n"
            ));
        }
        let profile = IndexProfile {
            mode: ChunkMode::Prose,
            target_tokens: 50, // target_chars = 200
            overlap_pct: 0.2,  // overlap_chars = 40
        };
        let chunks = chunk_file("doc.md", &text, &profile);
        assert!(
            chunks.len() >= 2,
            "expected multiple chunks, got {}",
            chunks.len()
        );
        let overlap_chars = 40usize;
        assert!(chunks[0].text.len() >= overlap_chars);
        let expected_tail = &chunks[0].text[chunks[0].text.len() - overlap_chars..];
        assert!(
            chunks[1].text.starts_with(expected_tail),
            "chunk 1 should open with chunk 0's trailing {overlap_chars} chars;\n\
             chunk0={:?}\nchunk1={:?}",
            chunks[0].text,
            chunks[1].text
        );
    }

    #[test]
    fn code_mode_splits_into_line_blocks_without_overlap() {
        let mut text = String::new();
        for i in 0..200 {
            text.push_str(&format!("let x{i} = {i}; // padding line to build length\n"));
        }
        let profile = IndexProfile {
            mode: ChunkMode::Code,
            target_tokens: 50,
            overlap_pct: 0.0,
        };
        let chunks = chunk_file("main.rs", &text, &profile);
        assert!(chunks.len() >= 2, "expected multiple chunks");
        let last_line_of_first = chunks[0].text.lines().last().unwrap();
        let first_line_of_second = chunks[1].text.lines().next().unwrap();
        assert_ne!(
            last_line_of_first, first_line_of_second,
            "code mode must not repeat lines across chunk boundaries"
        );
    }

    /// Each chunk knows the line its own content starts on: the heading's
    /// line for a section, the first new paragraph after an overlap tail,
    /// and the first line of a code block.
    #[test]
    fn chunks_carry_the_line_their_content_starts_on() {
        let text = "Intro.\n\n\n# Section Two\n\nBody of two.\n";
        let profile = IndexProfile { mode: ChunkMode::Prose, target_tokens: 350, overlap_pct: 0.15 };
        let chunks = chunk_file("doc.md", text, &profile);
        assert_eq!(chunks[0].line, 1);
        assert_eq!(chunks[1].line, 4, "the heading's line");

        let mut prose = String::new();
        for i in 0..30 {
            prose.push_str(&format!("Paragraph {i} with filler content to pad it.\n\n"));
        }
        let profile = IndexProfile { mode: ChunkMode::Prose, target_tokens: 50, overlap_pct: 0.2 };
        for c in chunk_file("doc.md", &prose, &profile) {
            // The chunk's first new paragraph is the one on its line.
            let own = prose.lines().nth(c.line - 1).unwrap();
            assert!(c.text.contains(own), "line {} = {own:?} not in {:?}", c.line, c.text);
        }

        let code: String = (0..200).map(|i| format!("let x{i} = {i}; // padding line\n")).collect();
        let profile = IndexProfile { mode: ChunkMode::Code, target_tokens: 50, overlap_pct: 0.0 };
        for c in chunk_file("main.rs", &code, &profile) {
            assert_eq!(code.lines().nth(c.line - 1).unwrap(), c.text.lines().next().unwrap());
        }
    }

    #[test]
    fn chunking_enforces_two_hundred_chunk_cap() {
        let mut text = String::new();
        for i in 0..500 {
            text.push_str(&format!("line {i} of a very large generated file padding content\n"));
        }
        let profile = IndexProfile {
            mode: ChunkMode::Code,
            target_tokens: 5, // tiny target -> far more than 200 blocks pre-cap
            overlap_pct: 0.0,
        };
        let chunks = chunk_file("generated.rs", &text, &profile);
        assert_eq!(chunks.len(), CHUNK_CAP);
        assert_eq!(chunks.last().unwrap().seq, CHUNK_CAP - 1);
    }

    #[test]
    fn content_hash_is_stable_and_reflects_text() {
        let profile = IndexProfile::default();
        let a = chunk_file(
            "a.md",
            "Hello world, this is a stable chunk of prose text.",
            &profile,
        );
        let b = chunk_file(
            "a.md",
            "Hello world, this is a stable chunk of prose text.",
            &profile,
        );
        assert_eq!(a[0].content_hash, b[0].content_hash);

        let c = chunk_file(
            "a.md",
            "Hello world, this is a DIFFERENT chunk of prose text.",
            &profile,
        );
        assert_ne!(a[0].content_hash, c[0].content_hash);
    }

    #[test]
    fn a_json_chunk_says_what_its_numbers_are() {
        let mut text = String::from("{\n  \"$Comment\": \"Base damage per weapon, by tier.\",\n  \"Weapons\": [\n");
        for i in 0..40 {
            text.push_str(&format!(
                "    {{\n      \"Name\": \"Sword {i}\",\n      \"Tiers\": [\n        {{ \"DamageMin\": {i}, \"DamageMax\": {} }}\n      ]\n    }},\n",
                i + 5
            ));
        }
        text.push_str("    { \"Name\": \"Last\" }\n  ]\n}\n");
        let profile = IndexProfile { mode: ChunkMode::Code, target_tokens: 60, overlap_pct: 0.0 };
        let chunks = chunk_file("data/equipment/WeaponBases.json", &text, &profile);
        assert!(chunks.len() > 3, "{}", chunks.len());

        let first = &chunks[0].text;
        assert!(first.starts_with("data/equipment/WeaponBases.json\nkeys: $Comment Weapons Weapons[].Name Weapons[].Tiers"), "{first}");
        assert!(first.contains("Base damage per weapon, by tier."), "{first}");
        // The raw text is still there, after the header.
        assert!(first.contains("\"DamageMin\": 0"), "{first}");

        // A chunk from the middle knows where it sits and what holds it.
        let mid = &chunks[2].text;
        assert!(mid.contains("keys: Weapons[]"), "{mid}");
        assert!(mid.contains("Weapons[].Tiers[].DamageMin"), "{mid}");
        assert!(mid.contains("Base damage per weapon, by tier."), "the file's comment holds every chunk: {mid}");
        // Its line is still the file line its own text starts on, the one
        // after the three header lines (path, keys, names).
        assert_eq!(mid.lines().nth(3).unwrap(), text.lines().nth(chunks[2].line - 1).unwrap());
    }

    #[test]
    fn json_outline_reads_keys_names_and_what_is_open() {
        let text = "{\n \"Bases\": {\n  \"Sword_T1\": {\n   \"MinLow\": 4,\n   \"Name\": \"Stone \\\"Crude\\\" sword\"\n  }\n },\n \"List\": [1, 2, \"x\"]\n}\n";
        let o = json_outline(text, &[1, 4, 8]);
        let keys: Vec<&str> = o.keys.iter().map(|(_, k)| k.as_str()).collect();
        assert_eq!(keys, ["Bases", "Bases.Sword_T1", "Bases.Sword_T1.MinLow", "Bases.Sword_T1.Name", "List"]);
        assert_eq!(o.names, vec![(5, "Stone \"Crude\" sword".to_string())]);
        assert_eq!(o.at_start[0], (String::new(), vec![]));
        assert_eq!(o.at_start[1].0, "Bases.Sword_T1");
        assert_eq!(o.at_start[2].0, "", "back at the top by line 8");
        // Not JSON at all: no keys, no panic, one answer per start line.
        let o = json_outline("not { json [ at \" all", &[1, 9]);
        assert!(o.keys.is_empty() && o.at_start.len() == 2);
    }

    #[test]
    fn only_json_gets_a_header() {
        let profile = IndexProfile::default_for("a.yaml");
        let chunks = chunk_file("a.yaml", "Name: x\nDamage: 4\n", &profile);
        assert_eq!(chunks[0].text, "Name: x\nDamage: 4");
    }

    #[test]
    fn token_estimate_is_len_over_four() {
        let profile = IndexProfile::default();
        let text = "0123456789"; // 10 chars
        let chunks = chunk_file("a.txt", text, &profile);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].token_est, 10 / 4);
    }
}
