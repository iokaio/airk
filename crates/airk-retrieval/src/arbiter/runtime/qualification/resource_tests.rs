// SPDX-License-Identifier: Apache-2.0
use super::*;

fn compiler(focus: bool) -> CompilerProfile {
    CompilerProfile {
        schema: COMPILER_SCHEMA.into(),
        recipe: if focus {
            CompilerRecipe::QwenRerankYesNoV1
        } else {
            CompilerRecipe::QwenSingleChoiceV1
        },
        tokenizer: Sha256Digest::from_bytes([7; 32]),
        max_tokens: if focus { 512 } else { 2048 },
        max_options: if focus { 2 } else { 8 },
        controls: vec![],
        symbols: if focus {
            vec![
                BoundToken {
                    text: "no".into(),
                    id: 2152,
                },
                BoundToken {
                    text: "yes".into(),
                    id: 9693,
                },
            ]
        } else {
            (0..8)
                .map(|index| BoundToken {
                    text: char::from(b'A' + index).to_string(),
                    id: u32::from(index) + 1,
                })
                .collect()
        },
    }
}
#[test]
fn software_v2_context_serialization_changes_only_the_declared_native_width() -> Result<()> {
    let original = compiler(false);
    original.validate()?;
    let before = serde_json::to_vec_pretty(&original)?;
    let bytes = compiler_with_limits(&before, false, &ResourcePolicy::software_v2())?;
    let after: CompilerProfile = serde_json::from_slice(&bytes)?;
    assert_eq!(after.max_tokens, 8192);
    assert_eq!(original.max_tokens, 2048);
    let mut expected = serde_json::to_value(&original)?;
    expected["max_tokens"] = 8192.into();
    assert_eq!(serde_json::to_value(after)?, expected);
    assert_ne!(sha(&before), sha(&bytes));
    assert_eq!(
        serde_json::from_slice::<CompilerProfile>(&before)?.max_tokens,
        2048
    );
    Ok(())
}
#[test]
fn software_v2_focus_static512_compiler_bytes_remain_exact_and_wrong_limits_refuse() -> Result<()> {
    let original = compiler(true);
    original.validate()?;
    let before = serde_json::to_vec_pretty(&original)?;
    assert_eq!(
        compiler_with_limits(&before, true, &ResourcePolicy::software_v2())?,
        before
    );
    let mut wrong = original;
    wrong.max_tokens = 8192;
    assert!(
        compiler_with_limits(
            &serde_json::to_vec(&wrong)?,
            true,
            &ResourcePolicy::software_v2()
        )
        .is_err()
    );
    let mut policy = ResourcePolicy::software_v2();
    policy.context_input_tokens = 2048;
    assert!(compiler_with_limits(&before, true, &policy).is_err());
    assert!(compiler_with_limits(&before, false, &ResourcePolicy::software_v2()).is_err());
    Ok(())
}
