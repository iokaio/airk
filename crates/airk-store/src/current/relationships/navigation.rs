// SPDX-License-Identifier: Apache-2.0
//! Conservative, source-bound Rust type/impl navigation over retained facts.
use super::super::discovery::fact_hit;
use airk_core::{
    Artifact, CodeEvidence, EvidenceBasis, Fact, RelationshipHit, RelationshipTargetSuggestion,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const MAX_FACTS: usize = 4096;
const MAX_TARGETS: usize = 100;
const COVERAGE: &str = "Declared associated-method spelling from the same retained Rust file; written-target matches remain syntax candidates, not resolved callers. Aliases and imported types are not resolved.";

#[derive(Default)]
pub(super) struct Navigation {
    pub path: String,
    pub contains: Vec<RelationshipHit>,
    pub suggestions: Vec<RelationshipTargetSuggestion>,
    pub truncated: bool,
    pub verified: Option<VerifiedSource>,
}

pub(super) struct VerifiedSource {
    pub revision: crate::current::CurrentToken,
    pub source_hash: String,
    pub edges: Vec<RelationshipHit>,
}

fn native_definition(fact: &Fact) -> Option<&CodeEvidence> {
    let code = fact.code.as_ref()?;
    (fact.kind == "definition"
        && code.basis == EvidenceBasis::Syntax
        && code.producer == "airk-tree-sitter/rust"
        && !code.details_truncated)
        .then_some(code)
}

/// Remove only balanced generic arguments from a written Rust type path.
/// No aliases, Self/super/crate roots, references or arbitrary type expressions
/// are inferred. Those need stronger source/module evidence than these facts.
fn type_path(value: &str) -> Option<Vec<String>> {
    let mut depth = 0usize;
    let mut plain = String::new();
    for ch in value.trim().chars() {
        match ch {
            '<' => depth = depth.checked_add(1)?,
            '>' => depth = depth.checked_sub(1)?,
            _ if depth > 0 => {}
            ch if ch.is_whitespace() => return None,
            ch if ch == ':' || ch == '_' || ch.is_alphanumeric() => plain.push(ch),
            _ => return None,
        }
    }
    if depth != 0 || plain.len() > 1024 {
        return None;
    }
    let mut parts: Vec<_> = plain.split("::").map(str::to_owned).collect();
    if parts.first().is_some_and(|part| part == "self") {
        parts.remove(0);
    }
    (!parts.is_empty()
        && parts.iter().all(|part| {
            !matches!(part.as_str(), "" | "Self" | "super" | "crate")
                && part
                    .chars()
                    .next()
                    .is_some_and(|c| c == '_' || c.is_alphabetic())
        }))
    .then_some(parts)
}

fn declared_type(fact: &Fact) -> Option<Vec<String>> {
    let code = native_definition(fact)?;
    matches!(fact.syntax_kind.as_str(), "struct_item" | "enum_item").then_some(())?;
    let name = type_path(&fact.name)?;
    (name.len() == 1).then_some(())?;
    let mut path = code.scope.clone();
    path.extend(name);
    Some(path)
}

fn implemented_type(fact: &Fact) -> Option<Vec<String>> {
    let code = native_definition(fact)?;
    (fact.syntax_kind == "impl_item").then_some(())?;
    let target = type_path(&fact.name)?;
    // A generic parameter can shadow a same-named local declaration:
    // `struct Session; impl<Session: Trait> Trait for Session { ... }`.
    // Its written target must never be attached to that concrete struct.
    if generic_parameter_shadows(code.signature.as_deref()?, &target)? {
        return None;
    }
    let mut path = code.scope.clone();
    path.extend(target);
    Some(path)
}

fn generic_parameter_shadows(signature: &str, target: &[String]) -> Option<bool> {
    let mut signature = signature.trim_start();
    for modifier in ["default", "unsafe"] {
        if let Some(rest) = signature.strip_prefix(modifier)
            && rest.chars().next().is_some_and(char::is_whitespace)
        {
            signature = rest.trim_start();
        }
    }
    let rest = signature.strip_prefix("impl")?;
    if rest
        .chars()
        .next()
        .is_some_and(|c| c == '_' || c.is_alphanumeric())
    {
        return None;
    }
    let rest = rest.trim_start();
    if !rest.starts_with('<') {
        return Some(false);
    }
    // A finite generic-binder reader must not mistake an arrow or a const
    // expression's comparison for the closing generic delimiter.
    if rest.contains("->")
        || rest
            .chars()
            .any(|c| matches!(c, '{' | '}' | '"' | '/' | '#'))
    {
        return None;
    }
    let mut depth = 0usize;
    let mut parameters = String::new();
    for ch in rest.chars() {
        match ch {
            '<' => {
                depth += 1;
                if depth > 1 {
                    parameters.push(ch);
                }
            }
            '>' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    break;
                }
                parameters.push(ch);
            }
            ',' if depth == 1 => parameters.push('\n'),
            _ if depth > 0 => parameters.push(ch),
            _ => return None,
        }
    }
    if depth != 0 {
        return None;
    }
    Some(parameters.lines().any(|parameter| {
        let parameter = parameter.trim_start();
        if parameter.starts_with('\'') {
            return false;
        }
        let parameter = parameter
            .strip_prefix("const ")
            .unwrap_or(parameter)
            .trim_start();
        let name: String = parameter
            .chars()
            .take_while(|c| *c == '_' || c.is_alphanumeric())
            .collect();
        target.contains(&name)
    }))
}

