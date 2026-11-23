// SPDX-License-Identifier: Apache-2.0
//! Literal ONNX protobuf metadata only; no ONNX library, allocation or forward.
use super::*;
fn varint(mut n: u64) -> Vec<u8> {
    let mut v = Vec::new();
    while n >= 128 {
        v.push(n as u8 | 128);
        n >>= 7;
    }
    v.push(n as u8);
    v
}
fn bytes(tag: u64, v: &[u8]) -> Vec<u8> {
    [varint(tag * 8 + 2), varint(v.len() as u64), v.to_vec()].concat()
}
fn int(tag: u64, v: u64) -> Vec<u8> {
    [varint(tag * 8), varint(v)].concat()
}
fn ia(name: &str, value: i64) -> Vec<u8> {
    [bytes(1, name.as_bytes()), int(3, value as u64), int(20, 2)].concat()
}
fn fa(name: &str, value: u32) -> Vec<u8> {
    [
        bytes(1, name.as_bytes()),
        varint(21),
        value.to_le_bytes().to_vec(),
        int(20, 1),
    ]
    .concat()
}
fn tensor(element: u64, offset: u64, length: u64, dimension: u64, name: &str) -> Vec<u8> {
    let mut t = [
        int(1, dimension),
        int(2, element),
        bytes(8, name.as_bytes()),
        int(14, 1),
    ]
    .concat();
    for (k, v) in [
        ("location", "weights.bin".into()),
        ("offset", offset.to_string()),
        ("length", length.to_string()),
    ] {
        t.extend(bytes(
            13,
            &[bytes(1, k.as_bytes()), bytes(2, v.as_bytes())].concat(),
        ));
    }
    t
}
fn cast(name: &str, element: i64, input: &str) -> Vec<u8> {
    [
        bytes(1, input.as_bytes()),
        bytes(2, name.as_bytes()),
        bytes(3, name.as_bytes()),
        bytes(4, b"Cast"),
        bytes(5, &ia("to", element)),
    ]
    .concat()
}
struct Config {
    focus: bool,
    layers: usize,
    heads: i64,
    scale: u32,
    kv: i64,
    causal: i64,
    bias: bool,
    cast: i64,
    half: bool,
    overlap: bool,
    sanitize: bool,
    present_outputs: &'static [&'static str],
}
fn config(focus: bool, layers: usize, heads: i64) -> Config {
    Config {
        focus,
        layers,
        heads,
        scale: 0x3db504f3,
        kv: 8,
        causal: 1,
        bias: false,
        cast: 10,
        half: true,
        overlap: false,
        sanitize: true,
        present_outputs: if focus {
            &[]
        } else {
            &["present_key", "present_value"]
        },
    }
}
fn graph(c: &Config) -> (Vec<u8>, BTreeMap<String, u64>) {
    graph_with_extra(c, &[])
}
fn graph_with_extra(c: &Config, extra: &[u8]) -> (Vec<u8>, BTreeMap<String, u64>) {
    let mut g = Vec::new();
    let halfwidth = if c.half { 2 } else { 4 };
    g.extend(bytes(
        5,
        &tensor(
            if c.half { 10 } else { 1 },
            0,
            halfwidth,
            1,
            "literal-learned",
        ),
    ));
    g.extend(bytes(
        5,
        &tensor(
            1,
            if c.overlap { 1 } else { halfwidth },
            4,
            1,
            "literal-protected-norm",
        ),
    ));
    for i in 0..c.layers {
        let stem = format!("/airk_g5/attention/{i}/");
        for (suffix, ty) in [("q16", c.cast), ("k16", 10), ("v16", 10), ("output32", 1)] {
            let input = if suffix == "output32" {
                format!(
                    "{stem}{}",
                    if c.focus {
                        "zero_padded_queries"
                    } else {
                        "raw16"
                    }
                )
            } else {
                "literal-parent".into()
            };
            g.extend(bytes(1, &cast(&format!("{stem}{suffix}"), ty, &input)));
        }
        let mut n = Vec::new();
        for s in ["q16", "k16", "v16"] {
            n.extend(bytes(1, format!("{stem}{s}").as_bytes()));
        }
        for v in if c.focus {
            vec![if c.bias { "literal-bias" } else { "" }, "packed-mask"]
        } else {
            vec![
                if c.bias { "literal-cache" } else { "" },
                "",
                "seqlens-int32",
                "total-int32",
            ]
        } {
            n.extend(bytes(1, v.as_bytes()));
        }
        n.extend(bytes(2, format!("{stem}raw16").as_bytes()));
        for suffix in c.present_outputs {
            n.extend(bytes(2, format!("{stem}{suffix}").as_bytes()));
        }
        n.extend(bytes(3, format!("{stem}efficient").as_bytes()));
        n.extend(bytes(
            4,
            if c.focus {
                b"MultiHeadAttention"
            } else {
                b"GroupQueryAttention"
            },
        ));
        n.extend(bytes(7, b"com.microsoft"));
        n.extend(bytes(5, &ia("num_heads", c.heads)));
        n.extend(bytes(5, &fa("scale", c.scale)));
        if c.focus {
            n.extend(bytes(5, &ia("unidirectional", 1)));
        } else {
            for (k, v) in [
                ("kv_num_heads", c.kv),
                ("causal", c.causal),
                ("local_window_size", -1),
                ("do_rotary", 0),
            ] {
                n.extend(bytes(5, &ia(k, v)));
            }
        }
        g.extend(bytes(1, &n));
        if c.focus && c.sanitize {
            let name = format!("{stem}zero_padded_queries");
            let n = [
                bytes(1, b"/airk_g5/live_queries"),
                bytes(1, format!("{stem}raw16").as_bytes()),
                bytes(1, b"/airk_g5/half_zero"),
                bytes(2, name.as_bytes()),
                bytes(3, name.as_bytes()),
                bytes(4, b"Where"),
            ]
            .concat();
            g.extend(bytes(1, &n));
        }
    }
    g.extend_from_slice(extra);
    (
        bytes(7, &g),
        BTreeMap::from([("weights.bin".into(), halfwidth + 4)]),
    )
}
#[test]
fn two_literal_selected_rosters_admit_actual_half_boundaries() -> Result<()> {
    for (choice, c, expected) in [
        (ModelChoice::FocusQwenReranker06B, config(true, 28, 16), 28),
        (ModelChoice::ContextQwenCompact, config(false, 28, 16), 28),
    ] {
        let (g, f) = graph(&c);
        let nodes = audit(&g, &f, choice)?;
        assert_eq!(nodes.len(), expected);
        assert!(nodes.iter().all(|n| n.domain == "com.microsoft"));
    }
    // The retired Large roster (36 layers, 32 heads) is no longer admitted.
    let (g, f) = graph(&config(false, 36, 32));
    assert!(audit(&g, &f, ModelChoice::ContextQwenLarge).is_err());
    Ok(())
}
#[test]
fn wrong_scale_grouping_causal_and_bias_refuse_before_session() {
    for kind in 0..4 {
        let mut c = config(false, 28, 16);
        match kind {
            0 => c.scale = 0x3e000000,
            1 => c.kv = 4,
            2 => c.causal = 0,
            _ => c.bias = true,
        };
        let (g, f) = graph(&c);
        assert!(
            audit(&g, &f, ModelChoice::ContextQwenCompact).is_err(),
            "case {kind}"
        );
    }
}
#[test]
fn wrong_half_cast_and_missing_layer_refuse() {
    for c in [
        Config {
            cast: 1,
            ..config(false, 28, 16)
        },
        config(false, 27, 16),
    ] {
        let (g, f) = graph(&c);
        assert!(audit(&g, &f, ModelChoice::ContextQwenCompact).is_err());
    }
}
#[test]
fn focus_bias_and_wrong_head_count_refuse() {
    for c in [
        Config {
            bias: true,
            ..config(true, 28, 16)
        },
        config(true, 28, 8),
    ] {
        let (g, f) = graph(&c);
        assert!(audit(&g, &f, ModelChoice::FocusQwenReranker06B).is_err());
    }
}
#[test]
fn all_f32_and_overlap_are_not_half_weight_closure() {
    for c in [
        Config {
            half: false,
            ..config(false, 28, 16)
        },
        Config {
            overlap: true,
            ..config(false, 28, 16)
        },
    ] {
        let (g, f) = graph(&c);
        assert!(audit(&g, &f, ModelChoice::ContextQwenCompact).is_err());
    }
}
#[test]
fn large_half_metadata_is_additive_and_never_allocates_tensor_bytes() -> Result<()> {
    let model = bytes(
        7,
        &bytes(
            5,
            &tensor(10, 0, 1u64 << 31, 1u64 << 30, "literal-large-half"),
        ),
    );
    let files = BTreeMap::from([("weights.bin".into(), 1u64 << 31)]);
    assert!(super::super::large_initializers(&model, &files).is_err());
    let plan = super::super::half_initializers(&model, &files)?;
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].element, 10);
    assert_eq!(plan[0].length, 1u64 << 31);
    Ok(())
}
#[test]
fn unknown_quantized_half_closure_refuses() {
    let model = bytes(7, &bytes(5, &tensor(2, 0, 1, 1, "literal-u8")));
    assert!(
        super::super::half_initializers(&model, &BTreeMap::from([("weights.bin".into(), 1)]))
            .is_err()
    );
}

