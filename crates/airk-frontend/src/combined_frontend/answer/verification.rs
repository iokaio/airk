// SPDX-License-Identifier: Apache-2.0
//! One final saved-source verification budget for the whole Answer projection.
use super::super::*;
use airk_core::verification::{VerificationBudget, VerificationControls, VerificationMode};

pub(super) struct AnswerVerification {
    controls: VerificationControls,
    budget: Option<VerificationBudget>,
    maximum: usize,
}
impl AnswerVerification {
    pub(super) fn parse(arguments: &Map<String, Value>) -> Result<Self> {
        let parsed: airk_mcp::AnswerArgs =
            serde_json::from_value(Value::Object(arguments.clone()))?;
        parsed.request.validate()?;
        let controls = parsed.verification.controls();
        let budget = controls.resolve()?;
        Ok(Self {
            controls,
            budget,
            maximum: parsed
                .request
                .budget
                .max_bytes
                .min(super::ANSWER_REPLY_BYTES),
        })
    }
    pub(super) fn child_arguments(&self, arguments: &mut Map<String, Value>) {
        // The admitted whole owner performs the only saved-source check.
        for name in serde_json::to_value(VerificationControls::resolved(self.budget))
            .expect("native verification controls serialize")
            .as_object()
            .expect("native verification controls object")
            .keys()
        {
            arguments.remove(name);
        }
        arguments.insert("verification".into(), json!("off"));
    }
    pub(super) fn reserve_delivery_bytes(&mut self, bytes: usize) -> Result<()> {
        self.maximum = self
            .maximum
            .checked_sub(bytes)
            .context("Answer delivery metadata exceeds original byte budget")?;
        Ok(())
    }
    pub(super) fn minimum_attachment_bytes(&self) -> Result<usize> {
        if self.budget.is_none() {
            return Ok(0);
        }
        // Existing native VerifyProjection minimum. Larger actual receipts use
        // the exact residual cap; this does not promise 1 KiB covers all rows.
        let mut bytes = 1024usize + serde_json::to_vec("member_verification")?.len() + 2;
        if self
            .budget
            .is_some_and(|budget| budget.mode == VerificationMode::Require)
        {
            bytes = bytes
                .checked_add(self.required_failure_bytes()?)
                .context("verification metadata size overflow")?;
        }
        Ok(bytes)
    }
    fn required_failure_bytes(&self) -> Result<usize> {
        if self
            .budget
            .is_some_and(|budget| budget.mode == VerificationMode::Require)
        {
            Ok(serde_json::to_vec(&json!({"verification_failure":{"code":"verification_required_unmet","retryable":false}}))?.len()-1)
        } else {
            Ok(0)
        }
    }
    fn receipt_capacity(&self, body: &Value) -> Result<usize> {
        ensure!(
            body.is_object() && body.get("member_verification").is_none(),
            "verification would replace native data"
        );
        let bytes = serde_json::to_vec(body)?.len();
        let required_failure = self.required_failure_bytes()?;
        // Adding one object member: comma + encoded key + colon + receipt.
        let overhead = serde_json::to_vec("member_verification")?
            .len()
            .checked_add(2)
            .and_then(|bytes| bytes.checked_add(required_failure))
            .context("verification attachment size overflow")?;
        Ok(self
            .maximum
            .saturating_sub(bytes.saturating_add(overhead))
            .min(airk_core::verification::MAX_RECEIPT_BYTES))
    }
    fn arguments(
        &self,
        body: Value,
        remaining: u64,
        capacity: usize,
    ) -> Result<Map<String, Value>> {
        let mut args = serde_json::to_value(self.controls)?
            .as_object()
            .context("verification controls")?
            .clone();
        args.insert("projection".into(), body);
        args.insert("remaining_deadline_ms".into(), json!(remaining));
        args.insert("receipt_bytes_budget".into(), json!(capacity));
        Ok(args)
    }
    fn attach(&self, mut original: Value, proof: Value) -> Result<Value> {
        let mut body = canonical_body(&original);
        ensure!(
            body.get("member_verification").is_none(),
            "verification would replace native data"
        );
        let failed = result_failed(&proof);
        body["member_verification"] = proof;
        let required = self
            .budget
            .is_some_and(|budget| budget.mode == VerificationMode::Require);
        if failed && required {
            body["verification_failure"] =
                json!({"code":"verification_required_unmet","retryable":false});
        }
        airk_core::verification::validate_projection(&body)?;
        airk_core::index_catalog::encoded_size(&body, self.maximum)?;
        if original.get("content").is_some() {
            ensure!(
                original
                    .pointer("/content/0/text")
                    .and_then(Value::as_str)
                    .is_some(),
                "native Answer text missing"
            );
            original["content"][0]["text"] = json!(serde_json::to_string(&body)?);
            if original.get("structuredContent").is_some() {
                original["structuredContent"] = body;
            }
            if failed && required {
                original["isError"] = json!(true);
            }
            Ok(original)
        } else {
            Ok(body)
        }
    }
    pub(super) async fn apply<S: Session>(
        &self,
        backend: &Backend<S>,
        result: Value,
        selected: &QuerySelection,
        before: Snapshot<S::Authority, S::Child>,
        context: airk_mcp::read_attempt::ReadAttemptContext,
    ) -> Result<Value> {
        if self.budget.is_none() {
            return Ok(result);
        }
        let capacity = self.receipt_capacity(&canonical_body(&result))?;
        let proof = if capacity < 1024 {
            failure(
                "verification_receipt_budget_exhausted",
                Some(false),
                &backend.selection.requested(),
            )
        } else {
            let remaining = u64::try_from(
                context
                    .deadline()
                    .saturating_duration_since(Instant::now())
                    .as_millis(),
            )?;
            let args = self.arguments(canonical_body(&result), remaining, capacity)?;
            match backend
                .verify_projection(args, selected, before, context)
                .await
            {
                Ok(proof) => proof,
                Err(_) => failure(
                    "member_verification_observation_unavailable",
                    None,
                    &backend.selection.requested(),
                ),
            }
        };
        self.attach(result, proof)
    }
}

#[cfg(test)]
mod tests;
