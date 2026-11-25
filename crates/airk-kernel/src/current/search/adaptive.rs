// SPDX-License-Identifier: Apache-2.0
//! Query-time sizing for an omitted Search result limit.

use std::collections::BTreeSet;

const MAX_ADAPTIVE_HITS: usize = 20;
pub(super) const TAIL_BYTES: usize = 256;
const PRECISE_HITS: usize = 4;
const LEAD_SCORE_RATIO: f64 = 1.75;
const TAIL_SCORE_RATIO: f64 = 2.0 / 3.0;

/// Size the response from fused scores in their file-diversified rank order.
pub(super) fn limit<'a>(
    ranked: impl IntoIterator<Item = (f64, &'a str)>,
    default: usize,
    max_bytes: usize,
    broad: bool,
) -> usize {
    let mut ranked = ranked.into_iter();
    if !broad {
        // A substantial lead suggests a precise lookup. Keep several
        // alternatives even when one candidate is clearly ahead.
        return match (ranked.next(), ranked.next()) {
            (Some((first, _)), Some((second, _)))
                if first.is_finite()
                    && second.is_finite()
                    && second > 0.0
                    && first > LEAD_SCORE_RATIO * second =>
            {
                default.min(PRECISE_HITS)
            }
            _ => default,
        };
    }

    let ceiling = MAX_ADAPTIVE_HITS.min(max_bytes / TAIL_BYTES);
    if default == 0 || ceiling <= default {
        return default;
    }

    let mut paths = BTreeSet::new();
    let mut cutoff = 0.0;
    for _ in 0..default {
        let Some((score, path)) = ranked.next() else {
            return default;
        };
        paths.insert(path);
        cutoff = score;
    }
    if !cutoff.is_finite() || cutoff <= 0.0 {
        return default;
    }

    // Extend only for new files that still score near the normal cutoff.
    // The threshold is relative because fused scores vary by query.
    let mut result = default;
    let threshold = cutoff * TAIL_SCORE_RATIO;
    for (offset, (score, path)) in ranked.take(ceiling - default).enumerate() {
        if score.is_finite() && score >= threshold && paths.insert(path) {
            result = default + offset + 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broad_extension_requires_new_paths_and_near_cutoff_scores() {
        let ranked = [
            (12.0, "a"),
            (11.0, "b"),
            (10.0, "c"),
            (9.0, "d"),
            (8.0, "e"),
            (7.0, "f"),
            (6.0, "g"),
            (6.0, "h"),
            (6.0, "i"),
            (6.0, "j"),
            (6.0, "a"),
            (4.0, "k"),
            (4.0, "k"),
            (3.9, "l"),
            (4.5, "m"),
        ];

        assert_eq!(limit(ranked, 10, 8192, true), 15);
        assert_eq!(limit(ranked, 10, 14 * 256, true), 12);
        assert_eq!(limit(ranked, 10, 11 * 256, true), 10);
        assert_eq!(limit(ranked[..9].iter().copied(), 10, 8192, true), 10);
    }

    #[test]
    fn broad_extension_stops_at_twenty() {
        let files: Vec<_> = (0..30).map(|index| index.to_string()).collect();
        let ranked = files.iter().map(|path| (1.0, path.as_str()));
        assert_eq!(limit(ranked, 10, 8192, true), 20);
    }

    #[test]
    fn precise_reduction_requires_clear_separation() {
        assert_eq!(limit([(1.8, "a"), (1.0, "b")], 10, 8192, false), 4);
        assert_eq!(limit([(1.75, "a"), (1.0, "b")], 10, 8192, false), 10);
        assert_eq!(limit([(2.0, "a")], 10, 8192, false), 10);
        assert_eq!(limit([(1.8, "a"), (1.0, "b")], 2, 8192, false), 2);
    }
}
