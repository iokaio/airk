// SPDX-License-Identifier: Apache-2.0
use super::*;
pub(super) fn reader_context(
    mut node: Node<'_>,
    source: &str,
    budget: &mut HostBudget,
) -> Result<Vec<String>> {
    let mut context = Vec::new();
    for _ in 0..128 {
        let Some(parent) = node.parent() else {
            context.reverse();
            return Ok(context);
        };
        budget.charge(1, 0, 0, 0)?;
        if matches!(parent.kind(), "read_cond_lit" | "splicing_read_cond_lit") {
            let mut prior = node.prev_named_sibling();
            while prior.is_some_and(|n| n.kind() == "comment") {
                budget.charge(1, 0, 0, 0)?;
                prior = prior.and_then(|n| n.prev_named_sibling());
            }
            if let Some(feature) = prior.filter(|n| n.kind() == "kwd_lit") {
                ensure!(
                    feature.end_byte() - feature.start_byte() <= 1024,
                    "reader feature byte limit"
                );
                context.push(format!("reader:{}", text(feature, source)));
            }
        }
        node = parent;
    }
    anyhow::bail!("reader context depth limit")
}
pub(super) fn select<'a>(node: Node<'a>, source: &str, edn: bool) -> Vec<Cue<'a>> {
    let structural = |kind, name: &str| {
        let mut c = cue(node, kind, name, false);
        c.structural = true;
        c
    };
    if matches!(node.kind(), "read_cond_lit" | "splicing_read_cond_lit") {
        return vec![structural("reader_condition", text(node, source))];
    }
    if edn {
        return if node.kind() == "map_lit" {
            let mut result = vec![structural("data_map", text(node, source))];
            let mut cursor = node.walk();
            for (index, key) in node
                .children_by_field_name("value", &mut cursor)
                .take(20_002)
                .enumerate()
            {
                if index % 2 == 0 {
                    let mut selected = cue(key, "data_key", text(key, source), false);
                    selected.structural = true;
                    result.push(selected);
                }
            }
            result
        } else {
            vec![]
        };
    }
    if node.kind() != "list_lit" {
        return vec![];
    }
    let mut cursor = node.walk();
    let children = node
        .children_by_field_name("value", &mut cursor)
        .take(3)
        .collect::<Vec<_>>();
    let Some(head) = children
        .first()
        .filter(|n| matches!(n.kind(), "sym_lit" | "kwd_lit"))
    else {
        return vec![];
    };
    let head = text(*head, source);
    let name = children.get(1).map(|n| text(*n, source)).unwrap_or("");
    match head {
        "ns" => vec![
            structural("namespace", name),
            structural("namespace_form", text(node, source)),
        ],
        "def" | "defn" | "defn-" | "defmacro" | "defmulti" | "defmethod" => {
            vec![structural("written_definition", name)]
        }
        "require" | "use" | "import" | ":require" | ":require-macros" | ":import" => {
            vec![structural("import", text(node, source))]
        }
        _ if head.starts_with(':') => vec![],
        _ => vec![structural("call", head)],
    }
}
