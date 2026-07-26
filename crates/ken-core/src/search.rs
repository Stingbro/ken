//! Hybrid search merge (semantic-index task 1.7).
//!
//! Pure, no DB/IO: takes already-ranked FTS and KNN chunk hit lists and
//! merges them into a path-grouped result list the UI can render as "files"
//! (today's mental model), each labeled by which retriever(s) found it.
//!
//! ## Design: B4 "FTS-priority fill", not RRF
//!
//! The original design (D4 in `design.md`) called this `rrf_merge` and
//! specified reciprocal rank fusion (`score(d) = Σ 1/(60 + rank_i(d))`,
//! k=60). Spike S7b (`spikes/S7b-retrieval-fixes.md`, 2026-07-25) found
//! naive RRF at k=60 *lost* to FTS5-only on the golden mini-set (0.278 MRR)
//! and replaced it with **Condition B: B4 "FTS-priority fill"** — FTS
//! ordering is preserved exactly as given, and KNN contributes only the
//! hits FTS didn't already surface, appended below, in KNN's own order
//! (not re-ranked, not interleaved). `openspec/changes/semantic-index/
//! tasks.md` (task 1.7) and `specs/semantic-index/spec.md`'s "Hybrid search
//! merges FTS and KNN by FTS-priority fill" requirement have both already
//! been updated to this design; this module implements it.
//!
//! Dedupe/tagging/grouping shape is preserved from the original design:
//! results are deduped and grouped by `path` (one entry per file, best
//! chunk's text as the snippet), and each entry is labeled `Keyword`
//! (FTS only), `Semantic` (KNN only), or `Both`.

use std::collections::{HashMap, HashSet};

/// One chunk-level hit from the FTS5 `chunks_fts` query, already ordered
/// best-first (by BM25 rank) by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtsHit {
    pub chunk_id: i64,
    pub path: String,
    pub text: String,
}

/// One chunk-level hit from the `vec_chunks` KNN query, already ordered
/// best-first (ascending distance) by the caller.
#[derive(Debug, Clone, PartialEq)]
pub struct VecHit {
    pub chunk_id: i64,
    pub path: String,
    pub text: String,
    pub distance: f64,
}

/// Which retriever(s) surfaced a path's winning chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Found by FTS5 keyword search only.
    Keyword,
    /// Found by vector KNN only.
    Semantic,
    /// Found by both retrievers (for this path, not necessarily the same
    /// chunk).
    Both,
}

/// One path-grouped hybrid search result. `snippet` is the winning chunk's
/// text — the FTS chunk's text if the path was found by FTS at all
/// (preserving "FTS ordering as-is" per B4), otherwise the best (first)
/// KNN chunk's text.
#[derive(Debug, Clone, PartialEq)]
pub struct HybridHit {
    pub path: String,
    pub chunk_id: i64,
    pub snippet: String,
    pub source: Source,
}