#[test]
fn focus_unwritten_padded_queries_require_explicit_sanitization() {
    let c = Config {
        sanitize: false,
        ..config(true, 28, 16)
    };
    let (g, f) = graph(&c);
    assert!(audit(&g, &f, ModelChoice::FocusQwenReranker06B).is_err());
}

#[test]
fn context_present_outputs_are_required_in_exact_order() {
    for (model, layers, heads) in std::iter::once((ModelChoice::ContextQwenCompact, 28, 16)) {
        for present_outputs in [
            &[][..],
            &["present_key"][..],
            &["present_value", "present_key"][..],
            &["present_key", "present_key"][..],
            &["present_key", "foreign"][..],
        ] {
            let (g, f) = graph(&Config {
                present_outputs,
                ..config(false, layers, heads)
            });
            assert_eq!(
                audit(&g, &f, model).unwrap_err().to_string(),
                "G5 attention output catalog differs"
            );
        }
    }
    let (g, f) = graph(&Config {
        present_outputs: &["present_key", "present_value"],
        ..config(true, 28, 16)
    });
    assert_eq!(
        audit(&g, &f, ModelChoice::FocusQwenReranker06B)
            .unwrap_err()
            .to_string(),
        "G5 attention output catalog differs"
    );
}

#[test]
fn context_present_outputs_cannot_collide_escape_or_be_consumed() -> Result<()> {
    let c = config(false, 28, 16);
    let model = ModelChoice::ContextQwenCompact;
    let name = "/airk_g5/attention/0/present_value";
    let value_info = bytes(1, name.as_bytes());
    let initializer = [
        int(1, 1),
        int(2, 10),
        bytes(8, name.as_bytes()),
        bytes(9, &[0, 0]),
    ]
    .concat();
    let nested = [
        bytes(2, b"nested-node"),
        bytes(4, b"PrivateGraph"),
        bytes(5, &bytes(6, &bytes(12, &value_info))),
    ]
    .concat();
    for (extra, expected) in [
        (
            bytes(1, &nested),
            "G5 Context present outputs require one graph scope",
        ),
        (
            bytes(1, &cast("consumer", 10, name)),
            "G5 Context present output has a node consumer",
        ),
        (
            bytes(1, &cast(name, 10, "literal-parent")),
            "G5 Context present output collides with another node output",
        ),
        (
            bytes(5, &initializer),
            "G5 Context present output collides with a graph endpoint or initializer",
        ),
        (
            bytes(15, &bytes(1, &initializer)),
            "G5 Context present output collides with a graph endpoint or initializer",
        ),
        (
            bytes(11, &value_info),
            "G5 Context present output collides with a graph endpoint or initializer",
        ),
        (
            bytes(12, &value_info),
            "G5 Context present output collides with a graph endpoint or initializer",
        ),
    ] {
        let (g, f) = graph_with_extra(&c, &extra);
        assert_eq!(audit(&g, &f, model).unwrap_err().to_string(), expected);
    }
    // A private value may have optional metadata without becoming an endpoint.
    let (g, f) = graph_with_extra(&c, &bytes(13, &value_info));
    audit(&g, &f, model)?;
    Ok(())
}
