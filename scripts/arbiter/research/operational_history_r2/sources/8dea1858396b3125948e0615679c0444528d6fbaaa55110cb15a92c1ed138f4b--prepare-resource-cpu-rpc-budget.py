# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json
base=Path(__file__).resolve().parent
live=base/'p10-worktree/crates/airk-cli/tests/combined_reads/split_metrics.rs'
s=live.read_text(encoding='utf-8')
start=s.index('fn canonical_usage_error(')
end=s.index('impl Capture {',start)
s=s[:start]+s[end:]
start=s.index('            use airk_core::native_reads::{NativeRead, ReportFormat};')
end=s.index('            if complete {',start)
replacement='''            use airk_core::native_reads::{NativeRead, ReportFormat};
            let mut cursor = None;
            let mut assembled = Value::Null;
            let mut complete = false;
            for _ in 0..4 {
                let started = Instant::now();
                let response = airk_cli::service_runtime::request(&f.control, ControlMethod::NativeRead {
                    project:snapshot.request.destination_registration.binding,
                    usage_client:airk_core::usage::ClientFamily::Unknown,
                    request:NativeRead::UsageReport {days:1,cursor,max_bytes:20 << 10,
                        format:ReportFormat::Json,price:None}
                }).await?;
                let encoded = serde_json::to_vec(&response)?;
                ensure!(encoded.len() <= 65536,"usage RPC observation exceeds frozen 64KiB reply bound");
                let digest=airk_core::identity::framed_digest(b"airk/p10-metric-usage-rpc/1", &[&encoded]);
                self.event(json!({"phase":"settled","usage_rpc_reply":response,
                    "reply_bytes":encoded.len(),"reply_digest":digest,"elapsed_us_separate_from_query":started.elapsed().as_micros(),
                    "observation_basis":"canonical service NativeRead UsageReport RPC; four pages, 20KiB per page"}))?;
                let ControlResponse::NativeRead { result } = response else {
                    anyhow::bail!("unexpected native usage RPC response retained at {digest}");
                };
                // The requested budget meets the canonical 20..48KiB contract.
                // Native errors and malformed pages/records remain failures,
                // retained above in full; none imply a missing/zero counter.
                ensure!(result.get("error").is_none(),"native usage RPC error retained at {digest}: {result}");
                let page:airk_kernel::usage_page::Page=serde_json::from_value(result)?;
                for record in page.items {
                    airk_kernel::usage_page::assemble(&mut assembled,record)?;
                }
                cursor=page.cursor;
                if cursor.is_none() {complete=true;break;}
            }
'''
s=s[:start]+replacement+s[end:]
s=s.replace('trusted owned-fixture local canonical NativeRead; exclusive native Index spans only; not RPC transport, original removal, cleanup, actor or helper total',
 'canonical service NativeRead UsageReport RPC; exclusive native Index spans only; not original removal, cleanup, actor or helper total')
(base/'p10-resource-cpu-rpc-budget.proposed.rs').write_text(s,encoding='utf-8',newline='\n')
print(json.dumps({'base':hashlib.sha256(live.read_bytes()).hexdigest()}))