/// Merge FTS and KNN chunk hits per B4 "FTS-priority fill": FTS hits fill
/// the result list first, in their given order, one entry per distinct
/// path (first — i.e. best-ranked — chunk per path wins the snippet); KNN
/// hits are then appended, in their given order, for any path not already
/// present. A path present in both lists is tagged `Both` but keeps its
/// FTS position and FTS snippet — KNN never re-ranks or displaces an FTS
/// hit, it only adds paths FTS missed.
pub fn merge_hits(fts_hits: &[FtsHit], vec_hits: &[VecHit]) -> Vec<HybridHit> {
    let vec_chunk_ids: HashSet<i64> = vec_hits.iter().map(|h| h.chunk_id).collect();
    let vec_paths: HashSet<&str> = vec_hits.iter().map(|h| h.path.as_str()).collect();

    let mut out: Vec<HybridHit> = Vec::new();
    let mut path_index: HashMap<String, usize> = HashMap::new();

    for hit in fts_hits {
        if path_index.contains_key(&hit.path) {
            continue;
        }
        let source = if vec_chunk_ids.contains(&hit.chunk_id) || vec_paths.contains(hit.path.as_str())
        {
            Source::Both
        } else {
            Source::Keyword
        };
        path_index.insert(hit.path.clone(), out.len());
        out.push(HybridHit {
            path: hit.path.clone(),
            chunk_id: hit.chunk_id,
            snippet: hit.text.clone(),
            source,
        });
    }

    for hit in vec_hits {
        if path_index.contains_key(&hit.path) {
            // Already placed by the FTS pass above (possibly just upgraded
            // to `Both` there); KNN never moves or overwrites it.
            continue;
        }
        path_index.insert(hit.path.clone(), out.len());
        out.push(HybridHit {
            path: hit.path.clone(),
            chunk_id: hit.chunk_id,
            snippet: hit.text.clone(),
            source: Source::Semantic,
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fts(chunk_id: i64, path: &str, text: &str) -> FtsHit {
        FtsHit {
            chunk_id,
            path: path.to_string(),
            text: text.to_string(),
        }
    }

    fn vec_hit(chunk_id: i64, path: &str, text: &str, distance: f64) -> VecHit {
        VecHit {
            chunk_id,
            path: path.to_string(),
            text: text.to_string(),
            distance,
        }
    }

    #[test]
    fn disjoint_lists_fts_first_then_vec_appended() {
        let fts_hits = vec![fts(1, "a.rs", "alpha"), fts(2, "b.rs", "beta")];
        let vec_hits = vec![vec_hit(3, "c.rs", "gamma", 0.1), vec_hit(4, "d.rs", "delta", 0.2)];

        let merged = merge_hits(&fts_hits, &vec_hits);

        assert_eq!(merged.len(), 4);
        assert_eq!(merged[0].path, "a.rs");
        assert_eq!(merged[0].source, Source::Keyword);
        assert_eq!(merged[1].path, "b.rs");
        assert_eq!(merged[1].source, Source::Keyword);
        assert_eq!(merged[2].path, "c.rs");
        assert_eq!(merged[2].source, Source::Semantic);
        assert_eq!(merged[3].path, "d.rs");
        assert_eq!(merged[3].source, Source::Semantic);
    }

    #[test]
    fn full_overlap_tags_both_and_keeps_fts_order() {
        let fts_hits = vec![fts(1, "a.rs", "alpha-fts"), fts(2, "b.rs", "beta-fts")];
        // Vec ranks b.rs before a.rs (better distance) but B4 must not
        // reorder — FTS order wins.
        let vec_hits = vec![
            vec_hit(20, "b.rs", "beta-vec", 0.05),
            vec_hit(10, "a.rs", "alpha-vec", 0.1),
        ];

        let merged = merge_hits(&fts_hits, &vec_hits);

        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].path, "a.rs");
        assert_eq!(merged[0].source, Source::Both);
        assert_eq!(merged[0].snippet, "alpha-fts", "FTS snippet wins, not vec's");
        assert_eq!(merged[1].path, "b.rs");
        assert_eq!(merged[1].source, Source::Both);
        assert_eq!(merged[1].snippet, "beta-fts");
    }

    #[test]
    fn fts_order_is_preserved_verbatim_even_against_better_vec_ranks() {
        let fts_hits = vec![
            fts(1, "low-rank.rs", "text"),
            fts(2, "high-rank.rs", "text"),
        ];
        let vec_hits = vec![vec_hit(3, "new.rs", "brand new hit", 0.01)];

        let merged = merge_hits(&fts_hits, &vec_hits);

        // FTS order untouched: low-rank.rs still first even though it's not
        // "best"; the vector-only hit is appended last regardless of its
        // (very good) distance.
        assert_eq!(
            merged.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(),
            vec!["low-rank.rs", "high-rank.rs", "new.rs"]
        );
    }

    #[test]
    fn mixed_tagging_keyword_semantic_and_both() {
        let fts_hits = vec![
            fts(1, "keyword-only.rs", "kw"),
            fts(2, "shared.rs", "shared-fts"),
        ];
        let vec_hits = vec![
            vec_hit(2, "shared.rs", "shared-vec", 0.1),
            vec_hit(3, "semantic-only.rs", "sem", 0.2),
        ];

        let merged = merge_hits(&fts_hits, &vec_hits);

        let by_path: HashMap<&str, &HybridHit> =
            merged.iter().map(|h| (h.path.as_str(), h)).collect();
        assert_eq!(by_path["keyword-only.rs"].source, Source::Keyword);
        assert_eq!(by_path["shared.rs"].source, Source::Both);
        assert_eq!(by_path["semantic-only.rs"].source, Source::Semantic);
    }

    #[test]
    fn multiple_chunks_same_path_dedupe_to_first_fts_chunk() {
        // Two FTS hits for the same path (different chunks of that file) —
        // only the first (best-ranked) one should produce an entry.
        let fts_hits = vec![
            fts(1, "a.rs", "best chunk"),
            fts(2, "a.rs", "second-best chunk"),
        ];
        let vec_hits: Vec<VecHit> = Vec::new();

        let merged = merge_hits(&fts_hits, &vec_hits);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].chunk_id, 1);
        assert_eq!(merged[0].snippet, "best chunk");
    }

    #[test]
    fn empty_inputs_yield_empty_output() {
        assert!(merge_hits(&[], &[]).is_empty());
    }
}