pub(super) fn derive(artifact: &Artifact, symbol: &str) -> Navigation {
    let mut navigation = Navigation {
        path: artifact.path.clone(),
        ..Default::default()
    };
    if artifact.language != "rust" {
        return navigation;
    }
    if artifact.facts.len() > MAX_FACTS {
        navigation.truncated = true;
        return navigation;
    }
    if artifact.facts.iter().any(|fact| {
        fact.kind == "definition"
            && matches!(
                fact.syntax_kind.as_str(),
                "struct_item" | "enum_item" | "impl_item"
            )
            && fact.code.as_ref().is_some_and(|code| {
                code.producer == "airk-tree-sitter/rust" && code.details_truncated
            })
    }) {
        navigation.truncated = true;
        return navigation;
    }
    let mut anchors = artifact.facts.iter().filter(|fact| {
        native_definition(fact).is_some_and(|code| code.symbol_id == symbol)
            && matches!(
                fact.syntax_kind.as_str(),
                "struct_item" | "enum_item" | "impl_item"
            )
    });
    let Some(anchor) = anchors.next() else {
        return navigation;
    };
    if anchors.next().is_some() {
        navigation.truncated = true;
        return navigation;
    }
    let mut declarations = BTreeMap::<Vec<String>, usize>::new();
    let mut by_type = BTreeMap::<Vec<String>, Vec<&Fact>>::new();
    let mut methods_by_owner = BTreeMap::<&str, Vec<&Fact>>::new();
    let mut incomplete_method_owners = BTreeSet::new();
    // Build bounded local indexes once, rather than re-scanning every retained
    // fact for each impl. MAX_FACTS and MAX_TARGETS bound both passes.
    for fact in &artifact.facts {
        match fact.syntax_kind.as_str() {
            "struct_item" | "enum_item" => {
                if let Some(path) = declared_type(fact) {
                    *declarations.entry(path).or_default() += 1;
                }
            }
            "impl_item" => {
                if let Some(path) = implemented_type(fact) {
                    by_type.entry(path).or_default().push(fact);
                }
            }
            "function_item" => {
                if fact.kind == "definition"
                    && let Some(code) = fact.code.as_ref()
                    && code.basis == EvidenceBasis::Syntax
                    && code.producer == "airk-tree-sitter/rust"
                    && code.details_truncated
                    && let Some(owner) = code.owner_id.as_deref()
                {
                    incomplete_method_owners.insert(owner);
                }
                if let Some(code) = native_definition(fact)
                    && let Some(owner) = code.owner_id.as_deref()
                {
                    methods_by_owner.entry(owner).or_default().push(fact);
                }
            }
            _ => {}
        }
    }
    let mut implementations = Vec::new();
    if anchor.syntax_kind == "impl_item" {
        if implemented_type(anchor).is_some() {
            implementations.push(anchor);
        }
    } else if let Some(path) = declared_type(anchor) {
        // Same spelling in cfg alternatives or incomplete syntax is ambiguous.
        // Do not choose one merely because it appeared first in retained facts.
        if declarations.get(&path) != Some(&1) {
            navigation.truncated = true;
            return navigation;
        }
        implementations = by_type.remove(&path).unwrap_or_default();
    }
    for implementation in implementations {
        let code = native_definition(implementation).expect("selected native implementation");
        navigation.truncated |= incomplete_method_owners.contains(code.symbol_id.as_str());
        if code.symbol_id != symbol {
            if navigation.contains.len() == MAX_TARGETS {
                navigation.truncated = true;
                break;
            }
            navigation.contains.push(RelationshipHit {
                source: symbol.into(),
                target: code.symbol_id.clone(),
                kind: "contains".into(),
                resolution: "syntax_candidate".into(),
                basis: EvidenceBasis::Syntax,
                producer: "airk-retained-rust-type-navigation".into(),
                producer_version: "type-navigation/1".into(),
                citation: fact_hit(artifact, implementation).citation,
            });
        }
        let Some(written_type) = type_path(&implementation.name) else {
            continue;
        };
        for method in methods_by_owner
            .get(code.symbol_id.as_str())
            .into_iter()
            .flatten()
        {
            let Some(method_code) = native_definition(method) else {
                continue;
            };
            if method.syntax_kind != "function_item"
                || method_code.owner_id.as_deref() != Some(code.symbol_id.as_str())
                || method_code.scope.len() != code.scope.len() + 1
                || !method_code.scope.starts_with(&code.scope)
                || method_code.scope.last() != Some(&implementation.name)
                || type_path(&method.name).is_none_or(|parts| parts.len() != 1)
            {
                continue;
            }
            if navigation.suggestions.len() == MAX_TARGETS {
                navigation.truncated = true;
                return navigation;
            }
            navigation.suggestions.push(RelationshipTargetSuggestion {
                target: format!("{}::{}", written_type.join("::"), method.name),
                symbol_id: method_code.symbol_id.clone(),
                citation: fact_hit(artifact, method).citation,
                resolution: "syntax_candidate".into(),
                basis: EvidenceBasis::Syntax,
                coverage: COVERAGE.into(),
            });
        }
    }
    navigation.suggestions.sort_by(|a, b| {
        (a.citation.start_line, &a.target, &a.symbol_id).cmp(&(
            b.citation.start_line,
            &b.target,
            &b.symbol_id,
        ))
    });
    navigation
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::DocumentLocation;

    fn definition(
        name: &str,
        syntax: &str,
        symbol: &str,
        scope: &[&str],
        owner: Option<&str>,
    ) -> Fact {
        Fact {
            name: name.into(),
            kind: "definition".into(),
            syntax_kind: syntax.into(),
            start_line: 1,
            end_line: 2,
            location: DocumentLocation::Lines { start: 1, end: 2 },
            document: None,
            code: Some(CodeEvidence {
                fingerprints: None,
                symbol_id: symbol.into(),
                qualified_name: scope
                    .iter()
                    .copied()
                    .chain([name])
                    .collect::<Vec<_>>()
                    .join("::"),
                scope: scope.iter().map(|s| (*s).into()).collect(),
                owner_id: owner.map(str::to_owned),
                signature: (syntax == "impl_item").then(|| format!("impl {name}")),
                span: None,
                signature_span: None,
                parameters: None,
                return_type: None,
                modifiers: Vec::new(),
                documentation: None,
                target: None,
                basis: EvidenceBasis::Syntax,
                producer: "airk-tree-sitter/rust".into(),
                producer_version: "native-facts/4".into(),
                coverage: "fixture syntax".into(),
                details_truncated: false,
            }),
        }
    }

    fn artifact(facts: Vec<Fact>) -> Artifact {
        Artifact {
            path: "one.rs".into(),
            content_hash: "0".repeat(64),
            bytes: 1,
            chunks: Vec::new(),
            language: "rust".into(),
            extraction_method: "utf8".into(),
            parse_status: "parsed".into(),
            facts,
            retained: None,
        }
    }

    #[test]
    fn explicit_impl_suggests_only_direct_owned_methods_with_complete_scope() {
        let mut value = artifact(vec![
            definition(
                "Session<T>",
                "impl_item",
                "impl",
                &["inner"],
                Some("module"),
            ),
            definition(
                "spawn",
                "function_item",
                "spawn",
                &["inner", "Session<T>"],
                Some("impl"),
            ),
            definition(
                "wrong_scope",
                "function_item",
                "wrong",
                &["other", "Session<T>"],
                Some("impl"),
            ),
            definition(
                "nested",
                "function_item",
                "nested",
                &["inner", "Session<T>", "spawn"],
                Some("spawn"),
            ),
        ]);
        let result = derive(&value, "impl");
        assert!(result.contains.is_empty());
        assert_eq!(result.suggestions.len(), 1);
        assert_eq!(result.suggestions[0].target, "Session::spawn");
        assert_eq!(result.suggestions[0].symbol_id, "spawn");
        assert_eq!(result.suggestions[0].citation.content_hash, "0".repeat(64));
        value.facts[1].code.as_mut().unwrap().details_truncated = true;
        let incomplete = derive(&value, "impl");
        assert!(incomplete.truncated && incomplete.suggestions.is_empty());
    }

    #[test]
    fn missing_or_truncated_type_evidence_never_guesses_an_alias_or_owner() {
        let mut value = artifact(vec![
            definition("Session", "struct_item", "type", &[], None),
            definition("Alias", "impl_item", "alias", &[], None),
            definition("spawn", "function_item", "spawn", &["Alias"], Some("alias")),
        ]);
        assert!(derive(&value, "type").suggestions.is_empty());
        value.facts[1].name = "Session".into();
        value.facts[2].code.as_mut().unwrap().scope = vec!["Session".into()];
        assert_eq!(derive(&value, "type").suggestions.len(), 1);
        value.facts[0].code.as_mut().unwrap().details_truncated = true;
        let omitted = derive(&value, "type");
        assert!(omitted.contains.is_empty() && omitted.suggestions.is_empty());
        value.facts[0].code.as_mut().unwrap().details_truncated = false;
        value.facts.push(value.facts[0].clone());
        assert!(derive(&value, "type").suggestions.is_empty());
        value.language = "typescript".into();
        assert!(derive(&value, "alias").suggestions.is_empty());
    }

    #[test]
    fn generic_paths_are_finite_and_unsupported_type_expressions_stay_unknown() {
        assert_eq!(type_path("Session<T>"), Some(vec!["Session".into()]));
        assert_eq!(
            type_path("inner::Session<Vec<T>>"),
            Some(vec!["inner".into(), "Session".into()])
        );
        assert_eq!(type_path("self::Session<T>"), Some(vec!["Session".into()]));
        for expression in [
            "Session<",
            "Session>",
            "&Session",
            "dyn Session",
            "crate::Session",
            "super::Session",
            "Self",
            "::Session",
            "1Session",
        ] {
            assert!(type_path(expression).is_none(), "{expression}");
        }
    }

    #[test]
    fn fact_ceiling_discloses_omission_instead_of_assuming_uniqueness() {
        let fact = definition("Session", "struct_item", "type", &[], None);
        let result = derive(&artifact(vec![fact; MAX_FACTS + 1]), "type");
        assert!(result.truncated && result.contains.is_empty() && result.suggestions.is_empty());
    }

    #[test]
    fn generic_parameters_do_not_bind_a_same_named_concrete_type() {
        let target = vec!["Session".into()];
        assert_eq!(
            generic_parameter_shadows("impl<T> Trait for Session<T>", &target),
            Some(false)
        );
        assert_eq!(
            generic_parameter_shadows("impl<Session: Trait> Trait for Session", &target),
            Some(true)
        );
        assert_eq!(
            generic_parameter_shadows("impl<A: Fn() -> (), Session> Trait for Session", &target),
            None
        );
        assert_eq!(
            generic_parameter_shadows(
                "impl<#[allow(unused)] Session: Trait> Trait for Session",
                &target
            ),
            None
        );
        let mut value = artifact(vec![
            definition("Session", "struct_item", "type", &[], None),
            definition("Session", "impl_item", "generic_impl", &[], None),
            definition(
                "spawn",
                "function_item",
                "spawn",
                &["Session"],
                Some("generic_impl"),
            ),
        ]);
        value.facts[1].code.as_mut().unwrap().signature =
            Some("impl<Session: Trait> Trait for Session".into());
        let result = derive(&value, "type");
        assert!(result.contains.is_empty() && result.suggestions.is_empty());
    }

    #[test]
    fn many_implementations_share_one_owner_index_and_disclose_target_limit() {
        let mut facts = vec![definition("Session", "struct_item", "type", &[], None)];
        for index in 0..MAX_TARGETS + 20 {
            let owner = format!("impl-{index}");
            facts.push(definition("Session", "impl_item", &owner, &[], None));
            facts.push(definition(
                &format!("method_{index}"),
                "function_item",
                &format!("method-{index}"),
                &["Session"],
                Some(&owner),
            ));
        }
        let result = derive(&artifact(facts), "type");
        assert!(result.truncated);
        assert_eq!(result.contains.len(), MAX_TARGETS);
        assert_eq!(result.suggestions.len(), MAX_TARGETS);
    }
}
