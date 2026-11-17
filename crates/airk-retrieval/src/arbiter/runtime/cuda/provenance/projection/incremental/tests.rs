// SPDX-License-Identifier: Apache-2.0
//! Pure literal refusal controls. No filesystem, process, DLL or model work.
use super::*;
use serde_json::json;

fn h(n: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([n; 32])
}
fn p(name: &str, n: u8) -> Pin {
    Pin {
        path: std::env::temp_dir()
            .join(name)
            .to_string_lossy()
            .into_owned(),
        bytes: 7,
        sha256: h(n),
    }
}
fn clock_fixture() -> Value {
    json!({"started_monotonic":100.0,"finished_monotonic":101.0,"original_deadline_monotonic":700.0,
        "elapsed_seconds":1.0,"started_utc":"2026-11-17T20:00:00+00:00",
        "finished_utc":"2026-11-17T20:00:01+00:00","original_deadline":"2026-11-17T20:10:00+00:00"})
}
fn process_fixture() -> (Value, Vec<Value>) {
    let tool = p("incremental-synthetic-cl.exe", 1);
    let exits = json!([{"pid":41,"exit_observed":true,"exit_code":0}]);
    let close = json!({"forced":false,"settled":true,"final_active":0,"members_before":[],
        "final_member_pids":[],"retained_exits":exits});
    let auth = json!({"pid":41,"execution_user_sid":SID,"default_owner_sid":SID,
        "image":{"path":tool.path,"sha256":tool.sha256},
        "component_job":{"process_bytes":17179869184u64,"job_bytes":25769803776u64,"flags":8968,"active_limit":2}});
    let result = json!({"exit_code":0,"timed_out":false,"log_oversized":false,
        "owned_descendants_settled":true,"owned_job_forced_termination":false,
        "owned_job_final_active_processes":0,"owned_job_final_member_pids":[],
        "parent_assigned_to_job":false,"child_assigned_before_resume":true,
        "unexpected_third_process":false,"whole_process_deadline_not_refreshed":true,
        "owned_leader_pid":41,"owned_total_processes":1,"peak_host_committed_bytes":4096,
        "retained_member_exits":exits,"handle_close_proof":close,"suspended_assignment":auth});
    (
        json!({"argv":[tool.path],"owned":result}),
        vec![serde_json::to_value(tool).unwrap()],
    )
}
#[test]
fn original_clock_accepts_exact_bound_and_refuses_renewal_expiry_or_missing() -> Result<()> {
    let v = clock_fixture();
    clocks(&v)?;
    for (key, value) in [
        ("original_deadline_monotonic", json!(701.0)),
        ("finished_monotonic", json!(700.0)),
        ("elapsed_seconds", json!(2.0)),
        ("original_deadline", json!("2026-11-17T20:10:01+00:00")),
        ("finished_utc", json!("2026-11-17T20:10:01+00:00")),
    ] {
        let mut changed = v.clone();
        changed[key] = value;
        assert!(clocks(&changed).is_err(), "{key}");
    }
    for key in [
        "started_monotonic",
        "finished_monotonic",
        "original_deadline",
        "elapsed_seconds",
    ] {
        let mut changed = v.clone();
        changed.as_object_mut().unwrap().remove(key);
        assert!(clocks(&changed).is_err(), "{key}");
    }
    Ok(())
}
#[test]
fn retained_handle_settlement_needs_actual_leader_exit() -> Result<()> {
    let (v, tools) = process_fixture();
    owned(&v, &tools)?;
    for key in [
        "retained_member_exits",
        "handle_close_proof",
        "suspended_assignment",
    ] {
        let mut changed = v.clone();
        changed["owned"].as_object_mut().unwrap().remove(key);
        assert!(owned(&changed, &tools).is_err(), "{key}");
    }
    let mut changed = v.clone();
    changed["owned"]["handle_close_proof"]["retained_exits"][0]["pid"] = json!(42);
    assert!(owned(&changed, &tools).is_err());
    let mut changed = v;
    changed["owned"]["retained_member_exits"][0]["exit_code"] = json!(101);
    assert!(owned(&changed, &tools).is_err());
    Ok(())
}
#[test]
fn suspended_image_sid_default_owner_and_original_job_are_independent_refusals() -> Result<()> {
    let (v, tools) = process_fixture();
    owned(&v, &tools)?;
    for key in ["execution_user_sid", "default_owner_sid"] {
        let mut changed = v.clone();
        changed["owned"]["suspended_assignment"][key] = json!("foreign");
        assert!(owned(&changed, &tools).is_err(), "{key}");
    }
    let mut changed = v.clone();
    changed["owned"]["suspended_assignment"]["image"]["sha256"] = json!(h(2));
    assert!(owned(&changed, &tools).is_err());
    let mut changed = v.clone();
    changed["owned"]["suspended_assignment"]["component_job"]["flags"] = json!(0);
    assert!(owned(&changed, &tools).is_err());
    let mut changed = v;
    changed["owned"]["suspended_assignment"]["pid"] = json!(42);
    assert!(owned(&changed, &tools).is_err());
    Ok(())
}
#[test]
fn process_force_foreign_member_and_missing_deadline_proof_refuse() -> Result<()> {
    let (v, tools) = process_fixture();
    for key in [
        "timed_out",
        "log_oversized",
        "owned_job_forced_termination",
        "parent_assigned_to_job",
        "unexpected_third_process",
    ] {
        let mut changed = v.clone();
        changed["owned"][key] = json!(true);
        assert!(owned(&changed, &tools).is_err(), "{key}");
    }
    for key in [
        "owned_descendants_settled",
        "child_assigned_before_resume",
        "whole_process_deadline_not_refreshed",
    ] {
        let mut changed = v.clone();
        changed["owned"][key] = json!(false);
        assert!(owned(&changed, &tools).is_err(), "{key}");
    }
    let mut changed = v;
    changed["owned"]["owned_job_final_member_pids"] = json!([41]);
    assert!(owned(&changed, &tools).is_err());
    Ok(())
}
#[test]
fn forty_session_inputs_allow_exactly_one_replacement() -> Result<()> {
    let before: Vec<_> = (0..40)
        .map(|n| serde_json::to_value(p(&format!("object-{n}.obj"), n + 1)).unwrap())
        .collect();
    let old = pin(&before[17])?;
    let fresh = p("fresh-object.obj", 99);
    let mut after = before.clone();
    after[17] = serde_json::to_value(&fresh)?;
    replaced(&before, &after, &old, &fresh, 40)?;
    let mut changed = after.clone();
    changed[5]["sha256"] = json!(h(100));
    assert!(replaced(&before, &changed, &old, &fresh, 40).is_err());
    assert!(replaced(&before, &before, &old, &fresh, 40).is_err());
    after.push(after[0].clone());
    assert!(replaced(&before, &after, &old, &fresh, 40).is_err());
    Ok(())
}
#[test]
fn ninety_six_libraries_preserve_every_other_pin_and_order() -> Result<()> {
    let before: Vec<_> = (0..96)
        .map(|n| serde_json::to_value(p(&format!("library-{n}.lib"), n + 1)).unwrap())
        .collect();
    let old = pin(&before[95])?;
    let fresh = p("fresh-session.lib", 200);
    let mut after = before.clone();
    after[95] = serde_json::to_value(&fresh)?;
    replaced(&before, &after, &old, &fresh, 96)?;
    after.swap(0, 1);
    assert!(replaced(&before, &after, &old, &fresh, 96).is_err());
    Ok(())
}
fn archive() -> Value {
    json!({"kind":"static","members":2,"static_members":2,"import_members":0,"imported_dlls":[],
        "object_bodies":[{"sha256":h(1),"bytes":7,"count":1},{"sha256":h(2),"bytes":11,"count":1}]})
}
#[test]
fn coff_closure_preserves_body_keys_and_only_existing_dedup_semantics() -> Result<()> {
    let expected = BTreeMap::from([((h(1), 7), 3), ((h(2), 11), 1)]);
    body_closure(&archive(), &expected)?;
    let mut changed = archive();
    changed["object_bodies"][0]["sha256"] = json!(h(3));
    assert!(body_closure(&changed, &expected).is_err());
    let mut changed = archive();
    changed["object_bodies"][0]["count"] = json!(4);
    changed["members"] = json!(5);
    changed["static_members"] = json!(5);
    assert!(body_closure(&changed, &expected).is_err());
    Ok(())
}
#[test]
fn import_archive_duplicate_body_and_zero_member_refuse() {
    for (key, value) in [
        ("kind", json!("import_or_mixed")),
        ("import_members", json!(1)),
        ("members", json!(0)),
    ] {
        let mut v = archive();
        v[key] = value;
        assert!(bodies(&v).is_err(), "{key}");
    }
    let mut v = archive();
    v["object_bodies"][1] = v["object_bodies"][0].clone();
    assert!(bodies(&v).is_err());
}
#[test]
fn exact_private_roster_refuses_missing_and_unknown_fields() -> Result<()> {
    let v = json!({"schema":"literal","qualification":false});
    exact_keys(&v, &["schema", "qualification"])?;
    assert!(exact_keys(&v, &["schema"]).is_err());
    assert!(exact_keys(&v, &["schema", "qualification", "extra"]).is_err());
    Ok(())
}
#[test]
fn pins_refuse_missing_size_zero_hash_and_unknown_authority() {
    let v = serde_json::to_value(p("literal.bin", 1)).unwrap();
    let mut changed = v.clone();
    changed["bytes"] = json!(0);
    assert!(pin(&changed).is_err());
    let mut changed = v.clone();
    changed["sha256"] = json!(h(0));
    assert!(pin(&changed).is_err());
    let mut changed = v;
    changed["approved"] = json!(true);
    assert!(pin(&changed).is_err());
}
