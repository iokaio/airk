// SPDX-License-Identifier: Apache-2.0
//! Bounded selection from a wider, already materialized search shortlist.

use airk_core::{Citation, DocumentLocation, Hit, lexical::terms};
use airk_store::current::lexical::TermGroup;
use std::{cmp::Reverse, collections::BTreeSet};

pub(super) fn duplicate(left: &Citation, right: &Citation) -> bool {
    if super::same_citation(left, right) {
        return true;
    }
    if left.path != right.path || left.content_hash != right.content_hash {
        return false;
    }

    let (
        DocumentLocation::Lines {
            start: left_start,
            end: left_end,
        },
        DocumentLocation::Lines {
            start: right_start,
            end: right_end,
        },
    ) = (&left.location, &right.location)
    else {
        return false;
    };

    let start = (*left_start).max(*right_start);
    let end = (*left_end).min(*right_end);
    if *left_start == 0 || *right_start == 0 || end < start {
        return false;
    }
    let shorter = (left_end - left_start).min(right_end - right_start) + 1;
    let shared = end - start + 1;
    shared >= shorter - shorter / 5
}

pub(super) fn select(candidates: &[(Hit, bool)], limit: usize) -> Vec<usize> {
    if candidates.len() <= limit {
        return (0..candidates.len()).collect();
    }

    let prefix = limit.saturating_sub(4);
    let mut selected: Vec<_> = (0..prefix).collect();
    let mut paths: BTreeSet<_> = candidates[..prefix]
        .iter()
        .map(|(hit, _)| hit.citation.path.as_str())
        .collect();
    let semantic_target = limit.min(prefix + 2);
    for (index, (hit, semantic)) in candidates.iter().enumerate().skip(prefix) {
        if selected.len() == semantic_target {
            break;
        }
        if *semantic && paths.insert(hit.citation.path.as_str()) {
            selected.push(index);
        }
    }
    for (index, (hit, _)) in candidates.iter().enumerate().skip(prefix) {
        if selected.len() == limit {
            break;
        }
        if paths.insert(hit.citation.path.as_str()) {
            selected.push(index);
        }
    }
    for index in prefix..candidates.len() {
        if selected.len() == limit {
            break;
        }
        if !selected.contains(&index) {
            selected.push(index);
        }
    }

    // The admission preferences change membership, while the fused rank
    // already establishes the deterministic order among admitted hits.
    selected.sort_unstable();
    selected
}

/// Rank up to four cutoff candidates by added query-group coverage. The
/// fixed prefix covers the first `limit - 2` results in fused order.
pub(super) fn complementary_band(
    candidates: &[(Hit, bool)],
    limit: usize,
    groups: &[TermGroup],
) -> Vec<usize> {
    let prefix = limit.saturating_sub(2).min(candidates.len());
    if limit == 0 || prefix == candidates.len() {
        return Vec::new();
    }

    let matches: Vec<Vec<usize>> = candidates
        .iter()
        .map(|(hit, _)| {
            let found = terms(&hit.text);
            groups
                .iter()
                .enumerate()
                .filter_map(|(index, group)| group.matches(&found).then_some(index))
                .collect()
        })
        .collect();
    let mut counts = vec![0; groups.len()];
    let mut paths = BTreeSet::new();
    for (index, (hit, _)) in candidates.iter().enumerate().take(prefix) {
        for &group in &matches[index] {
            counts[group] += 1;
        }
        paths.insert(hit.citation.path.as_str());
    }

    let slots = (candidates.len() - prefix).min(4);
    let mut band = Vec::with_capacity(slots);
    for _ in 0..slots {
        let Some(index) = (prefix..candidates.len())
            .filter(|index| !band.contains(index))
            .max_by_key(|&index| {
                let score: usize = matches[index]
                    .iter()
                    .map(|&group| 8 / (1 + counts[group]))
                    .sum();
                let new_path = !paths.contains(candidates[index].0.citation.path.as_str());
                (score, new_path, Reverse(index))
            })
        else {
            break;
        };
        band.push(index);
        for &group in &matches[index] {
            counts[group] += 1;
        }
        paths.insert(candidates[index].0.citation.path.as_str());
    }
    band
}

