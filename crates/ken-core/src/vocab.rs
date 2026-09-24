//! Aliases in the query: the team's words and the platform's words for the
//! same thing, read from the library, so a search in the wrong word still
//! finds the page (docs-system: "When a search comes back empty because it
//! used the wrong word, add a row"; decisions: "Every entry carries all the
//! terms anyone might search by").
//!
//! Three sources, all read from what the index already stores:
//! - the Vocabulary page's word tables (our word ↔ the platform's word);
//! - each decisions-log entry's topic and `aliases:` line;
//! - each page's frontmatter title and `aliases`.
//!
//! Keyword search requires every word, so an alias is not appended to the
//! query; it yields an alternative query with the matched phrase swapped,
//! which the caller runs alongside the original.

use crate::db::Db;
use crate::Result;

/// Most alternatives tried per query, so a busy vocabulary cannot turn one
/// search into dozens.
pub const MAX_ALTERNATIVES: usize = 3;

/// Groups of phrases that mean the same thing, lowercased.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Vocabulary {
    pub groups: Vec<Vec<String>>,
}

impl Vocabulary {
    /// Build from a project's index: its Vocabulary and DECISIONS pages and
    /// every page's frontmatter aliases.
    pub fn from_db(db: &Db) -> Result<Vocabulary> {
        let mut groups = Vec::new();
        for path in db.paths_named(&["vocabulary.md"])? {
            if let Some(text) = db.get_text(&path)? {
                groups.extend(parse_vocabulary(&text));
            }
        }
        for path in db.paths_named(&["decisions.md"])? {
            if let Some(text) = db.get_text(&path)? {
                groups.extend(parse_decisions(&text));
            }
        }
        for (title, aliases) in db.page_aliases()? {
            let mut g: Vec<String> = title.into_iter().chain(aliases).collect();
            g.retain(|p| !p.trim().is_empty());
            if g.len() > 1 {
                groups.push(g);
            }
        }
        Ok(Vocabulary::from_groups(groups))
    }

    pub fn from_groups(groups: Vec<Vec<String>>) -> Vocabulary {
        let groups = groups
            .into_iter()
            .map(|g| {
                let mut out: Vec<String> = Vec::new();
                for p in g.iter().map(|p| normalize(p)).filter(|p| p.len() >= 2) {
                    if !out.contains(&p) {
                        out.push(p);
                    }
                }
                out
            })
            .filter(|g| g.len() > 1)
            .collect();
        Vocabulary { groups }
    }

    /// Alternative phrasings of `query`: each phrase of a group found in the
    /// query (as whole words) swapped for each other phrase of that group.
    /// Longest match first, the original never included, at most
    /// [`MAX_ALTERNATIVES`].
    pub fn alternatives(&self, query: &str) -> Vec<String> {
        let q = normalize(query);
        let padded = format!(" {q} ");
        let mut hits: Vec<(&str, &Vec<String>)> = Vec::new();
        for g in &self.groups {
            for p in g {
                if padded.contains(&format!(" {p} ")) {
                    hits.push((p.as_str(), g));
                }
            }
        }
        hits.sort_by_key(|(p, _)| std::cmp::Reverse(p.len()));
        let mut out: Vec<String> = Vec::new();
        for (found, group) in hits {
            for other in group.iter().filter(|o| o.as_str() != found) {
                let alt = padded.replacen(&format!(" {found} "), &format!(" {other} "), 1).trim().to_string();
                if alt != q && !out.contains(&alt) {
                    out.push(alt);
                }
                if out.len() == MAX_ALTERNATIVES {
                    return out;
                }
            }
        }
        out
    }
}

/// Lowercase, punctuation to spaces, spaces collapsed.
fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_placeholder(cell: &str) -> bool {
    let c = cell.trim();
    c.is_empty() || c.contains("{{")
}

fn cells(line: &str) -> Option<Vec<String>> {
    let t = line.trim();
    let inner = t.strip_prefix('|')?.strip_suffix('|')?;
    Some(inner.split('|').map(|c| c.trim().trim_matches('`').to_string()).collect())
}

