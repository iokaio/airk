// SPDX-License-Identifier: Apache-2.0
//! Transactional bounded event retention. No source/query/path payload is recorded.
use super::ServiceRegistry;
use airk_core::{
    service::ProjectBinding,
    service_events::{EventCursor, EventError, EventKind, EventPage, ServiceEvent},
};
use anyhow::{Context, Result, ensure};
use rusqlite::params;

pub(super) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS service_events(
    seq INTEGER PRIMARY KEY AUTOINCREMENT, observed_ms INTEGER NOT NULL CHECK(observed_ms>=0),
    project TEXT NOT NULL CHECK(length(project)=64), kind TEXT NOT NULL CHECK(length(kind)<=32),
    job TEXT CHECK(job IS NULL OR length(job)=64), job_kind TEXT CHECK(job_kind IS NULL OR length(job_kind)<=16),
    epoch INTEGER CHECK(epoch IS NULL OR epoch>=0), attempts INTEGER CHECK(attempts IS NULL OR attempts BETWEEN 0 AND 3),
    code TEXT CHECK(code IS NULL OR length(code)<=128));
CREATE INDEX IF NOT EXISTS service_events_project ON service_events(project,seq);
CREATE TRIGGER IF NOT EXISTS service_events_bound AFTER INSERT ON service_events BEGIN
    DELETE FROM service_events WHERE seq <= NEW.seq-1024;
END;
CREATE TRIGGER IF NOT EXISTS service_event_registration AFTER INSERT ON projects BEGIN
    INSERT INTO service_events(observed_ms,project,kind) VALUES(CAST(unixepoch('subsec')*1000 AS INTEGER),NEW.binding,'registered');
END;
CREATE TRIGGER IF NOT EXISTS service_event_pause AFTER UPDATE OF paused ON projects WHEN NEW.paused<>OLD.paused BEGIN
    INSERT INTO service_events(observed_ms,project,kind) VALUES(CAST(unixepoch('subsec')*1000 AS INTEGER),NEW.binding,CASE WHEN NEW.paused=1 THEN 'paused' ELSE 'resumed' END);
END;
CREATE TRIGGER IF NOT EXISTS service_event_accept AFTER INSERT ON service_jobs BEGIN
    INSERT INTO service_events(observed_ms,project,kind,job,job_kind,epoch,attempts)
    VALUES(CAST(unixepoch('subsec')*1000 AS INTEGER),NEW.project,'job_pending',NEW.id,NEW.kind,NEW.epoch,NEW.attempts);
END;
CREATE TRIGGER IF NOT EXISTS service_event_transition AFTER UPDATE ON service_jobs
WHEN NEW.state<>OLD.state OR NEW.epoch<>OLD.epoch OR NEW.attempts<>OLD.attempts BEGIN
    INSERT INTO service_events(observed_ms,project,kind,job,job_kind,epoch,attempts,code)
    VALUES(CAST(unixepoch('subsec')*1000 AS INTEGER),NEW.project,'job_'||NEW.state,NEW.id,NEW.kind,NEW.epoch,NEW.attempts,
    CASE WHEN NEW.state='failed' THEN json_extract(CAST(NEW.outcome AS TEXT),'$.code') ELSE NULL END);
END;
CREATE TRIGGER IF NOT EXISTS service_event_cancel AFTER INSERT ON service_job_cancellation BEGIN
    INSERT INTO service_events(observed_ms,project,kind,job,job_kind,epoch,attempts)
    SELECT CAST(unixepoch('subsec')*1000 AS INTEGER),project,'cancellation_requested',id,kind,epoch,attempts
    FROM service_jobs WHERE id=NEW.job;
END;
CREATE TRIGGER IF NOT EXISTS service_event_retry AFTER INSERT ON service_retry_requests BEGIN
    INSERT INTO service_events(observed_ms,project,kind,job,job_kind,epoch,attempts)
    SELECT CAST(unixepoch('subsec')*1000 AS INTEGER),j.project,'job_retry_requested',j.id,j.kind,j.epoch,j.attempts
    FROM service_jobs j JOIN service_requests r ON r.job=j.id WHERE r.namespace=NEW.namespace AND r.epoch=NEW.epoch AND r.nonce=NEW.nonce AND r.caller=NEW.caller;
END;
CREATE TRIGGER IF NOT EXISTS service_event_watch AFTER UPDATE OF enabled ON service_watch WHEN NEW.enabled<>OLD.enabled BEGIN
    INSERT INTO service_events(observed_ms,project,kind) VALUES(CAST(unixepoch('subsec')*1000 AS INTEGER),NEW.project,
    CASE WHEN NEW.enabled=1 THEN 'watch_enabled' ELSE 'watch_disabled' END);
END;";

