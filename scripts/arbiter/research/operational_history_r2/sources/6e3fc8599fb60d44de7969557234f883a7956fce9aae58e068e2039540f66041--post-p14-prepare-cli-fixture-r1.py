# SPDX-License-Identifier: Apache-2.0
import hashlib,json,difflib
from pathlib import Path
B=Path(__file__).resolve().parent
W=B/'p10-worktree'
rel='crates/airk-cli/tests/combined_reads/deleted_source_recovery.rs'
original=(W/rel).read_bytes();s=(W/rel).read_text()
def replace(old,new):
 global s
 assert s.count(old)==1,old[:90]
 s=s.replace(old,new)
marker='async fn stop_deletion_service(f: &Fixture, service: &mut Option<OwnedProcess>) -> Result<()> {'
helper='''// Missing storage is a failed member under Report, not an RPC or whole-union
// failure. Explicit Require still refuses while retaining that same complete A
// outcome. Exercise all three actual read paths without replaying a mutation.
async fn missing_storage_reads(f: &Fixture, mcp: &mut McpSession, first_id: u64) -> Result<()> {
    for (offset, required) in [false, true].into_iter().enumerate() {
        let mut args = deletion_query();
        args["project_ids"] = json!(f.session.project_ids);
        args["member_availability"] = json!(if required { "require" } else { "report" });
        let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
        let native = native_read(backend.as_ref(), "airk_search", &args).await;
        backend.close();
        backend.shutdown().await?;
        let native = native?;
        let cli = f.cli("airk_search", &args).await?;
        let wire = mcp.call(first_id + offset as u64, "airk_search", args)?;
        ensure!(wire.get("error").is_none() && wire["result"].is_object(),
            "missing storage produced an RPC failure instead of a member result: {wire}");
        policy_parity("missing storage native/CLI", &native, &cli)?;
        policy_parity("missing storage native/MCP", &native, &wire["result"])?;
        for reply in [&native, &cli, &wire["result"]] {
            ensure!(reply.get("isError").is_none_or(Value::is_boolean)
                && (reply["isError"] == true) == required,
                "missing storage Report/Require outer flag differs: {reply}");
        }
        let value = body(&native)?;
        ensure!(value["schema"] == "airk.combined-read/1" && value["partial"] == true
            && value["ranking"].as_array().is_some_and(Vec::is_empty),
            "missing storage invented complete or ranked evidence: {value}");
        let members = value["members"].as_array().context("missing storage member inventory absent")?;
        ensure!(members.len() == 1 && members[0]["index_id"] == json!(f.indexes[2])
            && members[0]["outcome"] == "failed" && members[0]["is_error"] == true,
            "missing storage omitted or changed selected A: {value}");
        let child = member_body(&value, &members[0])?;
        ensure!(child["error"]["code"] == "scoped_connection_unavailable"
            && child["error"]["query_submitted"] == false && child["error"]["retryable"] == false,
            "missing storage failed for another reason: {child}");
        let publications = value["publication_vector"]["members"].as_array()
            .context("missing storage publication inventory absent")?;
        ensure!(publications.len() == 1 && publications[0]["index_id"] == json!(f.indexes[2])
            && publications[0]["publication"].is_null(),
            "missing storage invented an A publication: {value}");
        ensure!(if required { value["error"]["code"] == "member_unavailable" }
            else { value["error"].is_null() },
            "explicit member availability contract changed: {value}");
    }
    Ok(())
}

'''
replace(marker,helper+marker)
replace('''        let reply = unavailable.request(
            50,
            "tools/call",
            json!({"name":"airk_search","arguments":deletion_query()}),
        )?;
        ensure!(
            reply.get("error").is_some() || reply["result"]["isError"] == true,
            "missing-storage read unexpectedly produced evidence: {reply}"
        );''','''        missing_storage_reads(&f, &mut unavailable, 50).await?;''')