/// Rows of the word tables: a table whose header starts with "our word" and
/// "the platform's word" (either order). Each row's first two cells are one
/// group. Other tables (Method Words is word and meaning, not two words) are
/// skipped.
pub fn parse_vocabulary(text: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let mut in_word_table = false;
    let mut after_header = false;
    for line in text.lines() {
        let Some(row) = cells(line) else {
            in_word_table = false;
            after_header = false;
            continue;
        };
        if !in_word_table && !after_header {
            let h: Vec<String> = row.iter().map(|c| c.to_lowercase()).collect();
            let word = |c: &str| c == "our word" || c == "the platform's word" || c == "the platform’s word";
            in_word_table = h.len() >= 2 && word(&h[0]) && word(&h[1]);
            after_header = true;
            continue;
        }
        if row.iter().all(|c| c.chars().all(|ch| ch == '-' || ch == ':' || ch == ' ')) {
            continue; // the |---|---| rule
        }
        if in_word_table && row.len() >= 2 && !is_placeholder(&row[0]) && !is_placeholder(&row[1]) {
            out.push(vec![row[0].clone(), row[1].clone()]);
        }
    }
    out
}

/// Each decisions-log entry's topic and aliases, as one group. An entry
/// starts with `D-nnn · date · topic — ruling` (`·` or ` - ` between
/// fields); its `aliases:` line lists the other terms, comma separated.
pub fn parse_decisions(text: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let mut current: Option<Vec<String>> = None;
    let mut in_fence = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            in_fence = !in_fence; // the format example, not an entry
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(topic) = entry_topic(t) {
            if let Some(g) = current.take() {
                out.push(g);
            }
            current = Some(vec![topic]);
            continue;
        }
        if let (Some(g), Some(rest)) = (current.as_mut(), t.strip_prefix("aliases:")) {
            g.extend(rest.split(',').map(|a| a.trim().trim_matches('"').to_string()).filter(|a| !is_placeholder(a)));
        }
    }
    if let Some(g) = current {
        out.push(g);
    }
    out.into_iter().filter(|g| g.len() > 1).collect()
}

fn entry_topic(line: &str) -> Option<String> {
    let rest = line.strip_prefix("D-")?;
    if !rest.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let sep = if line.contains(" · ") { " · " } else { " - " };
    let fields: Vec<&str> = line.splitn(3, sep).collect();
    let tail = fields.get(2)?;
    let topic = tail.split(" — ").next().unwrap_or(tail).split(" - ").next().unwrap_or(tail).trim();
    (!is_placeholder(topic)).then(|| topic.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_tables_pair_our_word_with_the_platforms() {
        let text = "## Our Words\n\n| our word | the platform's word | where it lives |\n|---|---|---|\n| {{our word}} | {{the platform's word}} | x |\n| shard | region | Engine/World.md |\n\n## Method Words\n\n| word | means | where it lives |\n|---|---|---|\n| backlog | the Board's ranked list | tickets/ |\n";
        assert_eq!(parse_vocabulary(text), vec![vec!["shard".to_string(), "region".to_string()]]);
    }

    #[test]
    fn decisions_give_topic_and_aliases_and_skip_the_format_example() {
        let text = "Format:\n\n```\nD-001 · 2026-01-01 · topic — THE RULING\n  aliases: every term\n```\n\nD-012 · 2026-09-01 · save format — Worlds save as region files.\n  Why: disk.\n  aliases: world save, backup format, persistence\nD-013 · 2026-09-02 · combat — no aliases here.\n";
        assert_eq!(
            parse_decisions(text),
            vec![vec!["save format", "world save", "backup format", "persistence"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()]
        );
    }

    #[test]
    fn a_query_in_the_wrong_word_gets_the_right_one_as_an_alternative() {
        let v = Vocabulary::from_groups(vec![
            vec!["shard".into(), "region".into()],
            vec!["Team".into(), "who decides".into(), "how is the team structured".into()],
        ]);
        assert_eq!(v.alternatives("how do shards load"), Vec::<String>::new(), "whole words only");
        assert_eq!(v.alternatives("shard loading"), vec!["region loading"]);
        assert_eq!(v.alternatives("Who decides?"), vec!["team", "how is the team structured"]);
        assert!(v.alternatives("nothing matches").is_empty());
    }

    #[test]
    fn alternatives_are_capped() {
        let v = Vocabulary::from_groups(vec![vec!["a1".into(), "b1".into(), "c1".into(), "d1".into(), "e1".into()]]);
        assert_eq!(v.alternatives("a1").len(), MAX_ALTERNATIVES);
    }
}
