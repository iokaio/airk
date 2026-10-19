// SPDX-License-Identifier: Apache-2.0
//! Detail-independent written boundaries. These are hints, never resolved semantics.
use airk_core::{
    DocumentLocation,
    extraction::{
        ExtractionPayload, LocalSpan, MAX_SEMANTIC_BOUNDARIES, SemanticBoundary, SemanticProjection,
    },
};
use anyhow::{Result, ensure};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
struct Node {
    span: LocalSpan,
    signature: Option<LocalSpan>,
    heading: bool,
    children: Vec<usize>,
}

fn hints(payload: &ExtractionPayload) -> BTreeMap<Option<u32>, Vec<Node>> {
    let identities: BTreeMap<_, _> = payload
        .syntax_identities
        .iter()
        .map(|i| (i.id, i))
        .collect();
    let mut result: BTreeMap<_, Vec<Node>> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut headings: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for fact in &payload.facts {
        if fact.kind == "definition"
            && let Some(code) = &fact.code
            && let Some(identity) = identities.get(&code.syntax_id)
            && identity.region == fact.region
            && identity.span.start < identity.span.end
            && seen.insert(code.syntax_id)
        {
            let signature = code
                .signature_span
                .as_ref()
                .filter(|s| {
                    s.start_byte >= identity.span.start
                        && s.end_byte <= identity.span.end
                        && s.start_byte < s.end_byte
                })
                .map(|s| LocalSpan {
                    start: s.start_byte,
                    end: s.end_byte,
                });
            result.entry(fact.region).or_default().push(Node {
                span: identity.span.clone(),
                signature,
                heading: false,
                children: Vec::new(),
            });
        }
        if fact.kind == "section"
            && let Some(document) = &fact.document
            && matches!(
                document.basis.as_str(),
                "explicit-heading" | "heuristic-heading"
            )
            && fact.span.start < fact.span.end
        {
            headings
                .entry(fact.region)
                .or_default()
                .push((fact.span.clone(), document.heading_path.len().max(1)));
        }
    }
    for (region, mut rows) in headings {
        rows.sort_by_key(|(span, depth)| (span.start, *depth, span.end));
        rows.dedup_by_key(|(span, _)| span.start);
        let end = region.map_or(payload.text.len(), |id| {
            payload
                .regions
                .iter()
                .find(|r| r.id == id)
                .map_or(0, |r| r.text.len())
        });
        let nodes = result.entry(region).or_default();
        let mut stack: Vec<(usize, usize)> = Vec::new();
        for (span, depth) in rows {
            while stack
                .last()
                .is_some_and(|(_, old_depth)| *old_depth >= depth)
            {
                let (index, _) = stack.pop().unwrap();
                nodes[index].span.end = span.start;
            }
            let index = nodes.len();
            nodes.push(Node {
                span: LocalSpan {
                    start: span.start,
                    end,
                },
                signature: Some(span),
                heading: true,
                children: Vec::new(),
            });
            stack.push((index, depth));
        }
    }
    for nodes in result.values_mut() {
        nodes.sort_by_key(|node| {
            (
                node.span.start,
                std::cmp::Reverse(node.span.end),
                !node.heading,
            )
        });
        nodes.dedup_by(|right, left| right.span == left.span);
    }
    result
}