replace('''    fixture_job(&f, 2, airk_core::service::JobKind::Index).await?;
    f.session = affected_session.clone();''','''    // This shared fixture deliberately starts paused. Ordinary Index work must
    // be explicitly admitted, then drained before restoring that user choice.
    fixture_pause(&f, 2, false).await?;
    fixture_job(&f, 2, airk_core::service::JobKind::Index).await?;
    super::super::split::pause_quiescent_original(&f, 2).await?;
    f.session = affected_session.clone();''')
replace('''        "real native terminal WAL children missing; no fabricated positive fixture"
    );''','''        "real native terminal WAL children missing; selected={}, original={}; complete bounded actual funding={}",
        selected.len(), original.len(), {
            airk_core::index_catalog::encoded_size(&before, airk_core::MAX_RESPONSE_BYTES)?;
            serde_json::to_string(&before)?
        }
    );''')
replace('''        "max_bytes":16384,"response_mode":"full","verification":mode,
        "verification_max_sources":8,"verification_max_bytes":32768,
        "verification_deadline_ms":4000});''','''        "max_bytes":16384,"response_mode":"full","verification":mode});
    // Off forbids all verification budgets in the canonical native catalog.
    // Report/Require retain the originally bounded verification workload.
    if mode != "off" {
        args["verification_max_sources"] = json!(8);
        args["verification_max_bytes"] = json!(32768);
        args["verification_deadline_ms"] = json!(4000);
    }''')
replace('''        .context("union outcomes absent")?;''','''        .with_context(|| format!("union members absent; complete bounded native body: {value}"))?;''')
new=s.replace('\n','\r\n').encode() if b'\r\n' in original else s.encode()
out=B/'post-p14-cli-fixture-proposed-r1';out.mkdir(exist_ok=False)
dest=out/'source'/rel;dest.parent.mkdir(parents=True);dest.write_bytes(new)
patch=''.join(difflib.unified_diff(original.decode().splitlines(True),new.decode().splitlines(True),fromfile='a/'+rel,tofile='b/'+rel))
(out/'fix.patch').write_bytes(patch.encode())
m={'status':'INERT definite CLI fixture/contract corrections plus bounded funding diagnostic; no production changes or execution','files':[{'path':rel,'base_sha256':hashlib.sha256(original).hexdigest(),'proposed_sha256':hashlib.sha256(new).hexdigest()}],'patch_sha256':hashlib.sha256((out/'fix.patch').read_bytes()).hexdigest(),'causes':['Union fields already use members. Invalid verification=off with explicit verification budgets is refused by canonical VerificationControls.resolve before any member read; condition budgets on Report/Require.','Missing storage has actual modern outer-success Report + failed member/nullpublication/partial. Require availability retains same outcome and refuses. New helper compares complete Native/CLI/MCP results, exact failure code/no query submission and both availability modes.','Fixture initializes all projects paused=true. New individual-file test queued ordinaryIndex without explicit unpause, yielding Pending/attempts0. Reuse actual Pause and existing quiescent drain to restore Pause after real job.','Opaque terminalWAL assertion combines several branches; preserve positive requirement and dump complete bounded actualFundingReceipt plus selected/original counts on next necessary coalesced run. Actual checkpoint cause/unavailable participant not yet observed; no speculative reader campaign.'],'preserved':['Frozen defaultProject union/grant/index_idsNone','Shared native/CLI/MCP complete parity and scopes/revisions/budgets','No action replay, generic errors, increased timeouts, fabricated credit, fake files or weakened WAL assertion'],'validation':'No Cargo/native/SSH/tests executed; root formatting/compile/focused37 and independent review required'}
(out/'manifest.json').write_text(json.dumps(m,indent=2)+'\n')
print(json.dumps({'patch_sha256':m['patch_sha256'],'manifest_sha256':hashlib.sha256((out/'manifest.json').read_bytes()).hexdigest()}))
