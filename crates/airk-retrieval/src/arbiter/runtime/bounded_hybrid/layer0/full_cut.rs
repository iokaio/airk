// SPDX-License-Identifier: Apache-2.0
//! Four explicitly named outputs per diagnostic session, matching the admission ABI.
use super::*;

impl Scorer {
    pub const COMPACT_FULL_HEAD_TAPS: &'static [(&'static str, usize)] = &[
        ("/scorer/layers.0/input_layernorm/Mul_1_output_0", 2048),
        ("/scorer/layers.13/input_layernorm/Mul_1_output_0", 2048),
        ("/scorer/layers.27/input_layernorm/Mul_1_output_0", 2048),
        ("/scorer/norm/Mul_1_output_0", 2048),
    ];

    pub const COMPACT_FULL_ATTENTION_TAPS: &'static [(&'static str, usize)] = &[
        ("/airk_g5/attention/13/q16", 2048),
        ("/airk_g5/attention/13/k16", 1024),
        ("/airk_g5/attention/13/v16", 1024),
        ("/airk_g5/attention/13/raw16", 2048),
    ];

    pub const COMPACT_FULL_DENSE_TAPS: &'static [(&'static str, usize)] = &[
        ("/airk_g5/attention/13/raw16", 2048),
        ("/scorer/layers.13/self_attn/o_proj/MatMul_output_0", 2048),
        ("/scorer/layers.13/mlp/Mul_output_0", 6144),
        ("/scorer/layers.13/mlp/down_proj/MatMul_output_0", 2048),
    ];

    pub(in crate::arbiter::runtime) fn is_full_projection_tap_group(
        taps: &[(&str, usize)],
    ) -> bool {
        [
            Self::COMPACT_FULL_HEAD_TAPS,
            Self::COMPACT_FULL_ATTENTION_TAPS,
            Self::COMPACT_FULL_DENSE_TAPS,
        ]
        .contains(&taps)
    }
}

pub(super) fn fixed_batch(name: &str) -> bool {
    name == "/scorer/layers.0/input_layernorm/Mul_1_output_0"
}

#[test]
fn original_full_cut_has_one_head_and_all_normalized_inputs() {
    for taps in [
        Scorer::COMPACT_FULL_HEAD_TAPS,
        Scorer::COMPACT_FULL_ATTENTION_TAPS,
        Scorer::COMPACT_FULL_DENSE_TAPS,
    ] {
        // runtime.rs counts the original four declared intermediates in addition to scores.
        assert_eq!(taps.len(), super::COMPACT_FP16_LAYER0_TAPS.len());
        assert!(Scorer::is_full_projection_tap_group(taps));
    }
    let head = Scorer::COMPACT_FULL_HEAD_TAPS;
    assert_eq!(head[3], ("/scorer/norm/Mul_1_output_0", 2048));
    assert_eq!(head.iter().filter(|(name, _)| fixed_batch(name)).count(), 1);
    assert_eq!(
        Scorer::COMPACT_FULL_ATTENTION_TAPS[1],
        ("/airk_g5/attention/13/k16", 1024)
    );
    assert_eq!(
        Scorer::COMPACT_FULL_ATTENTION_TAPS[2],
        ("/airk_g5/attention/13/v16", 1024)
    );
    assert_eq!(
        Scorer::COMPACT_FULL_DENSE_TAPS[2],
        ("/scorer/layers.13/mlp/Mul_output_0", 6144)
    );
    assert!(!Scorer::is_full_projection_tap_group(&head[1..]));
}
