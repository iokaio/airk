// SPDX-License-Identifier: Apache-2.0
//! Finite literal source-span policy. No generated query or authority minting.
use super::*;
const PREFIX: &str = "airk.literal-search/1:";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLiteral {
    pub record: Sha256Digest,
    pub path: String,
    pub line: usize,
    pub returned_utf8_start: usize,
    pub returned_utf8_end: usize,
    pub query: String,
}
/// Maximal adjacent capitalized word runs, split into groups of at most four.
/// Only literal space/tab separates words. Source order is retained; no stop
/// list, title lookup, normalization, language model or human labels apply.
pub fn source_spans(text: &str) -> Vec<(usize, usize)> {
    let mut words = Vec::new();
    let mut iterator = text.char_indices().peekable();
    while let Some((start, c)) = iterator.next() {
        if !c.is_alphabetic() {
            continue;
        }
        let mut end = start + c.len_utf8();
        while let Some((index, next)) = iterator.peek().copied() {
            if !next.is_alphabetic() {
                break;
            }
            iterator.next();
            end = index + next.len_utf8();
        }
        words.push((start, end, c.is_uppercase()));
    }
    let mut spans = Vec::new();
    let mut index = 0;
    while index < words.len() && spans.len() < 8 {
        if !words[index].2 {
            index += 1;
            continue;
        }
        let mut count = 1;
        while count < 4
            && index + count < words.len()
            && words[index + count].2
            && text[words[index + count - 1].1..words[index + count].0]
                .chars()
                .all(|c| c == ' ' || c == '\t')
        {
            count += 1;
        }
        if count >= 2 {
            let start = words[index].0;
            let end = words[index + count - 1].1;
            if end - start <= 96 {
                spans.push((start, end));
            }
        }
        index += count;
    }
    spans
}
impl SourceLiteral {
    pub fn target(&self) -> Result<String> {
        self.validate()?;
        Ok(format!("{PREFIX}{}", serde_json::to_string(self)?))
    }
    pub fn parse(target: &str) -> Result<Option<Self>> {
        let Some(bytes) = target.strip_prefix(PREFIX) else {
            return Ok(None);
        };
        ensure!(
            bytes.len() <= 8192,
            "literal search provenance exceeds bound"
        );
        let source: Self = serde_json::from_str(bytes)?;
        source.validate()?;
        Ok(Some(source))
    }
    pub fn validate(&self) -> Result<()> {
        crate::index_catalog::relative_path(&self.path, false)?;
        ensure!(
            self.line > 0
                && self.line <= u32::MAX as usize
                && self.returned_utf8_start < self.returned_utf8_end
                && self.returned_utf8_end - self.returned_utf8_start == self.query.len()
                && source_spans(&self.query) == vec![(0, self.query.len())],
            "invalid literal source span"
        );
        Ok(())
    }
    pub fn matches(&self, record: Sha256Digest, hit: &crate::Hit) -> bool {
        self.record == record
            && self.path == hit.citation.path
            && self.line == hit.citation.start_line
            && hit
                .text
                .get(self.returned_utf8_start..self.returned_utf8_end)
                == Some(self.query.as_str())
    }
}
/// Source policy identity includes only CRLF→LF canonicalization, rejecting
/// lone CR; all other source bytes remain semantically distinct.
pub fn policy_digest() -> Result<Sha256Digest> {
    let bytes = include_bytes!("literal.rs");
    let mut canonical = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' {
            ensure!(
                bytes.get(index + 1) == Some(&b'\n'),
                "lone CR in literal source policy"
            );
            canonical.push(b'\n');
            index += 2;
        } else {
            canonical.push(bytes[index]);
            index += 1;
        }
    }
    Ok(crate::identity::framed_digest(
        b"airk.arbiter.literal-source-policy-lf/1",
        &[&canonical],
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_spans_are_exact_finite_capitalized_source_bytes() {
        let text = "prefix John Smith met Alice Brown. Lower case is not a span. New York City Harbor Office";
        let actual: Vec<_> = source_spans(text)
            .into_iter()
            .map(|(a, b)| &text[a..b])
            .collect();
        assert_eq!(
            actual,
            vec!["John Smith", "Alice Brown", "New York City Harbor"]
        );
        assert!(source_spans("One\nTwo lower text").is_empty());
        assert!(source_spans("lower words only").is_empty());
        assert_eq!(source_spans("Alpha Beta Gamma Delta Epsilon Zeta").len(), 2);
    }
    #[test]
    fn literal_target_round_trip_preserves_byte_offsets_and_refuses_nonliteral_rules() -> Result<()>
    {
        let source = SourceLiteral {
            record: Sha256Digest::from_bytes([1; 32]),
            path: "source.txt".into(),
            line: 2,
            returned_utf8_start: 7,
            returned_utf8_end: 17,
            query: "John Smith".into(),
        };
        let encoded = source.target()?;
        let parsed = SourceLiteral::parse(&encoded)?.unwrap();
        assert_eq!(parsed.query, "John Smith");
        assert_eq!(parsed.returned_utf8_start, 7);
        assert!(SourceLiteral::parse("source.txt:1-33")?.is_none());
        let mut invalid = source;
        invalid.query = "generated lowercase".into();
        assert!(invalid.target().is_err());
        Ok(())
    }
}
