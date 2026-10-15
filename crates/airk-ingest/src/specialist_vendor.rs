// SPDX-License-Identifier: Apache-2.0
//! Local vendor-language evidence. No vendor service, compiler or execution.
mod abap;
mod apex;
mod sql;
#[cfg(test)]
mod tests;
use crate::{build_facts::Facts, syntax::Analysis};
use airk_core::{EvidenceBasis, location::HostBudget};
use anyhow::{Result, ensure};

pub(crate) fn analyze(path: &str, source: &str, adapter: &str) -> Result<Analysis> {
    analyze_budgeted(
        path,
        source,
        adapter,
        &mut HostBudget::new(500_000, 10_000, 16_777_216)?,
    )
}

pub(crate) fn analyze_budgeted(
    path: &str,
    source: &str,
    adapter: &str,
    budget: &mut HostBudget,
) -> Result<Analysis> {
    ensure!(
        source.len() <= crate::MAX_FILE_BYTES,
        "vendor source byte limit"
    );
    budget.charge(0, 0, source.len(), 0)?;
    match adapter {
        "apex" => apex::analyze(path, source, budget),
        "abap" => abap::analyze(path, source, budget),
        "plsql" | "tsql" => sql::analyze(path, source, adapter, budget),
        _ => anyhow::bail!("unsupported vendor adapter"),
    }
}

pub(crate) use apex::qualifies_class;

fn finish(mut facts: Facts<'_>, partial: bool, adapter: &str) -> Result<Analysis> {
    for fact in &mut facts.facts {
        let code = fact.code.as_mut().expect("vendor evidence");
        if code.basis == EvidenceBasis::StructuralRule {
            code.producer = format!("airk-vendor-structure/{adapter}");
            code.producer_version = "vendor-facts/1".into();
            code.coverage = "Selected bounded written statements; vendor semantics, dynamic execution, metadata and references are unresolved; native continuity unavailable".into();
        }
    }
    ensure!(
        serde_json::to_vec(&facts.facts)?.len() <= 16_777_216,
        "vendor evidence byte limit"
    );
    let mut output = facts.finish(partial);
    if !partial {
        output.status = "structural-vendor".into();
    }
    Ok(output)
}
