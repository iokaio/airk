// SPDX-License-Identifier: Apache-2.0
//! Cheap input refusal before tensor allocation or an ORT session lock.
use crate::arbiter::compiler::CompiledInput;
use airk_core::arbiter::{CompilerRecipe, Readout};
use anyhow::{Result, ensure};

pub(super) fn selected_graph_shape(
    name: &str,
    recipe: CompilerRecipe,
    maximum_tokens: usize,
    shape: &[i64],
) -> Result<()> {
    match recipe {
        CompilerRecipe::QwenRerankYesNoV1 => ensure!(
            shape == [1, maximum_tokens as i64],
            "Focus static graph width differs from its compiler/profile"
        ),
        CompilerRecipe::QwenSingleChoiceV1 => ensure!(
            if name == "decision_positions" {
                shape == [1]
            } else {
                shape == [1, -1]
            },
            "Context graph must preserve its declared dynamic token/option axes"
        ),
        CompilerRecipe::InternSingleChoiceV1
        | CompilerRecipe::StrandsSingleChoiceV1
        | CompilerRecipe::KevSingleChoiceV1 => {}
    }
    Ok(())
}

pub(super) fn validate(
    input: &CompiledInput,
    readout: Readout,
    maximum_tokens: usize,
    maximum_candidates: usize,
) -> Result<()> {
    let tokens = input.input_ids.len();
    let candidates = input.candidate_positions.len();
    let focus = readout == Readout::FocusYesNo;
    ensure!(
        tokens > 0
            && tokens <= maximum_tokens
            && candidates > 0
            && (if focus {
                candidates == 2
            } else {
                candidates <= maximum_candidates.min(8)
            })
            && input.input_ids.iter().all(|id| *id >= 0)
            && (if focus {
                tokens == maximum_tokens
                    && input.attention_mask.len() == tokens
                    && input
                        .attention_mask
                        .iter()
                        .all(|value| matches!(value, 0 | 1))
                    && input.attention_mask.last() == Some(&1)
                    && input
                        .attention_mask
                        .windows(2)
                        .all(|pair| pair[0] <= pair[1])
                    && input
                        .input_ids
                        .iter()
                        .zip(&input.attention_mask)
                        .all(|(id, mask)| *mask != 0 || *id == 151643)
                    && input
                        .input_ids
                        .iter()
                        .zip(&input.attention_mask)
                        .find_map(|(id, mask)| (*mask == 1).then_some(*id))
                        == Some(151644)
                    && input.decision_position == tokens as i64 - 1
                    && input.candidate_positions == [2152, 9693]
            } else {
                input.attention_mask == vec![1; tokens]
            })
            && input.position_ids.iter().copied().eq(0..tokens as i64)
            && input.option_mask == vec![true; candidates]
            && (0..tokens as i64).contains(&input.decision_position),
        "Arbiter compiled input differs from its qualified batch-one readout"
    );
    if readout == Readout::PointerHead {
        ensure!(
            input
                .candidate_positions
                .iter()
                .all(|position| (0..tokens as i64).contains(position)),
            "Arbiter pointer position is outside verified input"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::identity::Sha256Digest;
    fn focus() -> CompiledInput {
        CompiledInput {
            prompt: "synthetic".into(),
            input_ids: vec![151643, 151644, 6],
            attention_mask: vec![0, 1, 1],
            position_ids: vec![0, 1, 2],
            decision_position: 2,
            candidate_positions: vec![2152, 9693],
            option_mask: vec![true, true],
            input_digest: Sha256Digest::from_bytes([1; 32]),
        }
    }
    #[test]
    fn focus_raw_pair_is_not_the_original_pool_cardinality() -> Result<()> {
        validate(&focus(), Readout::FocusYesNo, 3, 1)?;
        validate(&focus(), Readout::FocusYesNo, 3, 64)?;
        Ok(())
    }
    #[test]
    fn selected_graph_static_width_and_dynamic_axes_refuse_drift() -> Result<()> {
        selected_graph_shape(
            "input_ids",
            CompilerRecipe::QwenRerankYesNoV1,
            512,
            &[1, 512],
        )?;
        assert!(
            selected_graph_shape(
                "input_ids",
                CompilerRecipe::QwenRerankYesNoV1,
                512,
                &[1, 128]
            )
            .is_err()
        );
        assert!(
            selected_graph_shape(
                "input_ids",
                CompilerRecipe::QwenRerankYesNoV1,
                512,
                &[1, -1]
            )
            .is_err()
        );
        selected_graph_shape(
            "candidate_positions",
            CompilerRecipe::QwenSingleChoiceV1,
            2048,
            &[1, -1],
        )?;
        selected_graph_shape(
            "decision_positions",
            CompilerRecipe::QwenSingleChoiceV1,
            2048,
            &[1],
        )?;
        assert!(
            selected_graph_shape(
                "input_ids",
                CompilerRecipe::QwenSingleChoiceV1,
                2048,
                &[1, 17]
            )
            .is_err()
        );
        Ok(())
    }
    #[test]
    fn focus_padding_holes_all_padding_and_nonbinary_masks_refuse() {
        for mask in [
            vec![1, 0, 1],
            vec![0, 0, 0],
            vec![-1, 1, 1],
            vec![0, 2, 1],
            vec![1, 1],
        ] {
            let mut input = focus();
            input.attention_mask = mask;
            assert!(validate(&input, Readout::FocusYesNo, 3, 64).is_err());
        }
    }
    #[test]
    fn focus_wrong_symbols_positions_width_and_option_drop_refuse() {
        for case in 0..7 {
            let mut input = focus();
            match case {
                0 => input.candidate_positions.reverse(),
                1 => input.decision_position = 1,
                2 => input.position_ids[1] = 0,
                3 => input.option_mask[1] = false,
                4 => input.input_ids[0] = -1,
                5 => input.input_ids[0] = 0,
                6 => input.input_ids[1] = 42,
                _ => unreachable!(),
            }
            assert!(validate(&input, Readout::FocusYesNo, 3, 64).is_err());
        }
        assert!(validate(&focus(), Readout::FocusYesNo, 4, 64).is_err());
    }
    #[test]
    fn context_retains_unpadded_complete_option_input_and_pointer_bounds() -> Result<()> {
        let mut input = focus();
        input.attention_mask = vec![1, 1, 1];
        input.candidate_positions = vec![32, 33];
        validate(&input, Readout::DecisionSymbol, 3, 2)?;
        assert!(validate(&input, Readout::PointerHead, 3, 2).is_err());
        input.candidate_positions = vec![0, 2];
        validate(&input, Readout::PointerHead, 3, 2)?;
        input.attention_mask[0] = 0;
        assert!(validate(&input, Readout::DecisionSymbol, 3, 2).is_err());
        Ok(())
    }
}