impl ServiceRegistry {
    /// Snapshot pages survive new observations/restarts, but expire when rotation
    /// overtakes an unread position. Filters cannot change during continuation.
    pub fn events(
        &self,
        project: Option<ProjectBinding>,
        cursor: Option<&EventCursor>,
        limit: usize,
    ) -> Result<EventPage> {
        ensure!((1..=32).contains(&limit), EventError::InvalidLimit);
        if let Some(project) = project {
            self.get(project)?;
        }
        let tx = self.connection.unchecked_transaction()?;
        let namespace = self.incarnation()?;
        let (oldest, newest): (Option<i64>, Option<i64>) =
            tx.query_row("SELECT min(seq),max(seq) FROM service_events", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;
        let oldest = oldest.map(u64::try_from).transpose()?;
        let newest = u64::try_from(newest.unwrap_or(0))?;
        let (after, through) = match cursor {
            Some(cursor) => {
                ensure!(
                    cursor.namespace == namespace && cursor.project == project,
                    EventError::ScopeChanged
                );
                let through = cursor.through.unwrap_or(newest);
                ensure!(
                    cursor.after <= through && through <= newest,
                    EventError::InvalidCursor
                );
                ensure!(
                    oldest.is_none_or(|floor| cursor.after >= floor.saturating_sub(1)),
                    EventError::CursorExpired
                );
                (cursor.after, through)
            }
            None => (oldest.unwrap_or(1).saturating_sub(1), newest),
        };
        let mut statement = tx.prepare("SELECT seq,observed_ms,project,kind,job,job_kind,epoch,attempts,code FROM service_events
            WHERE seq>?1 AND seq<=?2 AND (?3 IS NULL OR project=?3) ORDER BY seq LIMIT ?4")?;
        let mut rows = statement.query(params![
            i64::try_from(after)?,
            i64::try_from(through)?,
            project.map(|p| p.to_string()),
            (limit + 1) as i64
        ])?;
        let mut events = Vec::new();
        let mut more = false;
        while let Some(row) = rows.next()? {
            if events.len() == limit {
                more = true;
                break;
            }
            let kind: String = row.get(3)?;
            ensure!(kind.len() <= 32, "oversized service event kind");
            let kind: EventKind = serde_json::from_value(kind.into())?;
            let job: Option<String> = row.get(4)?;
            let job_kind: Option<String> = row.get(5)?;
            let code: Option<String> = row.get(8)?;
            ensure!(
                code.as_ref().is_none_or(|c| !c.is_empty()
                    && c.len() <= 128
                    && c.bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')),
                "invalid service event failure code"
            );
            let event = ServiceEvent {
                sequence: u64::try_from(row.get::<_, i64>(0)?)?,
                observed_unix_ms: u64::try_from(row.get::<_, i64>(1)?)?,
                project: row.get::<_, String>(2)?.parse()?,
                kind,
                job: job.map(|id| id.parse()).transpose()?,
                job_kind: job_kind
                    .map(|kind| serde_json::from_value(kind.into()))
                    .transpose()?,
                fencing_generation: row
                    .get::<_, Option<i64>>(6)?
                    .map(u64::try_from)
                    .transpose()?,
                attempts: row.get(7)?,
                code,
            };
            let job_event = matches!(
                event.kind,
                EventKind::JobPending
                    | EventKind::JobRunning
                    | EventKind::JobSucceeded
                    | EventKind::JobFailed
                    | EventKind::JobCanceled
                    | EventKind::CancellationRequested
                    | EventKind::JobRetryRequested
            );
            ensure!(
                event.job.is_some() == job_event
                    && event.job_kind.is_some() == job_event
                    && event.fencing_generation.is_some() == job_event
                    && event.attempts.is_some() == job_event
                    && event.attempts.is_none_or(|n| n <= 3)
                    && event.code.is_some() == (event.kind == EventKind::JobFailed),
                "incoherent service event fields"
            );
            ensure!(
                serde_json::to_vec(&event)?.len() <= 1024,
                "service event exceeds byte bound"
            );
            events.push(event);
        }
        let next = if more {
            Some(EventCursor {
                namespace,
                project,
                after: events.last().context("empty event page")?.sequence,
                through: Some(through),
            })
        } else {
            None
        };
        let page = EventPage {
            namespace,
            oldest_sequence: oldest,
            snapshot_through: through,
            retained_prefix_lost: cursor.is_none() && oldest.is_some_and(|floor| floor > 1),
            events,
            next,
            tail: EventCursor {
                namespace,
                project,
                after: through,
                through: None,
            },
        };
        ensure!(
            serde_json::to_vec(&page)?.len() <= 48 * 1024,
            "service event page exceeds byte bound"
        );
        Ok(page)
    }
}

#[cfg(test)]
#[path = "service_events_tests.rs"]
mod tests;
