// SPDX-License-Identifier: Apache-2.0
//! Metadata-only G5 attention audit. Conversion/numerical proof is independent.
use super::*;
use airk_core::arbiter::ModelChoice;

const CONTEXT_PRESENT_OUTPUTS: [&str; 2] = ["present_key", "present_value"];
#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct AttentionNode {
    pub name: String,
    pub operator: String,
    pub domain: String,
}
#[derive(Clone, Debug)]
enum Attribute {
    Integer(i64),
    Float(u32),
}
fn text(bytes: &[u8]) -> Result<String> {
    let s = std::str::from_utf8(bytes)?;
    ensure!(
        !s.is_empty() && s.len() <= 1024 && !s.contains('\0'),
        "G5 ONNX text invalid"
    );
    Ok(s.into())
}
fn attribute(raw: &[u8]) -> Result<(String, Attribute)> {
    let mut c = Cursor(raw);
    let mut name = None;
    let mut integer = None;
    let mut float = None;
    let mut kind = None;
    while let Some((field, value)) = c.field()? {
        match (field, value) {
            (1, Field::Bytes(v)) => ensure!(
                name.replace(text(v)?).is_none(),
                "duplicate G5 attribute name"
            ),
            (2, Field::FloatBits(v)) => {
                ensure!(float.replace(v).is_none(), "duplicate G5 float attribute")
            }
            (3, Field::Integer(v)) => ensure!(
                integer.replace(v as i64).is_none(),
                "duplicate G5 integer attribute"
            ),
            (20, Field::Integer(v)) => {
                ensure!(kind.replace(v).is_none(), "duplicate G5 attribute type")
            }
            _ => bail!("unsupported G5 attention attribute representation"),
        }
    }
    let value = match (kind, integer, float) {
        (Some(1), None, Some(v)) => Attribute::Float(v),
        (Some(2), Some(v), None) => Attribute::Integer(v),
        _ => bail!("ambiguous G5 attribute"),
    };
    Ok((name.context("G5 attribute name missing")?, value))
}
fn integer(a: &BTreeMap<String, Attribute>, name: &str, expected: i64) -> Result<()> {
    ensure!(
        matches!(a.get(name),Some(Attribute::Integer(v)) if *v==expected),
        "G5 integer attribute differs: {name}"
    );
    Ok(())
}
fn node(
    raw: &[u8],
    model: ModelChoice,
    private_outputs: &BTreeSet<String>,
) -> Result<Option<AttentionNode>> {
    let mut c = Cursor(raw);
    let mut name = None;
    let mut op = None;
    let mut domain = None;
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut attrs = Vec::new();
    while let Some((field, value)) = c.field()? {
        match (field, value) {
            (1, Field::Bytes(v)) => {
                ensure!(inputs.len() < 32, "G5 node input capacity");
                let input = std::str::from_utf8(v)?;
                ensure!(
                    !private_outputs.contains(input),
                    "G5 Context present output has a node consumer"
                );
                inputs.push(input.to_owned());
            }
            (2, Field::Bytes(v)) => {
                ensure!(outputs.len() < 16, "G5 node output capacity");
                outputs.push(text(v)?);
            }
            (3, Field::Bytes(v)) => {
                ensure!(name.replace(text(v)?).is_none(), "duplicate G5 node name")
            }
            (4, Field::Bytes(v)) => ensure!(op.replace(text(v)?).is_none(), "duplicate G5 node op"),
            (5, Field::Bytes(v)) => {
                ensure!(attrs.len() < 32, "G5 node attribute capacity");
                if !private_outputs.is_empty() {
                    let mut fields = Cursor(v);
                    while let Some((tag, _)) = fields.field()? {
                        ensure!(
                            !matches!(tag, 6 | 11),
                            "G5 Context present outputs require one graph scope"
                        );
                    }
                }
                attrs.push(v);
            }
            (7, Field::Bytes(v)) => {
                ensure!(domain.is_none(), "duplicate G5 node domain");
                domain = Some(std::str::from_utf8(v)?.to_owned());
            }
            (6, Field::Bytes(_)) => {} // doc_string does not affect graph semantics.
            _ => bail!("unsupported G5 node field"),
        }
    }
    let op = op.context("G5 op missing")?;
    ensure!(op != "Softmax", "G5 dense Softmax survived");
    ensure!(
        !matches!(op.as_str(), "Loop" | "If" | "Scan"),
        "G5 conditional/iterative graph unsupported"
    );
    let attention = matches!(
        op.as_str(),
        "GroupQueryAttention"
            | "MultiHeadAttention"
            | "Attention"
            | "PackedAttention"
            | "DecoderMaskedMultiHeadAttention"
    );
    if !attention {
        ensure!(
            outputs
                .iter()
                .all(|output| !private_outputs.contains(output)),
            "G5 Context present output collides with another node output"
        );
        return Ok(None);
    }
    let focus = model == ModelChoice::FocusQwenReranker06B;
    ensure!(
        op == if focus {
            "MultiHeadAttention"
        } else {
            "GroupQueryAttention"
        } && domain.as_deref() == Some("com.microsoft"),
        "G5 unexpected attention operator/domain"
    );
    let mut a = BTreeMap::new();
    for raw in attrs {
        let (key, value) = attribute(raw)?;
        ensure!(a.insert(key, value).is_none(), "duplicate G5 attribute");
    }
    ensure!(
        matches!(a.get("scale"), Some(Attribute::Float(0x3db504f3))),
        "G5 original head128 scale bits differ"
    );
    let heads = if model == ModelChoice::ContextQwenLarge {
        32
    } else {
        16
    };
    integer(&a, "num_heads", heads)?;
    if focus {
        ensure!(
            a.len() == 3
                && inputs.len() == 5
                && inputs[0..3].iter().all(|v| !v.is_empty())
                && inputs[3].is_empty()
                && !inputs[4].is_empty(),
            "G5 Focus requires packed mask and no bias/cache"
        );
        integer(&a, "unidirectional", 1)?;
    } else {
        ensure!(
            a.len() == 6
                && inputs.len() == 7
                && inputs[0..3].iter().all(|v| !v.is_empty())
                && inputs[3].is_empty()
                && inputs[4].is_empty()
                && !inputs[5].is_empty()
                && !inputs[6].is_empty(),
            "G5 Context requires seqlens/total and no bias or past inputs"
        );
        integer(&a, "kv_num_heads", 8)?;
        integer(&a, "causal", 1)?;
        integer(&a, "local_window_size", -1)?;
        integer(&a, "do_rotary", 0)?;
    }
    let name = name.context("G5 attention node name missing")?;
    let stem = name
        .strip_suffix("efficient")
        .context("G5 attention identity suffix differs")?;
    for (index, suffix) in ["q16", "k16", "v16"].iter().enumerate() {
        ensure!(
            inputs[index] == format!("{stem}{suffix}"),
            "G5 attention half boundary differs"
        );
    }
    let mut expected_outputs = vec![format!("{stem}raw16")];
    if !focus {
        expected_outputs.extend(
            CONTEXT_PRESENT_OUTPUTS
                .iter()
                .map(|suffix| format!("{stem}{suffix}")),
        );
    }
    ensure!(
        outputs == expected_outputs,
        "G5 attention output catalog differs"
    );
    Ok(Some(AttentionNode {
        name,
        operator: op,
        domain: "com.microsoft".into(),
    }))
}
fn boundary(raw: &[u8], choice: ModelChoice) -> Result<Option<(String, i64)>> {
    let mut c = Cursor(raw);
    let mut name = None;
    let mut op = None;
    let mut inputs = Vec::new();
    let mut output_count = 0;
    let mut attrs = Vec::new();
    while let Some((tag, value)) = c.field()? {
        match (tag, value) {
            (1, Field::Bytes(v)) => inputs.push(std::str::from_utf8(v)?.to_owned()),
            (2, Field::Bytes(_)) => output_count += 1,
            (3, Field::Bytes(v)) => name = Some(text(v)?),
            (4, Field::Bytes(v)) => op = Some(text(v)?),
            (5, Field::Bytes(v)) => attrs.push(v),
            _ => {}
        }
    }
    let Some(name) = name else { return Ok(None) };
    if !name.starts_with("/airk_g5/attention/")
        || !["/q16", "/k16", "/v16", "/output32"]
            .iter()
            .any(|s| name.ends_with(s))
    {
        return Ok(None);
    }
    ensure!(
        op.as_deref() == Some("Cast") && inputs.len() == 1 && output_count == 1 && attrs.len() == 1,
        "G5 precision boundary is not exact Cast"
    );
    if let Some(stem) = name.strip_suffix("output32") {
        let expected = format!(
            "{stem}{}",
            if choice == ModelChoice::FocusQwenReranker06B {
                "zero_padded_queries"
            } else {
                "raw16"
            }
        );
        ensure!(
            inputs[0] == expected,
            "G5 output32 bypasses actual attention/sanitizer"
        );
    }
    let (key, value) = attribute(attrs[0])?;
    let Attribute::Integer(element) = value else {
        bail!("G5 Cast target not integer")
    };
    ensure!(key == "to", "G5 Cast target attribute differs");
    Ok(Some((name, element)))
}
fn sanitize(raw: &[u8]) -> Result<Option<String>> {
    let mut c = Cursor(raw);
    let mut name = None;
    let mut op = None;
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut attrs = 0;
    while let Some((tag, v)) = c.field()? {
        match (tag, v) {
            (1, Field::Bytes(v)) => inputs.push(std::str::from_utf8(v)?.to_owned()),
            (2, Field::Bytes(v)) => outputs.push(text(v)?),
            (3, Field::Bytes(v)) => name = Some(text(v)?),
            (4, Field::Bytes(v)) => op = Some(text(v)?),
            (5, _) => attrs += 1,
            _ => {}
        }
    }
    let Some(name) = name else { return Ok(None) };
    if !name.starts_with("/airk_g5/attention/") || !name.ends_with("/zero_padded_queries") {
        return Ok(None);
    }
    let stem = name
        .strip_suffix("zero_padded_queries")
        .context("G5 sanitizer stem absent")?;
    ensure!(
        op.as_deref() == Some("Where")
            && attrs == 0
            && inputs
                == [
                    "/airk_g5/live_queries".into(),
                    format!("{stem}raw16"),
                    "/airk_g5/half_zero".into()
                ]
            && outputs.len() == 1
            && outputs[0] == name,
        "G5 padded-query sanitization differs"
    );
    Ok(Some(name))
}
fn require_unbound_name(
    raw: &[u8],
    name_field: u64,
    private_outputs: &BTreeSet<String>,
) -> Result<()> {
    let mut fields = Cursor(raw);
    while let Some((tag, value)) = fields.field()? {
        if tag == name_field
            && let Field::Bytes(name) = value
        {
            ensure!(
                !private_outputs.contains(std::str::from_utf8(name)?),
                "G5 Context present output collides with a graph endpoint or initializer"
            );
        }
    }
    Ok(())
}

