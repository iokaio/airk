// SPDX-License-Identifier: Apache-2.0
//! Per-read projection, independent of retention, metadata encoding and admission.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplyDetail {
    Lean,
    Rich,
}
impl ReplyDetail {
    pub const ALL: [Self; 2] = [Self::Lean, Self::Rich];
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lean => "lean",
            Self::Rich => "rich",
        }
    }
    /// None preserves the caller's exact legacy default, including its encoding.
    pub fn compact(choice: Option<Self>, legacy: Option<bool>) -> Result<Option<bool>> {
        resolve(choice.map(|v| v == Self::Lean), legacy, "compact")
    }
    /// Record explicit choices only; absent controls cannot grow legacy replies.
    pub fn annotate(value: &mut Value, choice: Option<Self>) {
        if let Some(choice) = choice
            && let Some(fields) = value.as_object_mut()
        {
            fields.insert("reply_detail".into(), Value::String(choice.as_str().into()));
        }
    }
}
fn resolve(selected: Option<bool>, legacy: Option<bool>, flag: &str) -> Result<Option<bool>> {
    ensure!(
        selected.zip(legacy).is_none_or(|(a, b)| a == b),
        "reply_detail conflicts with {flag}"
    );
    Ok(selected.or(legacy))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_projection_and_compatibility_flags_agree_without_choosing_a_default() -> Result<()>
    {
        assert_eq!(ReplyDetail::compact(None, None)?, None);
        for legacy in [false, true] {
            assert_eq!(ReplyDetail::compact(None, Some(legacy))?, Some(legacy));
        }
        for choice in ReplyDetail::ALL {
            let compact = choice == ReplyDetail::Lean;
            assert_eq!(ReplyDetail::compact(Some(choice), None)?, Some(compact));
            assert_eq!(
                ReplyDetail::compact(Some(choice), Some(compact))?,
                Some(compact)
            );
            assert!(ReplyDetail::compact(Some(choice), Some(!compact)).is_err());
            assert_eq!(
                serde_json::to_value(choice)?,
                Value::String(choice.as_str().into())
            );
        }
        assert!(serde_json::from_value::<ReplyDetail>(Value::String("detailed".into())).is_err());
        let mut value = serde_json::json!({"hits":[]});
        let before = value.clone();
        ReplyDetail::annotate(&mut value, None);
        assert_eq!(value, before);
        Ok(())
    }
}
