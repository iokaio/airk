// SPDX-License-Identifier: Apache-2.0
//! Shared lexical normalization, independent of storage engines.
use std::collections::BTreeSet;

/// Whole identifier aliases plus underscore and camel-case components. No stemming.
pub fn terms(text: &str) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    for word in text
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
    {
        if !word.chars().any(char::is_alphanumeric) {
            continue;
        }
        result.insert(word.to_lowercase());
        for part in word.split('_').filter(|s| !s.is_empty()) {
            result.insert(part.to_lowercase());
            let mut start = 0;
            let chars: Vec<_> = part.char_indices().collect();
            for i in 1..chars.len() {
                if chars[i].1.is_uppercase() && chars[i - 1].1.is_lowercase() {
                    result.insert(part[start..chars[i].0].to_lowercase());
                    start = chars[i].0;
                }
            }
            result.insert(part[start..].to_lowercase());
        }
    }
    result
}
