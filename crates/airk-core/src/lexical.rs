// SPDX-License-Identifier: Apache-2.0
//! Shared lexical normalization, independent of storage engines.
use std::collections::BTreeSet;

/// Whole identifier aliases plus underscore and camel-case components. No stemming.
pub fn terms(text: &str) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    let _: Result<(), std::convert::Infallible> = visit_terms(text, |term| {
        result.insert(term.to_lowercase());
        Ok(())
    });
    result
}

/// Identical normalization with explicit count and aggregate UTF-8 byte bounds.
/// Bounds apply before retaining another normalized term. This also avoids the
/// former per-word vector of every character and byte offset.
pub fn bounded_terms(
    text: &str,
    maximum_terms: usize,
    maximum_bytes: usize,
) -> anyhow::Result<BTreeSet<String>> {
    let mut result = BTreeSet::new();
    let mut used = 0usize;
    visit_terms(text, |term| {
        let mut normalized_bytes = 0usize;
        for c in term.chars().flat_map(char::to_lowercase) {
            anyhow::ensure!(
                normalized_bytes.saturating_add(c.len_utf8()) <= maximum_bytes,
                "lexical normalized term byte bound exceeded"
            );
            normalized_bytes += c.len_utf8();
        }
        // str lowercasing retains contextual Unicode rules such as final Greek
        // sigma. Per-character lowercasing alone would change term identities.
        let normalized = term.to_lowercase();
        anyhow::ensure!(
            normalized.len() <= maximum_bytes,
            "lexical normalized term byte bound exceeded"
        );
        if !result.contains(&normalized) {
            anyhow::ensure!(
                result.len() < maximum_terms
                    && used.saturating_add(normalized.len()) <= maximum_bytes,
                "lexical normalized term budget exceeded"
            );
            used += normalized.len();
            result.insert(normalized);
        }
        Ok(())
    })?;
    Ok(result)
}
fn visit_terms<E>(text: &str, mut emit: impl FnMut(&str) -> Result<(), E>) -> Result<(), E> {
    for word in text
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
    {
        if !word.chars().any(char::is_alphanumeric) {
            continue;
        }
        emit(word)?;
        for part in word.split('_').filter(|s| !s.is_empty()) {
            emit(part)?;
            let mut start = 0;
            let mut previous_lower = false;
            for (offset, c) in part.char_indices() {
                if c.is_uppercase() && previous_lower {
                    emit(&part[start..offset])?;
                    start = offset;
                }
                previous_lower = c.is_lowercase();
            }
            emit(&part[start..])?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_normalization_preserves_aliases_and_unicode_and_limits_unique_terms()
    -> anyhow::Result<()> {
        for text in [
            "CamelCase snake_case",
            "İstanbul café λ μCase",
            "HTTPServer aBC __word__",
            "ΟΣ ΣΤΟΣ ΟΣ_CamelCase",
            &"z".repeat(9000),
        ] {
            assert_eq!(bounded_terms(text, 100, 32_768)?, terms(text));
        }
        assert_eq!(bounded_terms("repeat repeat repeat", 1, 6)?.len(), 1);
        assert!(bounded_terms("one two", 1, 20).is_err());
        assert!(bounded_terms("İ", 1, 2).is_err());
        assert!(bounded_terms("longword", 2, 4).is_err());
        Ok(())
    }
}
