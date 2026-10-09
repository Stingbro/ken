//! Small pure helpers for user-driven file operations in the Files tree
//! (create / rename / move, §12). Pure so the naming and safety policies are
//! unit-tested without a filesystem; the Tauri commands are thin shells.

/// Pick a document name that doesn't collide: `Untitled.md` → `Untitled 2.md`
/// → `Untitled 3.md` (space + counter, before the extension — the §12 style;
/// imports keep their own `report (2).pdf` style in `import.rs`). `exists` is
/// injected so the policy tests without disk.
pub fn numbered_name(desired: &str, exists: impl Fn(&str) -> bool) -> String {
    if !exists(desired) {
        return desired.to_string();
    }
    let (stem, ext) = split_stem_ext(desired);
    let mut n = 2u32;
    loop {
        let candidate = format!("{stem} {n}{ext}");
        if !exists(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Split a file name into (stem, extension-including-dot). The extension is the
/// final `.suffix` only when a non-empty stem precedes it, so a dotfile like
/// `.env` stays whole and the counter appends after it.
fn split_stem_ext(file_name: &str) -> (&str, &str) {
    match file_name.rfind('.') {
        Some(i) if i > 0 => (&file_name[..i], &file_name[i..]),
        _ => (file_name, ""),
    }
}

/// Make a pasted or dropped file's name safe to create inside a folder: only
/// the last path component survives (no `/`, `\`, or `..` climbing out),
/// control characters go, and leading dots are stripped so an attachment never
/// lands as a hidden file. Falls back to `attachment` when nothing is left.
pub fn sanitize_attachment_name(name: &str) -> String {
    let leaf = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = leaf.chars().filter(|c| !c.is_control()).collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim();
    if cleaned.is_empty() {
        "attachment".to_string()
    } else {
        cleaned.to_string()
    }
}

/// The `n`th candidate name for an attachment: the name itself, then
/// `shot-1.png`, `shot-2.png`… (hyphen + counter before the extension, the
/// usual style for screenshots and downloads, unlike `numbered_name`).
pub fn numbered_attachment_name(name: &str, n: u32) -> String {
    if n == 0 {
        return name.to_string();
    }
    let (stem, ext) = split_stem_ext(name);
    format!("{stem}-{n}{ext}")
}

/// Create a new file named after `desired` (sanitized) inside `dir`, never
/// overwriting: each candidate is opened with `create_new`, so two saves racing
/// for one name each get their own file. Returns the open file and the name it
/// was created under.
pub fn create_unique_file(
    dir: &std::path::Path,
    desired: &str,
) -> std::io::Result<(std::fs::File, String)> {
    let name = sanitize_attachment_name(desired);
    for n in 0..10_000 {
        let candidate = numbered_attachment_name(&name, n);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(&candidate))
        {
            Ok(file) => return Ok((file, candidate)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!("too many files named like {name}"),
    ))
}

/// Create a NEW folder named after `desired` (sanitized like an attachment)
/// inside `dir`: `Photos`, then `Photos-1`, `Photos-2`… The name is taken
/// whole (a folder has no extension), and `create_dir` fails atomically on an
/// existing entry, so a dropped folder is never merged into one already there.
/// Returns the name it was created under.
pub fn create_unique_dir(dir: &std::path::Path, desired: &str) -> std::io::Result<String> {
    let name = sanitize_attachment_name(desired);
    for n in 0..10_000 {
        let candidate = if n == 0 { name.clone() } else { format!("{name}-{n}") };
        match std::fs::create_dir(dir.join(&candidate)) {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!("too many folders named like {name}"),
    ))
}

/// Whether moving `from_rel` to `to_rel` would put a folder onto itself or
/// inside its own subtree. Rel paths use '/' separators (the project-relative
/// convention everywhere in Ken).
pub fn is_into_own_subtree(from_rel: &str, to_rel: &str) -> bool {
    to_rel == from_rel || to_rel.starts_with(&format!("{from_rel}/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn taken(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn numbered_name_returns_the_desired_name_when_free() {
        let t = taken(&[]);
        assert_eq!(numbered_name("Untitled.md", |c| t.contains(c)), "Untitled.md");
    }

    #[test]
    fn numbered_name_counts_up_past_collisions() {
        let t = taken(&["Untitled.md"]);
        assert_eq!(numbered_name("Untitled.md", |c| t.contains(c)), "Untitled 2.md");
        let t = taken(&["Untitled.md", "Untitled 2.md"]);
        assert_eq!(numbered_name("Untitled.md", |c| t.contains(c)), "Untitled 3.md");
    }

    #[test]
    fn numbered_name_keeps_dotfiles_and_multi_dot_names_sane() {
        let t = taken(&[".env"]);
        assert_eq!(numbered_name(".env", |c| t.contains(c)), ".env 2");
        let t = taken(&["a.tar.gz"]);
        assert_eq!(numbered_name("a.tar.gz", |c| t.contains(c)), "a.tar 2.gz");
    }

    #[test]
    fn attachment_names_lose_paths_dots_and_control_chars() {
        assert_eq!(sanitize_attachment_name("shot.png"), "shot.png");
        assert_eq!(sanitize_attachment_name("My Shot.png"), "My Shot.png");
        assert_eq!(sanitize_attachment_name("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_attachment_name("a\\b\\c.pdf"), "c.pdf");
        assert_eq!(sanitize_attachment_name(".hidden.png"), "hidden.png");
        assert_eq!(sanitize_attachment_name("..."), "attachment");
        assert_eq!(sanitize_attachment_name(".."), "attachment");
        assert_eq!(sanitize_attachment_name(""), "attachment");
        assert_eq!(sanitize_attachment_name("dir/"), "attachment");
        assert_eq!(sanitize_attachment_name("bad\u{0}na\nme.png"), "badname.png");
    }

    #[test]
    fn attachment_candidates_count_with_a_hyphen() {
        assert_eq!(numbered_attachment_name("shot.png", 0), "shot.png");
        assert_eq!(numbered_attachment_name("shot.png", 1), "shot-1.png");
        assert_eq!(numbered_attachment_name("shot.png", 2), "shot-2.png");
        assert_eq!(numbered_attachment_name("README", 1), "README-1");
        assert_eq!(numbered_attachment_name("a.tar.gz", 1), "a.tar-1.gz");
    }

    #[test]
    fn create_unique_file_never_overwrites() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("shot.png"), b"original").unwrap();
        std::fs::write(dir.path().join("shot-1.png"), b"first copy").unwrap();

        let (mut f, name) = create_unique_file(dir.path(), "shot.png").unwrap();
        f.write_all(b"new").unwrap();
        assert_eq!(name, "shot-2.png");
        assert_eq!(std::fs::read(dir.path().join("shot.png")).unwrap(), b"original");
        assert_eq!(std::fs::read(dir.path().join("shot-1.png")).unwrap(), b"first copy");
        assert_eq!(std::fs::read(dir.path().join("shot-2.png")).unwrap(), b"new");

        let (_, name) = create_unique_file(dir.path(), "../escape.png").unwrap();
        assert_eq!(name, "escape.png");
        assert!(dir.path().join("escape.png").is_file());
    }

    #[test]
    fn create_unique_dir_never_merges() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Photos")).unwrap();
        std::fs::write(dir.path().join("Photos-1"), b"a file, not a folder").unwrap();

        assert_eq!(create_unique_dir(dir.path(), "Photos").unwrap(), "Photos-2");
        assert!(dir.path().join("Photos-2").is_dir());
        // Folder names keep their dots: no extension split.
        assert_eq!(create_unique_dir(dir.path(), "v1.2").unwrap(), "v1.2");
        assert_eq!(create_unique_dir(dir.path(), "v1.2").unwrap(), "v1.2-1");
        assert_eq!(create_unique_dir(dir.path(), "../up").unwrap(), "up");
    }

    #[test]
    fn subtree_guard_blocks_self_and_descendants_only() {
        assert!(is_into_own_subtree("A", "A"));
        assert!(is_into_own_subtree("A", "A/B"));
        assert!(is_into_own_subtree("Meetings/2026", "Meetings/2026/Q1"));
        assert!(!is_into_own_subtree("A", "AB")); // sibling sharing a name prefix
        assert!(!is_into_own_subtree("A/B", "A")); // moving OUT of a subtree is fine
        assert!(!is_into_own_subtree("A", "B/A"));
    }
}
