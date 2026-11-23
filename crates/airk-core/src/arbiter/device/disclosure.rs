// SPDX-License-Identifier: Apache-2.0
//! Stage and variant reasons written from typed selection facts. Descriptive
//! text only: it names the route, the model, the threads or adapter route, the
//! token budget and the cause of any fallback. It states nothing about weight
//! precision and grants no execution, source or qualification authority.
use super::*;

/// Longest reason a stage, variant or index application may carry.
pub const MAX_DISCLOSURE_BYTES: usize = 1024;

/// What the native owner observed about one catalog variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariantState {
    /// The variant is the resident selection.
    Active,
    /// The GPU session is resident and its local eligibility is not current.
    ResidentNotEligible,
    /// The variant is admitted and is not the resident selection.
    Admitted,
    /// The variant has no current native admission.
    NotAdmitted,
}

fn model_name(model: SelectedModel) -> &'static str {
    match model {
        SelectedModel::Focus => "Focus",
        SelectedModel::Arbiter => "Arbiter",
    }
}
fn route_name(backend: GpuBackend) -> &'static str {
    match backend {
        GpuBackend::Cuda => "CUDA",
        GpuBackend::DirectMl => "DirectML",
        GpuBackend::CoreMl => "Core ML",
    }
}
/// The wire name of a reason, so the sentence and the typed row agree.
fn reason_name(reason: SelectionReason) -> String {
    serde_json::to_value(reason)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into())
}
/// Decimal digits in groups of three: 8192 is written 8,192.
fn grouped(value: usize) -> String {
    let digits = value.to_string();
    let mut text = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            text.push(',');
        }
        text.push(digit);
    }
    text
}
fn threads(count: usize) -> String {
    if count == 1 {
        "1 thread".into()
    } else {
        format!("{} threads", grouped(count))
    }
}
/// "CPU with 8 threads" or "GPU through CUDA", from the profile's typed fields.
fn route(profile: &Profile) -> String {
    match &profile.device_execution {
        Some(execution) => format!("GPU through {}", route_name(execution.backend)),
        None => format!(
            "CPU with {}",
            threads(profile.cpu_fit.clone().unwrap_or_default().intra_threads)
        ),
    }
}
fn budget(profile: &Profile) -> String {
    format!("the {}-token budget", grouped(profile.max_tokens))
}
/// One candidate that was considered and not selected.
fn unused(profile: Option<&Profile>, reason: SelectionReason) -> String {
    let reason = reason_name(reason);
    match profile {
        Some(profile) => match &profile.device_execution {
            Some(execution) => format!(
                "The {} route at {} was not used: {reason}.",
                route_name(execution.backend),
                budget(profile)
            ),
            None => format!(
                "The {}-token CPU tier with {} was not used: {reason}.",
                grouped(profile.max_tokens),
                threads(profile.cpu_fit.clone().unwrap_or_default().intra_threads)
            ),
        },
        None => format!("A candidate was not used: {reason}."),
    }
}
fn global(reason: SelectionReason) -> String {
    match reason {
        SelectionReason::ForcedCpu => "The device policy is CPU.".into(),
        SelectionReason::GpuNotOptedIn => "GPU use is not turned on in service resources.".into(),
        SelectionReason::NoGpu => "No GPU adapter was observed.".into(),
        SelectionReason::NoQualifiedGpuRoute => {
            "No qualified GPU route exists for this model.".into()
        }
        SelectionReason::NoQualifiedCpu => "No CPU tier is admitted for this model.".into(),
        other => format!("Cause: {}.", reason_name(other)),
    }
}
/// Joins sentences within the byte bound. Causes that do not fit are counted,
/// never cut in the middle.
fn bounded(head: String, causes: Vec<String>) -> String {
    let mut text = head;
    let total = causes.len();
    for (index, cause) in causes.into_iter().enumerate() {
        let rest = total - index - 1;
        let tail = if rest == 0 {
            0
        } else {
            // Room for the note below, were this the last cause that fits.
            " 00 more causes are in the selection rows.".len()
        };
        if text.len() + 1 + cause.len() + tail > MAX_DISCLOSURE_BYTES {
            let omitted = total - index;
            let note = if omitted == 1 {
                " 1 more cause is in the selection rows.".to_owned()
            } else {
                format!(" {omitted} more causes are in the selection rows.")
            };
            if text.len() + note.len() <= MAX_DISCLOSURE_BYTES {
                text.push_str(&note);
            }
            break;
        }
        text.push(' ');
        text.push_str(&cause);
    }
    text
}

/// The stage reason for a recorded selection: the selected route when there
/// is one, then the cause of each fallback in row order. `resolve` supplies the
/// immutable catalog description of a manifest; an unresolved manifest is named
/// without tier facts.
pub fn stage_disclosure<'a>(
    selection: &DeviceSelection,
    resolve: impl Fn(Sha256Digest) -> Option<&'a Profile>,
) -> String {
    let model = model_name(selection.model);
    let head = match (&selection.selected, selection.operation_failure) {
        (Some(selected), _) => match resolve(selected.manifest) {
            Some(profile) => format!(
                "{model} is active on {} at {}.",
                route(profile),
                budget(profile)
            ),
            None => match selected.backend {
                Some(backend) => {
                    format!("{model} is active on GPU through {}.", route_name(backend))
                }
                None => format!("{model} is active on CPU."),
            },
        },
        (None, Some(reason)) => format!(
            "{model} is not active. The operation stopped and nothing further was tried: {}.",
            reason_name(reason)
        ),
        (None, None) => format!("{model} is not active. No admitted route fits this system."),
    };
    let causes = selection
        .reasons
        .iter()
        .filter(|row| row.manifest.is_some() || Some(row.reason) != selection.operation_failure)
        .map(|row| match row.manifest {
            Some(manifest) => unused(resolve(manifest), row.reason),
            None => global(row.reason),
        })
        .collect();
    bounded(head, causes)
}

/// The reason of one catalog variant, from its immutable description, what the
/// native owner observed and the typed cause recorded for it, if any.
pub fn variant_disclosure(
    profile: &Profile,
    state: VariantState,
    cause: Option<SelectionReason>,
) -> String {
    let model = SelectedModel::from_model(profile.model)
        .map(model_name)
        .unwrap_or("This model");
    let subject = format!("{model} on {} at {}", route(profile), budget(profile));
    match state {
        VariantState::Active => format!("{subject} is active."),
        VariantState::ResidentNotEligible => format!(
            "{subject} remains resident, but its local eligibility is not current. Recover can reaccept it under a bounded management operation."
        ),
        VariantState::Admitted => match cause {
            Some(reason) => format!(
                "{subject} is admitted and not active. It was not used: {}.",
                reason_name(reason)
            ),
            None => format!("{subject} is admitted and not active."),
        },
        VariantState::NotAdmitted => format!("{subject} has no current native admission."),
    }
}

#[cfg(test)]
mod tests;