/// Native pages/rows/blocks and mapped regions are hard domains. Ordinary line
/// chunk boundaries are presentation choices and do not prevent coalescing.
fn slabs(payload: &ExtractionPayload) -> Vec<(Option<u32>, LocalSpan)> {
    let mut domains = BTreeMap::from([(None, payload.text.len())]);
    domains.extend(payload.regions.iter().map(|r| (Some(r.id), r.text.len())));
    let mut cuts: BTreeMap<_, BTreeSet<_>> = domains
        .iter()
        .map(|(region, len)| (*region, BTreeSet::from([0, *len])))
        .collect();
    for chunk in &payload.chunks {
        if !matches!(chunk.location, DocumentLocation::Lines { .. })
            && let Some(domain) = cuts.get_mut(&chunk.region)
        {
            domain.extend([chunk.span.start, chunk.span.end]);
        }
    }
    cuts.into_iter()
        .flat_map(|(region, boundaries)| {
            let boundaries: Vec<_> = boundaries.into_iter().collect();
            boundaries
                .windows(2)
                .filter(|p| p[0] < p[1])
                .map(|p| {
                    (
                        region,
                        LocalSpan {
                            start: p[0],
                            end: p[1],
                        },
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

enum Task {
    Visit(usize, u32, Option<LocalSpan>),
    Emit(LocalSpan, u32, Option<LocalSpan>, Option<LocalSpan>),
}

fn project_slab(
    region: Option<u32>,
    slab: &LocalSpan,
    candidates: &[Node],
    next_group: &mut u32,
) -> (Vec<SemanticBoundary>, bool) {
    let root_group = *next_group;
    *next_group += 1;
    let fallback = || {
        vec![SemanticBoundary {
            region,
            span: slab.clone(),
            group: root_group,
            context: None,
            split_context: None,
        }]
    };
    let mut nodes = vec![Node {
        span: slab.clone(),
        signature: None,
        heading: false,
        children: Vec::new(),
    }];
    let mut stack = vec![0];
    for candidate in candidates {
        // Context cannot escape a native location. Crossing hints are discarded;
        // the slab body is still indexed in full.
        if candidate.span.start < slab.start || candidate.span.end > slab.end {
            continue;
        }
        while stack.len() > 1 && candidate.span.start >= nodes[*stack.last().unwrap()].span.end {
            stack.pop();
        }
        let parent = *stack.last().unwrap();
        if candidate.span.end > nodes[parent].span.end || stack.len() > 128 {
            return (fallback(), true);
        }
        let index = nodes.len();
        nodes[parent].children.push(index);
        nodes.push(candidate.clone());
        stack.push(index);
    }
    if nodes.len() == 1 {
        return (fallback(), false);
    }
    let mut output = Vec::new();
    let mut tasks = vec![Task::Visit(0, root_group, None)];
    while let Some(task) = tasks.pop() {
        match task {
            Task::Emit(span, group, context, split_context) => {
                if span.start < span.end {
                    output.push(SemanticBoundary {
                        region,
                        span,
                        group,
                        context,
                        split_context,
                    });
                }
            }
            Task::Visit(index, parent_group, parent_context) => {
                let node = &nodes[index];
                let (group, context) = if index != 0 && (node.heading || !node.children.is_empty())
                {
                    let group = *next_group;
                    *next_group += 1;
                    (group, node.signature.clone())
                } else {
                    (parent_group, parent_context)
                };
                if node.children.is_empty() {
                    tasks.push(Task::Emit(
                        node.span.clone(),
                        group,
                        context,
                        (!node.heading).then(|| node.signature.clone()).flatten(),
                    ));
                } else {
                    let mut end = node.span.end;
                    for child in node.children.iter().rev() {
                        tasks.push(Task::Emit(
                            LocalSpan {
                                start: nodes[*child].span.end,
                                end,
                            },
                            group,
                            context.clone(),
                            None,
                        ));
                        tasks.push(Task::Visit(*child, group, context.clone()));
                        end = nodes[*child].span.start;
                    }
                    tasks.push(Task::Emit(
                        LocalSpan {
                            start: node.span.start,
                            end,
                        },
                        group,
                        context,
                        None,
                    ));
                }
            }
        }
    }
    (output, false)
}

pub(crate) fn capture(payload: &mut ExtractionPayload) -> Result<SemanticProjection> {
    let hints = hints(payload);
    let slabs = slabs(payload);
    ensure!(
        slabs.len() <= MAX_SEMANTIC_BOUNDARIES,
        "native semantic domains exceed bound"
    );
    let mut boundaries = Vec::new();
    let mut group = 0;
    let mut discarded = false;
    // Each hint is inspected only in its intersecting slab, rather than scanning
    // the whole fact table for every native page or region.
    let mut cursors = BTreeMap::<Option<u32>, usize>::new();
    for (region, slab) in &slabs {
        let nodes = hints.get(region).map_or(&[][..], Vec::as_slice);
        let cursor = cursors.entry(*region).or_default();
        while *cursor < nodes.len() && nodes[*cursor].span.start < slab.start {
            *cursor += 1;
        }
        let end = *cursor + nodes[*cursor..].partition_point(|node| node.span.start < slab.end);
        let (mut projected, ambiguous) =
            project_slab(*region, slab, &nodes[*cursor..end], &mut group);
        *cursor = end;
        discarded |= ambiguous;
        boundaries.append(&mut projected);
        if boundaries.len() > MAX_SEMANTIC_BOUNDARIES {
            boundaries = slabs
                .iter()
                .enumerate()
                .map(|(index, (region, span))| SemanticBoundary {
                    region: *region,
                    span: span.clone(),
                    group: index as u32,
                    context: None,
                    split_context: None,
                })
                .collect();
            discarded = true;
            break;
        }
    }
    if discarded && payload.diagnostics.len() < 256 {
        payload.diagnostics.push("semantic boundary hints coarsened after ambiguity or metadata bound; body coverage retained".into());
    }
    Ok(SemanticProjection {
        version: 1,
        boundaries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{Artifact, extraction::IndexDetail};
    use sha2::{Digest, Sha256};

    fn extract(path: &str, text: &str, detail: IndexDetail) -> Result<ExtractionPayload> {
        let registry = crate::adapters::registry::AdapterRegistry::baseline();
        let policy = crate::adapters::profile::ExtractionPolicy {
            version: 3,
            detail: Some(detail),
            rules: vec![],
        };
        let profile = policy
            .select_with_prefix(&registry, path, text.as_bytes())?
            .profile;
        if crate::composite::is_host(&profile.adapter_id) {
            return crate::composite::extract(path, text.as_bytes(), &profile);
        }
        let extracted = crate::documents::extract_selected(path, text.as_bytes(), &profile)?;
        let artifact = Artifact {
            path: path.into(),
            content_hash: format!("{:x}", Sha256::digest(text.as_bytes())),
            bytes: text.len(),
            chunks: extracted.chunks,
            language: extracted.language,
            extraction_method: extracted.method,
            parse_status: extracted.parse_status,
            facts: extracted.facts,
            retained: None,
        };
        crate::continuity::prepare_native(&artifact, text.as_bytes(), &profile)
    }

    fn assert_partition(payload: &ExtractionPayload) {
        for (region, text) in std::iter::once((None, &payload.text))
            .chain(payload.regions.iter().map(|r| (Some(r.id), &r.text)))
        {
            if !payload.chunks.iter().any(|c| c.region == region) {
                continue;
            }
            let mut cursor = 0;
            for boundary in payload
                .semantic_projection
                .boundaries
                .iter()
                .filter(|b| b.region == region)
            {
                assert_eq!(boundary.span.start, cursor);
                assert!(text.is_char_boundary(boundary.span.end));
                cursor = boundary.span.end;
            }
            assert_eq!(cursor, text.len());
        }
    }

    #[test]
    fn real_code_and_prose_projection_is_independent_of_detail() -> Result<()> {
        for (path, text) in [
            (
                "original.rs",
                "// café\r\npub fn first() -> u32 { 1 }\r\npub fn second() -> u32 { 2 }\r\n",
            ),
            (
                "original.py",
                "class Service:\n    def start(self):\n        return 1\n    def stop(self):\n        return 2\n",
            ),
            (
                "original.md",
                "Preamble.\n# Repeated\nFirst body.\n## Nested\nNested body.\n# Repeated\nSecond body.\n",
            ),
        ] {
            let detailed = extract(path, text, IndexDetail::Detailed)?;
            assert_partition(&detailed);
            for detail in [IndexDetail::Essential, IndexDetail::Balanced] {
                let payload = extract(path, text, detail)?;
                assert_eq!(
                    payload.semantic_projection, detailed.semantic_projection,
                    "{path}"
                );
                assert_partition(&payload);
            }
        }
        Ok(())
    }

    #[test]
    fn tiny_siblings_share_group_and_container_context_but_keep_split_signatures() -> Result<()> {
        let top = extract(
            "original.rs",
            "pub fn first() -> u32 { 1 }\npub fn second() -> u32 { 2 }\n",
            IndexDetail::Detailed,
        )?;
        let leaves: Vec<_> = top
            .semantic_projection
            .boundaries
            .iter()
            .filter(|b| b.split_context.is_some())
            .collect();
        assert_eq!(leaves.len(), 2);
        assert_eq!(leaves[0].group, leaves[1].group);
        assert!(leaves.iter().all(|b| b.context.is_none()));
        assert_ne!(leaves[0].split_context, leaves[1].split_context);
        let class = extract(
            "original.py",
            "class Service:\n    def start(self):\n        return 1\n    def stop(self):\n        return 2\n",
            IndexDetail::Detailed,
        )?;
        let methods: Vec<_> = class
            .semantic_projection
            .boundaries
            .iter()
            .filter(|b| b.split_context.is_some())
            .collect();
        assert_eq!(methods.len(), 2);
        assert_eq!(methods[0].group, methods[1].group);
        assert!(methods[0].context.is_some());
        assert_eq!(methods[0].context, methods[1].context);
        Ok(())
    }

    #[test]
    fn duplicate_headings_have_distinct_groups_and_fact_order_does_not_matter() -> Result<()> {
        let mut payload = extract(
            "original.md",
            "# Same\nFirst.\n# Same\nSecond.\n",
            IndexDetail::Detailed,
        )?;
        let projected = capture(&mut payload)?;
        assert_eq!(projected.boundaries.len(), 2);
        assert_ne!(projected.boundaries[0].group, projected.boundaries[1].group);
        payload.facts.reverse();
        assert_eq!(capture(&mut payload)?, projected);
        Ok(())
    }

    #[test]
    fn crossing_hints_coarsen_without_losing_body() {
        let node = |start, end| Node {
            span: LocalSpan { start, end },
            signature: None,
            heading: false,
            children: vec![],
        };
        let slab = LocalSpan { start: 0, end: 100 };
        let (projection, discarded) =
            project_slab(None, &slab, &[node(1, 60), node(30, 90)], &mut 0);
        assert!(discarded);
        assert_eq!(projection.len(), 1);
        assert_eq!(projection[0].span, slab);
    }

    #[test]
    fn native_slabs_and_notebook_domains_keep_original_boundaries() -> Result<()> {
        let mut native = extract(
            "original.txt",
            "page one\npage two\n",
            IndexDetail::Detailed,
        )?;
        native.chunks = vec![
            airk_core::extraction::LocalChunk {
                span: LocalSpan { start: 0, end: 9 },
                start_line: 1,
                end_line: 1,
                location: DocumentLocation::Page { number: 1 },
                region: None,
            },
            airk_core::extraction::LocalChunk {
                span: LocalSpan { start: 9, end: 18 },
                start_line: 2,
                end_line: 2,
                location: DocumentLocation::Page { number: 2 },
                region: None,
            },
        ];
        native.semantic_projection = capture(&mut native)?;
        assert_eq!(native.semantic_projection.boundaries.len(), 2);
        assert_ne!(
            native.semantic_projection.boundaries[0].group,
            native.semantic_projection.boundaries[1].group
        );
        let notebook = extract(
            "original.ipynb",
            r#"{"cells":[{"cell_type":"code","metadata":{},"source":["def café():\n","    return 7\n"],"outputs":[],"execution_count":null}],"metadata":{"kernelspec":{"language":"python","name":"python3","display_name":"Python"}},"nbformat":4,"nbformat_minor":5}"#,
            IndexDetail::Detailed,
        )?;
        assert!(!notebook.semantic_projection.boundaries.is_empty());
        assert!(
            notebook
                .semantic_projection
                .boundaries
                .iter()
                .all(|b| b.region.is_some())
        );
        assert_partition(&notebook);
        Ok(())
    }
}
