# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib,hashlib,json
base=Path(__file__).resolve().parent
relative='crates/airk-cli/tests/combined_reads/split_metrics.rs'
live=base/'p10-worktree'/relative
original=live.read_text(encoding='utf-8')
start=original.index('            let mut cursor = None;\n            let mut assembled = Value::Null;')
end=original.index('            if complete {',start)
replacement='''            use airk_core::native_reads::{NativeRead, ReportFormat};
            // Preserve the actual transport observation first. The scoped RPC
            // can refuse an administrative usage read; that is unknown CPU,
            // never a Page or a successful transport observation.
            let response = airk_cli::service_runtime::request(&f.control, ControlMethod::NativeRead {
                project:snapshot.request.destination_registration.binding,
                usage_client:airk_core::usage::ClientFamily::Unknown,
                request:NativeRead::UsageReport {days:1,cursor:None,max_bytes:16384,
                    format:ReportFormat::Json,price:None}
            }).await?;
            let encoded = serde_json::to_vec(&response)?;
            ensure!(encoded.len() <= 65536, "usage RPC observation exceeds frozen 64KiB reply bound");
            let digest = airk_core::identity::framed_digest(b"airk/p10-metric-usage-rpc/1", &[&encoded]);
            self.event(json!({"phase":"settled","usage_rpc_reply":response,
                "reply_bytes":encoded.len(),"reply_digest":digest,
                "cpu_coverage":"RPC observation alone grants no CPU counter; canonical error means unknown"}))?;
            let ControlResponse::NativeRead { result } = response else {
                anyhow::bail!("unexpected native usage transport response retained at {digest}");
            };
            if result.get("error").is_some() {
                ensure!(canonical_usage_error(&result),
                    "unknown/malformed native usage error retained at {digest}");
            } else {
                // Arbitrary or malformed page records are actual failures, not
                // excuses to silently downgrade counter coverage to unknown.
                let page:airk_kernel::usage_page::Page = serde_json::from_value(result)?;
                let mut checked=Value::Null;
                for record in page.items {
                    airk_kernel::usage_page::assemble(&mut checked,record)?;
                }
            }
            let kernel = airk_kernel::Kernel::open_existing_service_observer(
                &snapshot.request.destination_registration)?;
            let mut tools=airk_mcp::local::LocalTools::new(kernel).await?;
            let observed:Result<(Value,bool)>=async {
                let mut cursor=None;
                let mut assembled=Value::Null;
                let mut complete=false;
                for _ in 0..4 {
                    let result=tools.read_native(NativeRead::UsageReport {
                        days:1,cursor,max_bytes:16384,format:ReportFormat::Json,price:None
                    },snapshot.request.destination_registration.binding).await?;
                    let encoded=serde_json::to_vec(&result)?;
                    ensure!(encoded.len() <= 16384,"canonical local usage page exceeds requested bound");
                    self.event(json!({"phase":"settled","usage_local_native_page":result,
                        "observation_basis":"trusted owned-fixture existing Kernel observer + LocalTools canonical NativeRead; not service RPC transport",
                        "reply_bytes":encoded.len(),"reply_digest":airk_core::identity::framed_digest(b"airk/p10-metric-usage-local/1", &[&encoded])}))?;
                    let page:airk_kernel::usage_page::Page=serde_json::from_value(result)?;
                    for record in page.items {
                        airk_kernel::usage_page::assemble(&mut assembled,record)?;
                    }
                    cursor=page.cursor;
                    if cursor.is_none() {complete=true;break;}
                }
                Ok((assembled,complete))
            }.await;
            // Attempt shutdown even if decoding or canonical page assembly fails.
            let closed=tools.close().await;
            let (assembled,complete)=observed?;
            closed?;
'''
s=original[:start]+replacement+original[end:]
needle='"cpu_coverage":"exclusive native Index spans only; not original removal, cleanup, actor or helper total"'
assert s.count(needle)==1
s=s.replace(needle,'"cpu_coverage":"trusted owned-fixture local canonical NativeRead; exclusive native Index spans only; not RPC transport, original removal, cleanup, actor or helper total"')
helper='''
fn canonical_usage_error(value:&Value)->bool {
    // Exact canonical scoped refusal, or the worker's bounded native failure
    // envelope. Unknown error codes/extra fields remain validation failures.
    if *value == json!({"error":airk_core::scoped_reads::ScopeReadRefused.receipt()}) {
        return true;
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope { error:Failure }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Failure { code:String,message:String }
    serde_json::from_value::<Envelope>(value.clone()).is_ok_and(|envelope|
        envelope.error.code == "native_read_failed" && envelope.error.message.chars().count() <= 1024)
}
'''
s=s.replace('impl Capture {',helper+'impl Capture {',1)
(base/'p10-resource-cpu-observer.proposed.rs').write_text(s,encoding='utf-8',newline='\n')
print(json.dumps({'base':hashlib.sha256(live.read_bytes()).hexdigest()}))
