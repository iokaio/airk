// SPDX-License-Identifier: Apache-2.0
//! Atomic metadata retirement. Sources, settings and collection files stay untouched.
use airk_core::{
    identity::Sha256Digest,
    service::{JobOutcome, ProjectBinding},
    service_configuration::ProjectConfiguration,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, params};

/// Called inside fenced completion/recovery, in the same transaction as the
/// successful job receipt. A recorded preparation result alone is not success.
pub(super) fn finish_if_requested(
    connection: &Connection,
    project: ProjectBinding,
    job: Sha256Digest,
) -> Result<()> {
    let Some(work) = super::configuration::work(connection, job)? else {
        return Ok(());
    };
    if work.request.configuration != ProjectConfiguration::Unregister {
        return Ok(());
    }
    ensure!(
        super::configuration::result(connection, job)?
            == Some(serde_json::json!({"unregistration_requested":true})),
        "unregister preparation receipt is missing or changed"
    );
    ensure!(
        connection.execute(
            "UPDATE projects SET registered=0,paused=1 WHERE binding=?1 AND registered=1",
            [project.to_string()]
        )? == 1,
        "project is already unregistered"
    );
    let mut statement = connection.prepare("SELECT id FROM service_jobs WHERE project=?1 AND id<>?2 AND state IN('pending','running') ORDER BY seq LIMIT 1025")?;
    let canceled = statement
        .query_map(params![project.to_string(), job.to_string()], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        canceled.len() <= 1024,
        "unregister cancellation budget exceeded"
    );
    drop(statement);
    let outcome = serde_json::to_vec(&JobOutcome::Canceled)?;
    let now = super::maintenance::clock_ms()?;
    for id in canceled {
        connection.execute(
            "UPDATE service_jobs SET state='canceled',outcome=?1 WHERE id=?2",
            params![outcome, id],
        )?;
        super::maintenance::job_completed(connection, id.parse()?, now, None)?;
    }
    connection.execute(
        "UPDATE service_watch SET enabled=0,native_roots=0,backend='disabled' WHERE project=?1",
        [project.to_string()],
    )?;
    connection.execute(
        "DELETE FROM service_setup_pending WHERE project=?1",
        [project.to_string()],
    )?;
    connection.execute(
        "UPDATE service_background SET source_pending=0,semantic_pending=0 WHERE project=?1",
        [project.to_string()],
    )?;
    connection.execute(
        "UPDATE service_maintenance SET retries=7 WHERE project=?1",
        [project.to_string()],
    )?;
    let epoch: i64 = connection.query_row(
        "SELECT request_epoch FROM service_meta WHERE id=1",
        [],
        |row| row.get(0),
    )?;
    let next = epoch
        .checked_add(1)
        .context("request epoch exhausted during unregister")?;
    connection.execute(
        "UPDATE service_meta SET request_epoch=?1 WHERE id=1",
        [next],
    )?;
    super::bump(connection)?;
    super::configuration::write_result(
        connection,
        job,
        &serde_json::to_vec(&serde_json::json!({
            "unregistered":true,"sources_preserved":true,"settings_preserved":true,"derived_state_retained":true,
            "worker_drain":"observed separately"
        }))?,
    )?;
    Ok(())
}