/// Attention metadata only. Actual dtype and protected islands are additionally
/// pinned by the exported graph identity, final load/ABI, and independent proof.
pub(crate) fn audit(
    model: &[u8],
    files: &BTreeMap<String, u64>,
    choice: ModelChoice,
) -> Result<Vec<AttentionNode>> {
    half_initializers(model, files)?;
    ensure!(
        matches!(
            choice,
            ModelChoice::FocusQwenReranker06B
                | ModelChoice::ContextQwenCompact
                | ModelChoice::ContextQwenLarge
        ),
        "G5 unknown selected model"
    );
    let mut outer = Cursor(model);
    let mut graph = None;
    while let Some((tag, value)) = outer.field()? {
        if let (7, Field::Bytes(bytes)) = (tag, value) {
            ensure!(graph.replace(bytes).is_none(), "G5 duplicate graph");
        }
    }
    let mut graph = Cursor(graph.context("G5 graph missing")?);
    let layers = if choice == ModelChoice::ContextQwenLarge {
        36
    } else {
        28
    };
    let private_outputs = if choice == ModelChoice::FocusQwenReranker06B {
        BTreeSet::new()
    } else {
        (0..layers)
            .flat_map(|i| {
                CONTEXT_PRESENT_OUTPUTS
                    .iter()
                    .map(move |suffix| format!("/airk_g5/attention/{i}/{suffix}"))
            })
            .collect()
    };
    let mut attention = Vec::new();
    let mut count = 0usize;
    let mut boundaries = BTreeMap::new();
    let mut sanitized = BTreeSet::new();
    while let Some((tag, value)) = graph.field()? {
        let Field::Bytes(bytes) = value else { continue };
        match tag {
            1 => {
                count += 1;
                ensure!(count <= 65536, "G5 node capacity");
                if let Some(n) = node(bytes, choice, &private_outputs)? {
                    attention.push(n);
                }
                if let Some(name) = sanitize(bytes)? {
                    ensure!(
                        sanitized.insert(name),
                        "duplicate G5 padded-query sanitization"
                    );
                }
                if let Some((name, element)) = boundary(bytes, choice)? {
                    ensure!(
                        boundaries.insert(name, element).is_none(),
                        "duplicate G5 precision boundary"
                    );
                }
            }
            5 if !private_outputs.is_empty() => require_unbound_name(bytes, 8, &private_outputs)?,
            11 | 12 if !private_outputs.is_empty() => {
                require_unbound_name(bytes, 1, &private_outputs)?;
            }
            15 if !private_outputs.is_empty() => {
                // A SparseTensorProto declares its initializer name on values.
                let mut sparse = Cursor(bytes);
                while let Some((field, value)) = sparse.field()? {
                    if let (1, Field::Bytes(values)) = (field, value) {
                        require_unbound_name(values, 8, &private_outputs)?;
                    }
                }
            }
            _ => {}
        }
    }
    ensure!(
        attention.len() == layers,
        "G5 complete attention layer roster missing"
    );
    let expected = (0..layers)
        .map(|i| format!("/airk_g5/attention/{i}/efficient"))
        .collect::<BTreeSet<_>>();
    ensure!(
        attention
            .iter()
            .map(|n| n.name.clone())
            .collect::<BTreeSet<_>>()
            == expected,
        "G5 foreign/duplicate attention node identity"
    );
    ensure!(
        boundaries.len() == layers * 4,
        "G5 complete explicit precision boundaries missing"
    );
    for i in 0..layers {
        for (suffix, element) in [("q16", 10), ("k16", 10), ("v16", 10), ("output32", 1)] {
            ensure!(
                boundaries.get(&format!("/airk_g5/attention/{i}/{suffix}")) == Some(&element),
                "G5 actual boundary dtype differs"
            );
        }
    }
    let expected_sanitized = if choice == ModelChoice::FocusQwenReranker06B {
        (0..layers)
            .map(|i| format!("/airk_g5/attention/{i}/zero_padded_queries"))
            .collect()
    } else {
        BTreeSet::new()
    };
    ensure!(
        sanitized == expected_sanitized,
        "G5 complete padded-query sanitization missing"
    );
    attention.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(attention)
}
#[cfg(test)]
mod tests;
