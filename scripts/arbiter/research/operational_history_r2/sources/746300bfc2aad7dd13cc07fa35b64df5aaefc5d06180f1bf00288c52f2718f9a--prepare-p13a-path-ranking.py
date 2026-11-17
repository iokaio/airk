# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,difflib,json
b=Path(__file__).resolve().parent
live=b/'p10-worktree/crates/airk-store/src/current/lexical.rs'
before=live.read_text()
s=before
def change(old,new,count=1):
    global s
    assert s.count(old)==count,(old[:90],s.count(old))
    s=s.replace(old,new)
change('const MAX_INDEX_BYTES: u64 = 1 << 40;','''const MAX_INDEX_BYTES: u64 = 1 << 40;
// Experimental S1 candidate; not a storage/FTS format change or BM25 score.
const RANKING_POOL_MULTIPLIER: usize = 8;
const MAX_RANKING_POOL: usize = 800;
const MAX_RANKING_METADATA_BYTES: usize = 8 << 20;

fn path_term_affinity(path: &str, wanted: &std::collections::BTreeSet<String>) -> usize {
    terms(path).intersection(wanted).count()
}
fn rank_lexical_pool(values: &mut [CurrentLexicalCandidate], wanted: &std::collections::BTreeSet<String>) {
    values.sort_by_cached_key(|candidate| (
        std::cmp::Reverse(path_term_affinity(&candidate.citation.path,wanted)),
        candidate.stable_id,
    ));
}''')
change('''        let result = self.with_connection(|connection| {
            let sql = if scope.is_some()''','''        // Every scope, alias, prefix and metadata predicate remains inside
        // the existing SQL WHERE before this expanded bounded first cutoff.
        let pool_limit = query.limit.saturating_mul(RANKING_POOL_MULTIPLIER).min(MAX_RANKING_POOL);
        let result = self.with_connection(|connection| {
            let sql = if scope.is_some()''')
change('(query.limit + 1) as i64','(pool_limit + 1) as i64',2)
change('''            let mut incomplete = false;
            while let Some(row) = rows.next()? {
                if values.len() == query.limit { incomplete = true; break; }''','''            let mut incomplete = false;
            let mut ranking_bytes = 0usize;
            while let Some(row) = rows.next()? {
                if values.len() == pool_limit { incomplete = true; break; }''')
change('''                let payload: String = row.get(3)?;
                ensure!(payload.len() <= MAX_ROW_BYTES, "FTS5 candidate byte bound exceeded");
                let citation: Citation = serde_json::from_str(&payload)?;''','''                // Inspect the borrowed SQLite value before allocating/parsing.
                let payload = row.get_ref(3)?.as_str()?;
                ranking_bytes = ranking_bytes.checked_add(path.len()).and_then(|bytes|bytes.checked_add(payload.len()))
                    .context("lexical ranking metadata byte overflow")?;
                ensure!(payload.len() <= MAX_ROW_BYTES && ranking_bytes <= MAX_RANKING_METADATA_BYTES,
                    "lexical ranking metadata budget exceeded");
                let citation: Citation = serde_json::from_str(payload)?;''')
change('''            Ok(CurrentLexicalCandidates { manifest_digest: self.head().lexical_commit, values, incomplete })''','''            rank_lexical_pool(&mut values, &query_terms);
            incomplete |= values.len() > query.limit;
            values.truncate(query.limit);
            Ok(CurrentLexicalCandidates { manifest_digest: self.head().lexical_commit, values, incomplete })''')
# Test metadata semantics without defining or running any candidate relevance
# query. Integrated scope/budget/actual-ranking tests remain phase acceptance.
needle='''    #[test]
    fn flush_rollback_and_replay_have_identical_seals()'''
tests='''    #[test]
    fn experimental_path_affinity_matches_tokens_not_substrings() {
        let wanted=terms("authorize_refund");
        assert!(path_term_affinity("src/authorize_refund.rs",&wanted)>path_term_affinity("docs/guide.md",&wanted));
        assert_eq!(path_term_affinity("src/authorizer.rs",&terms("authorize")),0);
        assert_eq!(path_term_affinity("@specs/refund.md",&terms("refund")),1);
    }
    #[test]
    fn experimental_equal_affinity_uses_stable_identity_despite_input_order() -> Result<()> {
        let make=|path:&str|->Result<CurrentLexicalCandidate>{Ok(CurrentLexicalCandidate {
            stable_id:stable_id(path,0)?,local:0,
            citation:Citation {path:path.into(),content_hash:"fixture".into(),start_line:1,end_line:1,
                location:airk_core::DocumentLocation::Lines {start:1,end:1}},
        })};
        let mut left=vec![make("src/a.rs")?,make("src/b.rs")?];
        let mut right=left.iter().rev().cloned().collect::<Vec<_>>();
        rank_lexical_pool(&mut left,&terms("unmatched"));
        rank_lexical_pool(&mut right,&terms("unmatched"));
        assert_eq!(left.iter().map(|value|value.stable_id).collect::<Vec<_>>(),
            right.iter().map(|value|value.stable_id).collect::<Vec<_>>());
        assert!(left[0].stable_id < left[1].stable_id);
        Ok(())
    }
    #[test]
    fn flush_rollback_and_replay_have_identical_seals()'''
change(needle,tests)
out=b/'p13a-lexical-path-affinity.proposed.rs'
out.write_text(s,newline='\n')
print(json.dumps({'base_sha256':hashlib.sha256(live.read_bytes()).hexdigest(),'proposal':str(out)}))
