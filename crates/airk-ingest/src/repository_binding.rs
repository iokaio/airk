// SPDX-License-Identifier: Apache-2.0
//! Bind bounded repository grammar cues to original written-syntax evidence.
use crate::repository_syntax::{CueCategory, RepositoryFormat};
use airk_core::continuity::{CodeFingerprints, MAX_CONTINUITY_HASH_BYTES};
use airk_core::identity::framed_digest;
use airk_core::{CodeEvidence, CodeSpan, DocumentLocation, EvidenceBasis, Fact};
use anyhow::{Result, ensure};
use std::collections::BTreeMap;
use tree_sitter::Node;

fn symbol(path: &str, node: Node<'_>) -> String {
    format!(
        "syntax:{path}:{}:{}:{}",
        node.start_byte(),
        node.end_byte(),
        node.kind()
    )
}

fn display(value: &str) -> String {
    let mut end = value.len().min(1024);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].into()
}

pub(crate) fn analyze(
    path: &str,
    text: &str,
    root: Node<'_>,
    format: RepositoryFormat,
    language: &str,
) -> Result<crate::syntax::Analysis> {
    let cues = crate::repository_syntax::extract(root, text, format)?;
    let declarations: BTreeMap<_, _> = cues
        .iter()
        .filter(|cue| cue.category == CueCategory::Declaration)
        .map(|cue| (cue.node.id(), cue))
        .collect();
    let mut facts = Vec::with_capacity(cues.len());
    let mut hash_bytes = 0usize;
    let mut detail_bytes = 0usize;
    for cue in &cues {
        let mut scope = Vec::new();
        let mut ancestor = cue.owner;
        while let Some(node) = ancestor {
            ensure!(scope.len() < 128, "repository owner depth exceeds limit");
            let owner = declarations.get(&node.id()).ok_or_else(|| {
                anyhow::anyhow!("repository cue owner is not an admitted declaration")
            })?;
            scope.push(owner.name);
            ancestor = owner.owner;
        }
        scope.reverse();
        let qualified_bytes = scope.iter().fold(cue.name.len(), |total, name| {
            total.saturating_add(name.len()).saturating_add(2)
        });
        let mut qualified = String::new();
        for (index, name) in scope
            .iter()
            .copied()
            .chain(std::iter::once(cue.name))
            .enumerate()
        {
            if index > 0 {
                qualified.push_str("::");
            }
            let mut end = name.len().min(1024usize.saturating_sub(qualified.len()));
            while !name.is_char_boundary(end) {
                end -= 1;
            }
            qualified.push_str(&name[..end]);
            if qualified.len() >= 1024 || end < name.len() {
                break;
            }
        }
        let body = cue.body.and_then(|body| text.get(body.byte_range()));
        let signature = cue
            .body
            .and_then(|body| text.get(cue.node.start_byte()..body.start_byte()));
        let fingerprints = if cue.category == CueCategory::Declaration {
            hash_bytes = hash_bytes
                .saturating_add(cue.name.len())
                .saturating_add(qualified_bytes)
                .saturating_add(body.map_or(0, str::len))
                .saturating_add(signature.map_or(0, str::len));
            let admitted = hash_bytes <= MAX_CONTINUITY_HASH_BYTES;
            let qualified_fields: Vec<_> = scope
                .iter()
                .copied()
                .chain(std::iter::once(cue.name))
                .map(str::as_bytes)
                .collect();
            Some(CodeFingerprints {
                version: 1,
                name: admitted.then(|| framed_digest(b"code-name/1", &[cue.name.as_bytes()])),
                qualified_name: admitted
                    .then(|| framed_digest(b"code-qualified-name/1", &qualified_fields)),
                signature: signature
                    .filter(|_| admitted)
                    .map(|value| framed_digest(b"code-signature/1", &[value.as_bytes()])),
                body: body
                    .filter(|_| admitted)
                    .map(|value| framed_digest(b"code-body/1", &[value.as_bytes()])),
                body_informative: admitted
                    && cue
                        .body
                        .is_some_and(|body| crate::syntax::informative_body(body, text)),
                // Comment attachment is not qualified for these new grammars.
                documentation: None,
            })
        } else {
            None
        };
        let first = cue.node.start_position().row + 1;
        let last = (cue.node.end_position().row + usize::from(cue.node.end_position().column > 0))
            .max(first);
        let truncated = cue.name.len() > 1024
            || qualified_bytes > 1024
            || scope.iter().any(|name| name.len() > 1024)
            || signature.is_some_and(|value| value.len() > 1024);
        let fact = Fact {
            name: display(cue.name), kind: cue.kind.into(), syntax_kind: cue.node.kind().into(),
            start_line: first, end_line: last,
            location: DocumentLocation::Lines { start: first, end: last }, document: None,
            code: Some(CodeEvidence {
                fingerprints, symbol_id: symbol(path, cue.node), qualified_name: display(&qualified),
                scope: scope.into_iter().map(display).collect(),
                // Docker stages qualify following sibling instructions, but
                // only enclosing syntax can be a persisted syntax owner.
                owner_id: cue.owner.filter(|owner| owner.start_byte() <= cue.node.start_byte()
                    && owner.end_byte() >= cue.node.end_byte()
                    && owner.id() != cue.node.id()).map(|node| symbol(path, node)),
                signature: signature.map(display),
                span: Some(CodeSpan { start_byte: cue.node.start_byte(), end_byte: cue.node.end_byte() }),
                signature_span: signature.map(|_| CodeSpan { start_byte: cue.node.start_byte(), end_byte: cue.body.unwrap().start_byte() }),
                parameters: None, return_type: None, modifiers: vec![], documentation: None,
                target: matches!(cue.category, CueCategory::Reference | CueCategory::Import).then(|| display(cue.name)),
                basis: EvidenceBasis::Syntax, producer: format!("airk-tree-sitter/{language}"),
                producer_version: "repository-facts/1".into(),
                coverage: "Selected written repository syntax; no evaluation or resolved references; comment attachment remains unknown".into(),
                details_truncated: truncated,
            }),
        };
        let alias = cue.category == CueCategory::Declaration && cue.kind != "definition";
        detail_bytes = detail_bytes.saturating_add(
            serde_json::to_vec(&fact)?
                .len()
                .saturating_add(16)
                .saturating_mul(if alias { 2 } else { 1 }),
        );
        ensure!(
            detail_bytes <= 16_777_216,
            "repository evidence details exceed byte limit"
        );
        if alias {
            let mut declaration = fact.clone();
            declaration.kind = "definition".into();
            facts.push(declaration);
        }
        facts.push(fact);
        ensure!(facts.len() <= 10_000, "repository fact limit exceeded");
    }
    Ok(crate::syntax::Analysis {
        language: language.into(),
        status: if root.has_error() {
            "partial-syntax"
        } else {
            "parsed"
        }
        .into(),
        facts,
    })
}