#[cfg(test)]
mod tests {
    use super::*;

    fn citation(path: &str, start: usize, end: usize) -> Citation {
        Citation {
            path: path.to_owned(),
            start_line: start,
            end_line: end,
            content_hash: "source-version".to_owned(),
            location: DocumentLocation::Lines { start, end },
        }
    }

    fn hit(path: &str, semantic: bool) -> (Hit, bool) {
        (
            Hit {
                citation: citation(path, 1, 5),
                text: String::new(),
                truncated: false,
            },
            semantic,
        )
    }

    #[test]
    fn overlap_requires_at_least_four_fifths_of_the_shorter_native_span() {
        let a = citation("a", 10, 14);
        assert!(duplicate(&a, &citation("a", 11, 20)));
        assert!(duplicate(&citation("a", 11, 20), &a));
        assert!(!duplicate(&a, &citation("a", 12, 20)));
        assert!(!duplicate(&a, &citation("b", 10, 14)));

        let mut other_version = a.clone();
        other_version.content_hash.push('2');
        assert!(!duplicate(&a, &other_version));

        let wide = citation("a", 1, usize::MAX);
        assert!(duplicate(&wide, &citation("a", usize::MAX, usize::MAX)));
    }

    #[test]
    fn non_line_locations_require_an_exact_citation() {
        let mut page = citation("a", 10, 14);
        page.location = DocumentLocation::Page { number: 2 };
        assert!(duplicate(&page, &page));
        assert!(!duplicate(&page, &citation("a", 10, 14)));
        let mut other_page = page.clone();
        other_page.location = DocumentLocation::Page { number: 3 };
        assert!(!duplicate(&page, &other_page));
    }

    #[test]
    fn deeper_semantic_paths_can_enter_without_reordering_the_fused_prefix() {
        let candidates = [
            hit("a", false),
            hit("b", false),
            hit("c", false),
            hit("d", false),
            hit("e", false),
            hit("f", false),
            hit("a", true),
            hit("g", false),
            hit("h", false),
            hit("i", false),
            hit("j", true),
            hit("j", true),
            hit("k", true),
            hit("l", true),
        ];
        assert_eq!(select(&candidates, 10), [0, 1, 2, 3, 4, 5, 7, 8, 10, 12]);
    }

    #[test]
    fn smaller_pools_pass_through_and_single_file_pools_fill_in_rank_order() {
        let candidates = [hit("a", true), hit("a", false), hit("a", true)];
        assert_eq!(select(&candidates, 10), [0, 1, 2]);
        assert_eq!(select(&candidates, 2), [0, 1]);
        assert!(select(&candidates, 0).is_empty());
    }
    #[test]
    fn complementary_band_rewards_evidence_missing_from_the_prefix() {
        let mut candidates = vec![hit("first", false); 8];
        for (candidate, _) in &mut candidates {
            candidate.text = "common".to_owned();
        }
        for (path, text) in [
            ("new-common", "common"),
            ("first", "alpha"),
            ("new-alpha", "alpha alpha"),
            ("new-beta", "beta"),
            ("new-gamma", "gamma"),
        ] {
            let (mut candidate, semantic) = hit(path, false);
            candidate.text = text.to_owned();
            candidates.push((candidate, semantic));
        }
        let groups: Vec<_> = ["common", "alpha", "beta", "gamma"]
            .into_iter()
            .map(|term| TermGroup {
                alternatives: vec![vec![term.to_owned()]],
            })
            .collect();

        assert_eq!(
            complementary_band(&candidates, 10, &groups),
            [10, 11, 12, 9]
        );
        assert_eq!(complementary_band(&candidates[..9], 10, &groups), [8]);
        assert!(complementary_band(&candidates[..8], 10, &groups).is_empty());
        assert!(complementary_band(&candidates, 0, &groups).is_empty());
    }
}
