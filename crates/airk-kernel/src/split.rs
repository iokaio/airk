// SPDX-License-Identifier: Apache-2.0
//! Native split result qualification; this is not ordinary configuration execution.
use crate::Kernel;
use airk_core::catalog_split::{SplitJobPurpose, SplitJobResult};
use airk_store::service::registry::catalog::split::{ClaimedSplitWork, NativeSplitJobResult};
use anyhow::{Context, Result, ensure};
impl Kernel {
    pub fn qualify_split_job_result(
        &self,
        work: &ClaimedSplitWork,
        result: SplitJobResult,
    ) -> Result<NativeSplitJobResult> {
        self.require_unrestricted_read()?;
        if result.purpose == SplitJobPurpose::DestinationBuild
            && work.snapshot.request.require_semantic_ready
        {
            let status = self.status()?;
            ensure!(
                status.semantic_readiness.state == airk_core::readiness::SemanticState::Ready
                    && result.semantic_ready,
                "split destination semantic readiness is not complete"
            );
        }
        NativeSplitJobResult::from_current(work, &self.store, result)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SplitWitnessLimits {
    pub max_pages: usize,
    pub max_paths: usize,
    pub max_source_bytes: u64,
    pub max_millis: u64,
}
impl Default for SplitWitnessLimits {
    fn default() -> Self {
        Self {
            max_pages: 8,
            max_paths: 256,
            max_source_bytes: 16 << 20,
            max_millis: 5000,
        }
    }
}
impl Kernel {
    /// One positive remainder witness, with bounded retained discovery and full
    /// saved-source Require semantics. No whole snapshot/source tree materializes.
    pub fn qualify_split_remainder_witness(
        &self,
        state: &airk_store::service::registry::catalog::split::NativeSplitState,
        limits: SplitWitnessLimits,
    ) -> Result<airk_store::service::registry::catalog::split::NativeSplitRemainderWitness> {
        use airk_core::verification::SourceCheckState;
        use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
        self.require_unrestricted_read()?;
        ensure!(
            (1..=32).contains(&limits.max_pages)
                && (1..=1024).contains(&limits.max_paths)
                && (1..=64 << 20).contains(&limits.max_source_bytes)
                && (1..=30000).contains(&limits.max_millis),
            "split witness limits exceed supported bounds"
        );
        let snapshot = state.snapshot();
        snapshot.validate()?;
        let read = self.current_reader()?;
        let expected = read.token()?;
        // Frozen native source projection from this exact retained publication;
        // never consult a mutable configured root or change PolicyRuntime.
        let configuration = read
            .collection_state()?
            .context("split witness lacks retained configuration")?
            .configuration;
        let primary = read.head().scope.root.clone();
        let deadline = Instant::now() + Duration::from_millis(limits.max_millis);
        let mut cursor = None;
        let mut examined = 0usize;
        let mut charged = 0u64;
        let clock = || -> Result<u64> {
            Ok(SystemTime::now()
                .duration_since(UNIX_EPOCH)?
                .as_millis()
                .try_into()?)
        };
        for _ in 0..limits.max_pages {
            crate::service_execution::checkpoint()?;
            ensure!(
                Instant::now() < deadline && examined < limits.max_paths,
                "split remainder witness budget exhausted; nonempty state unknown"
            );
            let page = read.lookup_paths_filtered_checked(
                airk_store::current::postings::Lookup::Files,
                "",
                cursor.as_deref(),
                (limits.max_paths - examined).min(32),
                |path| {
                    let source =
                        airk_store::roots::source_location(&primary, &configuration.roots, path)?;
                    let root = snapshot
                        .baseline
                        .roots
                        .iter()
                        .find(|root| root.canonical_path == source.root)
                        .context("split witness retained source absent from frozen baseline")?;
                    Ok(snapshot
                        .plan
                        .remainder
                        .allows_canonical(root.id, &source.relative_path))
                },
                || {
                    crate::service_execution::checkpoint()?;
                    ensure!(
                        Instant::now() < deadline,
                        "split remainder witness budget exhausted; nonempty state unknown"
                    );
                    Ok(())
                },
            )?;
            ensure!(
                Instant::now() < deadline,
                "split remainder witness budget exhausted; nonempty state unknown"
            );
            let mut page_bytes = 0usize;
            let mut last_examined = cursor.clone();
            let mut byte_limited = false;
            for path in &page.paths {
                // Preserve the previous discovery page's 16KiB record budget.
                let record_bytes = serde_json::to_vec(path)?.len();
                ensure!(
                    record_bytes <= 16384,
                    "split witness path exceeds page byte budget; nonempty state unknown"
                );
                if record_bytes > 16384 - page_bytes {
                    byte_limited = true;
                    break;
                }
                page_bytes += record_bytes;
                last_examined = Some(path.clone());
                examined += 1;
                let info = self.current_file_info(&read, path)?;
                let Some(root) = snapshot
                    .baseline
                    .roots
                    .iter()
                    .find(|root| root.canonical_path == info.source.root)
                else {
                    continue;
                };
                if !snapshot
                    .plan
                    .remainder
                    .allows_canonical(root.id, &info.source.relative_path)
                {
                    continue;
                }
                self.require_source_policy_source(&info.source.root, &info.source.relative_path)?;
                if !self
                    .selected_source_policy(&info.source.root)?
                    .allows(&info.source.relative_path)?
                {
                    continue;
                }
                let remaining = limits.max_source_bytes.saturating_sub(charged);
                if info.source_bytes as u64 >= remaining {
                    continue;
                }
                let expected_hash = info.content_hash.parse()?;
                let start = clock()?;
                let check = airk_ingest::verify_saved_source(
                    std::path::Path::new(&info.source.root),
                    std::path::Path::new(&info.source.relative_path),
                    expected_hash,
                    remaining,
                    deadline,
                    &|| crate::service_execution::checkpoint().is_err(),
                );
                // The probe above only stops the read. An interrupted check is
                // not a mismatch: a drain or cancellation leaves this witness
                // with its own typed error instead of trying the next path or
                // ending in "no witness within bounds".
                crate::service_execution::checkpoint()?;
                charged = charged
                    .checked_add(check.bytes_read)
                    .ok_or_else(|| anyhow::anyhow!("split witness byte accounting overflow"))?;
                if check.state != SourceCheckState::Matched {
                    continue;
                }
                ensure!(
                    self.current_reader()?.token()? == expected,
                    "split witness publication changed during verification"
                );
                return airk_store::service::registry::catalog::split::NativeSplitRemainderWitness::from_current(state,&self.store,path,&check,start,clock()?);
            }
            cursor = if byte_limited {
                last_examined
            } else {
                page.next_after
            };
            if cursor.is_none() {
                break;
            }
        }
        anyhow::bail!(
            "no positively verified remainder witness within admitted bounds; nonempty state unknown"
        )
    }
}
