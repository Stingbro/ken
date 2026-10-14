//! The one findings list (knowledge-core decisions 10, 14 and 29): what a
//! check found that a person should look at, kept as Review items of kind
//! [`KIND`] in the knowledge base's index. Ingest files each contradiction a
//! source shows and each question it leaves open; drift and lint file theirs
//! here in slice 8. A finding changes nothing by itself: a person reads it,
//! fixes what needs fixing and marks it done.

use serde::{Deserialize, Serialize};

use crate::db::{Db, ReviewItemRow};
use crate::{Error, Result};

/// The Review kind of a finding.
pub const KIND: &str = "finding";

/// What a finding is about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// `contradiction` | `question`
    pub kind: String,
    /// Where it was found: the ingested note it came from.
    pub source: String,
    /// What it contradicts, as the note names it: a page or ticket path, a
    /// ruling id (`D-012`), or a code file. Empty for a question.
    #[serde(default)]
    pub against: String,
    /// `page` | `ruling` | `ticket` | `code` | `source` (the source disagrees
    /// with itself), empty for a question.
    #[serde(default)]
    pub against_kind: String,
    /// How the two differ, or the question.
    pub text: String,
    /// The source's own words, when the note quotes them.
    #[serde(default)]
    pub quote: String,
}

impl Finding {
    /// A contradiction with a ruling or with the code: never edited away,
    /// always for a person (decision 10).
    pub fn binds(&self) -> bool {
        matches!(self.against_kind.as_str(), "ruling" | "code")
    }

    fn title(&self) -> String {
        let short: String = self.text.chars().take(80).collect();
        match self.kind.as_str() {
            "question" => format!("Open question: {short}"),
            _ if self.against.is_empty() => format!("Contradiction: {short}"),
            _ => format!("Contradicts {}: {short}", self.against),
        }
    }

    fn body(&self) -> String {
        let stem = self.source.rsplit('/').next().unwrap_or(&self.source).trim_end_matches(".md");
        let mut s = self.text.clone();
        if !self.quote.is_empty() {
            s.push_str(&format!("\n\n> {}", self.quote));
        }
        s.push_str(&format!("\n\nFrom [[{stem}]]."));
        if self.binds() {
            s.push_str(" A contradiction with a ruling or the code waits for a person: Ken changed nothing for it.");
        }
        s
    }
}

/// File one finding. Returns its Review item's id.
pub fn file(db: &mut Db, f: &Finding, now: i64) -> Result<i64> {
    let payload = serde_json::to_string(f).map_err(|e| Error::Other(e.to_string()))?;
    db.insert_review_item(KIND, &f.title(), &f.body(), &f.source, Some(&payload), now)
}

/// Every open finding, newest first, with what it is about.
pub fn open(db: &Db) -> Result<Vec<(ReviewItemRow, Finding)>> {
    Ok(db
        .list_open_review_items()?
        .into_iter()
        .filter(|it| it.kind == KIND)
        .filter_map(|it| {
            let f = it.payload.as_deref().and_then(|p| serde_json::from_str::<Finding>(p).ok())?;
            Some((it, f))
        })
        .collect())
}

/// How many open findings came from `source`.
pub fn open_from(db: &Db, source: &str) -> Result<usize> {
    Ok(open(db)?.into_iter().filter(|(_, f)| f.source == source).count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_finding_is_filed_and_read_back() {
        let mut db = Db::open_in_memory().unwrap();
        let f = Finding {
            kind: "contradiction".into(),
            source: "Research/Ingestion/Ingested/Meetings/2026-10/2026-10-13-standup.md".into(),
            against: "D-012".into(),
            against_kind: "ruling".into(),
            text: "The note says saves are global; the ruling says per region".into(),
            quote: "we save it all in one file".into(),
        };
        let id = file(&mut db, &f, 5).unwrap();
        let open = open(&db).unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].0.id, id);
        assert_eq!(open[0].1, f);
        assert!(open[0].0.title.starts_with("Contradicts D-012: "));
        assert!(open[0].0.body.contains("> we save it all in one file") && open[0].0.body.contains("[[2026-10-13-standup]]"));
        assert!(open[0].0.body.contains("waits for a person"), "a ruling binds");
        assert_eq!(open_from(&db, &f.source).unwrap(), 1);
        let q = Finding { kind: "question".into(), source: f.source.clone(), text: "Who owns saves?".into(), ..Default::default() };
        assert!(!q.binds() && q.title() == "Open question: Who owns saves?");
    }
}
