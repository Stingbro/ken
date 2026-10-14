//! One git helper for everything that reads a repo's history (knowledge-core
//! decision 27): the live `history` tool, drift, a new knowledge base's repo
//! facts and the person's git identity. Every run is quiet (no console window
//! on Windows, [`crate::proc::quiet`]) and never prompts: a remote that wants
//! a password nobody stored fails instead of hanging.
//!
//! [`log`] is the log reader the history tool uses. [`commits_citing`] lists
//! the ticket and ruling ids a range of commits names, for slice 9's project
//! graph (decision 21) to turn into edges.

use std::path::Path;
use std::process::Command;

/// A `git` command in `dir`, quiet and never prompting. The caller adds the
/// arguments and runs it.
pub fn command(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    crate::proc::quiet(&mut cmd)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "never");
    cmd
}

/// Run git in `dir` and return what it printed, untouched. None when git is
/// missing or the command fails.
pub fn output_raw(dir: &Path, args: &[&str]) -> Option<String> {
    let out = command(dir).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Run git in `dir` and return what it printed, trimmed. None when git is
/// missing or the command fails; an empty answer is `Some("")`.
pub fn output(dir: &Path, args: &[&str]) -> Option<String> {
    output_raw(dir, args).map(|s| s.trim().to_string())
}

/// Run git in `dir` for its effect. The error is git's own last word, or why
/// it could not run.
pub fn run(dir: &Path, args: &[&str]) -> Result<(), String> {
    let out = command(dir).args(args).output().map_err(|e| format!("git could not run: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// A value of the person's global git config (`user.name`), None when git is
/// missing or the key is unset or empty.
pub fn global_config(key: &str) -> Option<String> {
    let mut cmd = Command::new("git");
    let out = crate::proc::quiet(&mut cmd)
        .args(["config", "--global", "--get", key])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .ok()?;
    let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !value.is_empty()).then_some(value)
}

/// One commit as [`log`] reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// The short sha.
    pub sha: String,
    /// `YYYY-MM-DD`.
    pub date: String,
    pub author: String,
    pub subject: String,
    /// The files it touched, as of then, when the log was asked for them
    /// (`--name-only`); empty otherwise.
    pub files: Vec<String>,
}

/// The fields of one commit in [`log`]'s output: a record separator before
/// each, the fields split by unit separators.
const LOG_FORMAT: &str = "--format=%x1e%h%x1f%ad%x1f%an%x1f%s";

/// The newest `limit` commits in `dir`, within the last `days` when given,
/// narrowed by `extra` (`--follow -- path`, `--grep=words`, `-Swords`, and
/// `--name-only` for each commit's files). None when git is missing, `dir` is
/// not a repo, or git refuses the arguments.
pub fn log(dir: &Path, limit: usize, days: Option<u64>, extra: &[&str]) -> Option<Vec<Commit>> {
    let n = format!("-n{limit}");
    let since = days.map(|d| format!("--since={d}.days.ago"));
    let mut args: Vec<&str> = vec!["log", &n, "--date=short", LOG_FORMAT];
    if let Some(since) = &since {
        args.push(since);
    }
    args.extend_from_slice(extra);
    output_raw(dir, &args).map(|out| parse_log(&out))
}

/// [`log`]'s output as commits; a record that is not whole is skipped.
pub fn parse_log(out: &str) -> Vec<Commit> {
    let mut commits = Vec::new();
    for record in out.split('\u{1e}').filter(|r| !r.trim().is_empty()) {
        let mut lines = record.lines();
        let head: Vec<&str> = lines.next().unwrap_or("").split('\u{1f}').collect();
        let [sha, date, author, subject] = head[..] else { continue };
        commits.push(Commit {
            sha: sha.to_string(),
            date: date.to_string(),
            author: author.to_string(),
            subject: subject.to_string(),
            files: lines.map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect(),
        });
    }
    commits
}

/// The ids a commit message names: ticket ids and ruling ids, both written
/// `PREFIX-number` (`SR-101`, `D-410`), in order, each once. These are
/// candidates: slice 9 keeps the ones the knowledge base has as a ticket or
/// a ruling, so a word of the same shape (`UTF-8`) is dropped there.
pub fn cited_ids(message: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let chars: Vec<char> = message.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let starts_word = i == 0 || !chars[i - 1].is_alphanumeric();
        if !(starts_word && chars[i].is_ascii_uppercase()) {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < chars.len() && (chars[j].is_ascii_uppercase() || chars[j].is_ascii_digit()) {
            j += 1;
        }
        if j < chars.len() && chars[j] == '-' {
            let mut k = j + 1;
            while k < chars.len() && chars[k].is_ascii_digit() {
                k += 1;
            }
            let ends_word = k == chars.len() || !chars[k].is_alphanumeric();
            if k > j + 1 && ends_word {
                let id: String = chars[i..k].iter().collect();
                if !out.contains(&id) {
                    out.push(id);
                }
            }
            i = k.max(i + 1);
        } else {
            i = j.max(i + 1);
        }
    }
    out
}

/// Each commit in `range` (`old..new`) whose message names an id, with the
/// ids it names ([`cited_ids`]), newest first. For slice 9: a commit citing
/// a ticket or a ruling becomes a graph edge (decision 21). None when git
/// cannot read the range.
pub fn commits_citing(dir: &Path, range: &str) -> Option<Vec<(String, Vec<String>)>> {
    let out = output_raw(dir, &["log", "--format=%x1e%h%x1f%B", range])?;
    Some(
        out.split('\u{1e}')
            .filter_map(|record| {
                let (sha, message) = record.split_once('\u{1f}')?;
                let ids = cited_ids(message);
                (!ids.is_empty()).then(|| (sha.trim().to_string(), ids))
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_ok(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(["-c", "user.name=Tess", "-c", "user.email=t@t"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn a_log_reads_each_commit_with_its_files() {
        let dir = tempfile::tempdir().unwrap();
        git_ok(dir.path(), &["init", "-q"]);
        std::fs::write(dir.path().join("auth.py"), "def check():\n    pass\n").unwrap();
        git_ok(dir.path(), &["add", "auth.py"]);
        git_ok(dir.path(), &["commit", "-q", "-m", "add the token check (SR-101)"]);
        std::fs::write(dir.path().join("auth.py"), "def check():\n    verify_signature()\n").unwrap();
        git_ok(dir.path(), &["commit", "-q", "-am", "harden it per D-410 and SR-101"]);

        let commits = log(dir.path(), 10, None, &["--name-only"]).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].subject, "harden it per D-410 and SR-101");
        assert_eq!(commits[0].author, "Tess");
        assert_eq!(commits[0].files, vec!["auth.py".to_string()]);
        assert_eq!(commits[0].date.len(), 10, "YYYY-MM-DD");
        let by_change = log(dir.path(), 10, None, &["-Sverify_signature"]).unwrap();
        assert_eq!(by_change.len(), 1);
        assert!(by_change[0].files.is_empty(), "no --name-only, no files");

        let first = output(dir.path(), &["rev-list", "--max-parents=0", "HEAD"]).unwrap();
        let cited = commits_citing(dir.path(), &format!("{first}..HEAD")).unwrap();
        assert_eq!(cited.len(), 1, "the range leaves the first commit out");
        assert_eq!(cited[0].1, vec!["D-410".to_string(), "SR-101".to_string()]);

        assert_eq!(output(dir.path(), &["no-such-command"]), None);
        assert!(run(dir.path(), &["no-such-command"]).is_err());
        let plain = tempfile::tempdir().unwrap();
        assert_eq!(log(plain.path(), 5, None, &[]), None, "not a repo");
    }

    #[test]
    fn a_message_names_ticket_and_ruling_ids() {
        assert_eq!(cited_ids("Fix SR-101: per D-410, see SR-101 again"), vec!["SR-101".to_string(), "D-410".to_string()]);
        assert!(cited_ids("bump x-1 and ABC- and -12").is_empty(), "lowercase, no number, no prefix");
        assert!(cited_ids("XSR-12a").is_empty(), "the number must end the word");
    }
}
