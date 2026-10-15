// SPDX-License-Identifier: Apache-2.0
//! Optional, bounded similarity hints. This module never assigns identity or
//! redirects reads. Callers supply authenticated, scoped unmatched endpoints.
use airk_core::identity::Sha256Digest;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const SKETCH_VERSION: u32 = 1;
pub const MIN_SCORE_BASIS_POINTS: u32 = 8_000;
const MAX_FILES: usize = 256;
const MAX_FILE_BYTES: usize = 65_536;
const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;
const MAX_SHINGLES: usize = 2_048;
const MAX_PAIRS: usize = 4_096;
const MAX_SUGGESTIONS: usize = 64;
const MIN_SHINGLES: usize = 8;

/// Text is original retained source or extracted text in one consistent domain.
/// Do not combine source and extracted text within a call. `current` must contain
/// actual indexed targets, not guessed filenames or live filesystem candidates.
pub struct FileView<'a> {
    pub root: &'a str,
    pub path: &'a str,
    pub profile: Sha256Digest,
    pub text: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSuggestion {
    pub before: usize,
    pub after: usize,
    pub score_basis_points: u32,
    pub shared_shingles: usize,
    pub union_shingles: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectorySuggestion {
    pub root: String,
    pub before: String,
    pub after: String,
    pub children: Vec<(usize, usize)>,
}

#[derive(Default, Debug)]
pub struct Suggestions {
    pub files: Vec<FileSuggestion>,
    pub directories: Vec<DirectorySuggestion>,
    pub compared_pairs: usize,
    pub examined_files: usize,
    pub examined_text_bytes: usize,
    /// Includes omitted endpoints, sketch limits, pair limits and output limits.
    pub partial: bool,
}

type Sketch = BTreeSet<[u8; 32]>;
type DirectoryGroups<'a> = BTreeMap<(&'a str, &'a str), BTreeMap<&'a str, Vec<(usize, usize)>>>;

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.starts_with('/')
        && !path.contains(['\\', '\0', ':'])
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

fn sketch(text: &str) -> Option<Sketch> {
    // At most 65,536 bytes are scanned. Token boundaries normalize whitespace
    // only; case, punctuation, and literals remain present. This is not a code
    // semantic fingerprint, nor proof that two sources are interchangeable.
    let tokens: Vec<_> = text.split_whitespace().collect();
    let mut values = Sketch::new();
    for window in tokens.windows(5) {
        let mut hash = Sha256::new();
        hash.update(b"airk-file-suggestion-shingles/1");
        for token in window {
            hash.update((token.len() as u64).to_le_bytes());
            hash.update(token.as_bytes());
        }
        values.insert(hash.finalize().into());
        if values.len() > MAX_SHINGLES {
            return None;
        }
    }
    Some(values)
}

fn prepare(files: &[FileView<'_>], result: &mut Suggestions) -> Vec<(usize, Sketch)> {
    let mut output = Vec::new();
    result.partial |= files.len() > MAX_FILES;
    let mut paths = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for file in files.iter().take(MAX_FILES) {
        if !paths.insert((file.root, file.path)) {
            duplicates.insert((file.root, file.path));
        }
    }
    for (index, file) in files.iter().take(MAX_FILES).enumerate() {
        result.examined_files += 1;
        if !valid_path(file.path)
            || file.root.is_empty()
            || file.root.len() > 4096
            || duplicates.contains(&(file.root, file.path))
            || file.text.len() > MAX_FILE_BYTES
            || file.text.len() > MAX_TEXT_BYTES - result.examined_text_bytes
        {
            result.partial = true;
            continue;
        }
        result.examined_text_bytes += file.text.len();
        match sketch(file.text) {
            Some(sketch) if sketch.len() >= MIN_SHINGLES => output.push((index, sketch)),
            Some(_) => (), // Explicit minimum information threshold, not exhaustion.
            None => result.partial = true,
        }
    }
    output.sort_by_key(|(index, _)| (files[*index].root, files[*index].path));
    output
}

fn extension(path: &str) -> Option<&str> {
    path.rsplit('/')
        .next()?
        .rsplit_once('.')
        .map(|(_, ext)| ext)
}

pub fn suggest(before: &[FileView<'_>], current: &[FileView<'_>]) -> Suggestions {
    let mut result = Suggestions::default();
    let old = prepare(before, &mut result);
    let new = prepare(current, &mut result);
    'pairs: for (left, old_sketch) in &old {
        for (right, new_sketch) in &new {
            let a = &before[*left];
            let b = &current[*right];
            if a.root != b.root
                || a.profile != b.profile
                || a.path == b.path
                || extension(a.path) != extension(b.path)
            {
                continue;
            }
            if result.compared_pairs == MAX_PAIRS {
                result.partial = true;
                break 'pairs;
            }
            result.compared_pairs += 1;
            let shared = old_sketch.intersection(new_sketch).count();
            let union = old_sketch.len() + new_sketch.len() - shared;
            let score = (shared * 10_000 / union) as u32;
            if score >= MIN_SCORE_BASIS_POINTS {
                result.files.push(FileSuggestion {
                    before: *left,
                    after: *right,
                    score_basis_points: score,
                    shared_shingles: shared,
                    union_shingles: union,
                });
            }
        }
    }
    result.files.sort_by(|a, b| {
        b.score_basis_points
            .cmp(&a.score_basis_points)
            .then_with(|| before[a.before].path.cmp(before[b.before].path))
            .then_with(|| current[a.after].path.cmp(current[b.after].path))
    });
    if result.files.len() > MAX_SUGGESTIONS {
        result.partial = true;
        result.files.truncate(MAX_SUGGESTIONS);
    }
    // Never make directory claims from an incomplete ambiguity search.
    if !result.partial {
        result.directories = directories(before, current, &result.files);
    }
    result
}

fn directories(
    before: &[FileView<'_>],
    current: &[FileView<'_>],
    files: &[FileSuggestion],
) -> Vec<DirectorySuggestion> {
    let mut old_counts = BTreeMap::<usize, usize>::new();
    let mut new_counts = BTreeMap::<usize, usize>::new();
    for pair in files {
        *old_counts.entry(pair.before).or_default() += 1;
        *new_counts.entry(pair.after).or_default() += 1;
    }
    let mut groups = DirectoryGroups::new();
    for pair in files {
        let a = &before[pair.before];
        let b = &current[pair.after];
        let (Some((from, name)), Some((to, target_name))) =
            (a.path.rsplit_once('/'), b.path.rsplit_once('/'))
        else {
            continue;
        };
        // Register inconsistent or ambiguous mappings too, so they invalidate
        // the directory proposal instead of being silently discarded.
        let entries = groups
            .entry((a.root, from))
            .or_default()
            .entry(to)
            .or_default();
        if name == target_name && old_counts[&pair.before] == 1 && new_counts[&pair.after] == 1 {
            entries.push((pair.before, pair.after));
        }
    }
    groups
        .into_iter()
        .filter_map(|((root, from), targets)| {
            if targets.len() != 1 {
                return None;
            }
            let (to, children) = targets.into_iter().next()?;
            (from != to && children.len() >= 3).then(|| DirectorySuggestion {
                root: root.into(),
                before: from.into(),
                after: to.into(),
                children,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prose(prefix: &str) -> String {
        (0..40)
            .map(|i| format!("{prefix}{i} records original invoice entry {i}.\n"))
            .collect()
    }
    fn file<'a>(path: &'a str, text: &'a str) -> FileView<'a> {
        FileView {
            root: "original-root",
            path,
            profile: Sha256Digest::from_bytes([1; 32]),
            text,
        }
    }
    #[test]
    fn calibrated_small_edit_passes_unrelated_and_low_information_fail() {
        let original = prose("billing");
        let edited = original.replace("entry 17.", "entry revised-seventeen.");
        let unrelated = prose("wildlife")
            .replace("invoice", "habitat")
            .replace("entry", "observation");
        let result = suggest(
            &[file("old/a.txt", &original)],
            &[
                file("new/a.txt", &edited),
                file("new/b.txt", &unrelated),
                file("new/c.txt", "same same same same same same same same same"),
            ],
        );
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].after, 0);
        assert!(result.files[0].score_basis_points > 9_000);
        assert!(!result.partial);
        assert_eq!(SKETCH_VERSION, 1);
    }
    #[test]
    fn roots_profiles_and_exact_paths_are_never_candidates() {
        let text = prose("original");
        let mut foreign = file("new/a.txt", &text);
        foreign.root = "other-root";
        let mut profile = file("new/b.txt", &text);
        profile.profile = Sha256Digest::from_bytes([2; 32]);
        let result = suggest(
            &[file("old/a.txt", &text)],
            &[foreign, profile, file("old/a.txt", &text)],
        );
        assert!(result.files.is_empty());
        assert_eq!(result.compared_pairs, 0);
    }
    #[test]
    fn calibrated_threshold_is_not_an_identity_claim() {
        let original = prose("billing");
        let edited = |count: usize| {
            let mut text = original.clone();
            for i in 0..count {
                text = text.replace(&format!("entry {i}."), &format!("entry revised-{i}."));
            }
            text
        };
        let positive = edited(5);
        let negative = edited(6);
        let result = suggest(
            &[file("old/a.txt", &original)],
            &[file("new/a.txt", &positive), file("new/b.txt", &negative)],
        );
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].score_basis_points, 8084);
        let a = sketch(&original).unwrap();
        let b = sketch(&negative).unwrap();
        let shared = a.intersection(&b).count();
        assert_eq!(shared * 10000 / (a.len() + b.len() - shared), 7744);
        // Equal-looking clones remain separate suggestions; neither is selected.
        let copies = suggest(
            &[file("old/a.txt", &original)],
            &[
                file("copy/a.txt", &positive),
                file("another/a.txt", &positive),
            ],
        );
        assert_eq!(copies.files.len(), 2);
        assert!(copies.directories.is_empty());
        let boilerplate = "invoice record status open ".repeat(100);
        assert!(
            suggest(
                &[file("old/a.txt", &boilerplate)],
                &[file("new/a.txt", &(boilerplate.clone() + "closed"))],
            )
            .files
            .is_empty()
        );
    }
    #[test]
    fn directory_requires_three_consistent_unique_children() {
        let texts: Vec<_> = ["alpha", "bravo", "charlie"]
            .map(prose)
            .into_iter()
            .collect();
        // Unique vocabulary avoids near-identical boilerplate creating cross-pairs.
        let texts: Vec<_> = texts
            .iter()
            .enumerate()
            .map(|(i, t)| {
                t.split_whitespace()
                    .map(|word| format!("{i}_{word}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        let old: Vec<_> = ["old/a.txt", "old/b.txt", "old/c.txt"]
            .into_iter()
            .zip(&texts)
            .map(|(p, t)| file(p, t))
            .collect();
        let mut new: Vec<_> = ["new/a.txt", "new/b.txt", "new/c.txt"]
            .into_iter()
            .zip(&texts)
            .map(|(p, t)| file(p, t))
            .collect();
        let result = suggest(&old, &new);
        assert_eq!(result.files.len(), 3);
        assert_eq!(result.directories.len(), 1);
        assert_eq!(result.directories[0].children.len(), 3);
        assert!(suggest(&old[..2], &new[..2]).directories.is_empty());
        new.push(file("copy/a.txt", &texts[0]));
        assert!(suggest(&old, &new).directories.is_empty());
    }
    #[test]
    fn exhaustion_is_explicit_and_disables_directory_suggestions() {
        let oversized = "x".repeat(MAX_FILE_BYTES + 1);
        assert!(suggest(&[file("a.txt", &oversized)], &[]).partial);
        let many = (0..3000).map(|i| format!("unique{i} ")).collect::<String>();
        assert!(suggest(&[file("a.txt", &many)], &[]).partial);
        let text = prose("original");
        let duplicate = suggest(
            &[file("a.txt", &text), file("a.txt", &text)],
            &[file("b.txt", &text)],
        );
        assert!(duplicate.partial && duplicate.files.is_empty());
        let paths: Vec<_> = (0..65).map(|i| format!("old/{i}.txt")).collect();
        let targets: Vec<_> = (0..65).map(|i| format!("new/{i}.txt")).collect();
        let a: Vec<_> = paths.iter().map(|p| file(p, &text)).collect();
        let b: Vec<_> = targets.iter().map(|p| file(p, &text)).collect();
        let result = suggest(&a, &b);
        assert_eq!(result.compared_pairs, MAX_PAIRS);
        assert!(result.partial && result.directories.is_empty());
        assert_eq!(result.files.len(), MAX_SUGGESTIONS);
    }
}
