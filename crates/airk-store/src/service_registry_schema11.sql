-- SPDX-License-Identifier: Apache-2.0
-- Original compiled empty schema 11; regenerate with the registry_schema example.
-- DDL only: contains no registrations, user rows, paths or source contents.

CREATE TABLE projects(binding TEXT PRIMARY KEY, payload BLOB NOT NULL CHECK(length(payload)<=20480),
                epoch INTEGER NOT NULL CHECK(epoch>=0), paused INTEGER NOT NULL CHECK(paused IN(0,1)), registered INTEGER NOT NULL DEFAULT 1 CHECK(registered IN(0,1)), cleanup_guard TEXT CHECK(cleanup_guard IS NULL OR length(cleanup_guard)=64)) WITHOUT ROWID;

CREATE TABLE service_background(
 project TEXT PRIMARY KEY REFERENCES projects(binding),
 revision INTEGER NOT NULL CHECK(revision>=0),
 source_pending INTEGER NOT NULL CHECK(source_pending IN(0,1)),
 semantic_pending INTEGER NOT NULL CHECK(semantic_pending IN(0,1))) WITHOUT ROWID;

CREATE TABLE service_background_audit(
 project TEXT PRIMARY KEY REFERENCES projects(binding) ON DELETE CASCADE,
 revision INTEGER NOT NULL CHECK(revision>=0),
 campaign INTEGER NOT NULL CHECK(campaign>=0),
 job TEXT UNIQUE REFERENCES service_jobs(id) ON DELETE SET NULL,
 parked INTEGER NOT NULL CHECK(parked IN(0,1)),
 manual INTEGER NOT NULL CHECK(manual IN(0,1)),
 intent BLOB CHECK(intent IS NULL OR length(intent)<=4096),
 coverage BLOB CHECK(coverage IS NULL OR length(coverage)<=4096),
 blocked BLOB CHECK(blocked IS NULL OR length(blocked)<=4096),
 next_ms INTEGER NOT NULL CHECK(next_ms>=0),
 CHECK(parked=0 OR job IS NOT NULL)) WITHOUT ROWID;

CREATE TABLE service_configuration(
    job TEXT PRIMARY KEY REFERENCES service_jobs(id) ON DELETE CASCADE,
    payload BLOB NOT NULL CHECK(length(payload)<=16640),
    result BLOB NOT NULL CHECK(length(result)=16392));

CREATE TABLE service_events(
    seq INTEGER PRIMARY KEY AUTOINCREMENT, observed_ms INTEGER NOT NULL CHECK(observed_ms>=0),
    project TEXT NOT NULL CHECK(length(project)=64), kind TEXT NOT NULL CHECK(length(kind)<=32),
    job TEXT CHECK(job IS NULL OR length(job)=64), job_kind TEXT CHECK(job_kind IS NULL OR length(job_kind)<=16),
    epoch INTEGER CHECK(epoch IS NULL OR epoch>=0), attempts INTEGER CHECK(attempts IS NULL OR attempts BETWEEN 0 AND 3),
    code TEXT CHECK(code IS NULL OR length(code)<=128));

CREATE TABLE service_inspections(
    project TEXT PRIMARY KEY REFERENCES projects(binding),payload BLOB NOT NULL CHECK(length(payload)<=9216)) WITHOUT ROWID;

CREATE TABLE service_job_cancellation(
        job TEXT PRIMARY KEY REFERENCES service_jobs(id) ON DELETE CASCADE) WITHOUT ROWID;

CREATE TABLE service_job_recovery(
 job TEXT PRIMARY KEY REFERENCES service_jobs(id) ON DELETE CASCADE,
 reason TEXT NOT NULL CHECK(length(reason)<=128),condition BLOB NOT NULL CHECK(length(condition)<=1024),
 waiting INTEGER NOT NULL CHECK(waiting IN(0,1)),next_ms INTEGER NOT NULL,started_ms INTEGER NOT NULL) WITHOUT ROWID;

CREATE TABLE service_job_source(
        job TEXT PRIMARY KEY REFERENCES service_jobs(id) ON DELETE CASCADE,
        revision INTEGER NOT NULL CHECK(revision>=0)) WITHOUT ROWID;

CREATE TABLE service_jobs(
        seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE,
        project TEXT NOT NULL REFERENCES projects(binding), kind TEXT NOT NULL,
        state TEXT NOT NULL, epoch INTEGER NOT NULL CHECK(epoch>=0),
        attempts INTEGER NOT NULL CHECK(attempts>=0 AND attempts<=3),
        outcome BLOB CHECK(outcome IS NULL OR length(outcome)<=2048));

CREATE TABLE service_lifecycle(
        seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE,
        request BLOB NOT NULL CHECK(length(request)<=2048),
        phase TEXT NOT NULL CHECK(phase IN('draining','starting','complete','abandoned')));

CREATE TABLE service_lifecycle_state(
        id INTEGER PRIMARY KEY CHECK(id=1), stopped INTEGER NOT NULL CHECK(stopped IN(0,1)),
        instance TEXT CHECK(instance IS NULL OR length(instance)=32));

CREATE TABLE service_maintenance(
        project TEXT PRIMARY KEY REFERENCES projects(binding),
        observation BLOB NOT NULL CHECK(length(observation)<=4096),
        pending INTEGER,failed INTEGER NOT NULL CHECK(failed IN(0,1)),
        retries INTEGER NOT NULL CHECK(retries BETWEEN 0 AND 7),next_ms INTEGER NOT NULL) WITHOUT ROWID;

CREATE TABLE service_meta(id INTEGER PRIMARY KEY CHECK(id=1), incarnation TEXT NOT NULL,
                revision INTEGER NOT NULL CHECK(revision>=0), request_epoch INTEGER NOT NULL CHECK(request_epoch>=0));

CREATE TABLE service_plan_items(
    plan TEXT NOT NULL REFERENCES service_plans(id) ON DELETE CASCADE,
    project TEXT NOT NULL REFERENCES projects(binding),phase TEXT NOT NULL,payload BLOB NOT NULL CHECK(length(payload)<=16384),
    attempts INTEGER NOT NULL CHECK(attempts BETWEEN 0 AND 3),next_ms INTEGER NOT NULL CHECK(next_ms>=0),
    PRIMARY KEY(plan,project)) WITHOUT ROWID;

CREATE TABLE service_plan_jobs(
    plan TEXT NOT NULL,project TEXT NOT NULL,step TEXT NOT NULL CHECK(step IN('inspect','action')),
    job TEXT NOT NULL REFERENCES service_jobs(id) ON DELETE CASCADE,
    FOREIGN KEY(plan,project) REFERENCES service_plan_items(plan,project) ON DELETE CASCADE,
    PRIMARY KEY(plan,project,step)) WITHOUT ROWID;

CREATE TABLE service_plan_requests(
    namespace TEXT NOT NULL,epoch INTEGER NOT NULL,nonce TEXT NOT NULL,caller TEXT NOT NULL,
    plan TEXT NOT NULL REFERENCES service_plans(id) ON DELETE CASCADE,
    PRIMARY KEY(namespace,epoch,nonce,caller)) WITHOUT ROWID;

CREATE TABLE service_plans(
    seq INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT UNIQUE NOT NULL CHECK(length(id)=64),
    request BLOB NOT NULL CHECK(length(request)<=20000),engine TEXT NOT NULL CHECK(length(engine)=64),phase TEXT NOT NULL,
    paused INTEGER NOT NULL CHECK(paused IN(0,1)),paused_at INTEGER NOT NULL CHECK(paused_at>=0),canceled INTEGER NOT NULL CHECK(canceled IN(0,1)),
    preview TEXT,fallback_approved INTEGER NOT NULL CHECK(fallback_approved IN(0,1)),outcome TEXT,
    inspection_until INTEGER NOT NULL CHECK(inspection_until>=0),created_ms INTEGER NOT NULL CHECK(created_ms>=0),updated_ms INTEGER NOT NULL CHECK(updated_ms>=0));

CREATE TABLE service_private_paths(
    path TEXT PRIMARY KEY CHECK(length(path)<=4096)) WITHOUT ROWID;

CREATE TABLE service_progress(
    project TEXT PRIMARY KEY REFERENCES projects(binding),
    job TEXT NOT NULL REFERENCES service_jobs(id) ON DELETE CASCADE,
    observation BLOB NOT NULL CHECK(length(observation)<=4096)) WITHOUT ROWID;

CREATE TABLE service_provider_drains(
 id TEXT PRIMARY KEY CHECK(length(id)=32),
 payload BLOB NOT NULL CHECK(length(payload)<=4096),
 digest TEXT NOT NULL CHECK(length(digest)=64)) WITHOUT ROWID;

CREATE TABLE service_recovery_budget(
 scope TEXT PRIMARY KEY,engine TEXT NOT NULL,failures INTEGER NOT NULL CHECK(failures BETWEEN 0 AND 3),
 next_ms INTEGER NOT NULL,started_ms INTEGER NOT NULL,suppressed INTEGER NOT NULL CHECK(suppressed IN(0,1))) WITHOUT ROWID;

CREATE TABLE service_removed_projects(binding TEXT PRIMARY KEY CHECK(length(binding)=64),root_identity TEXT NOT NULL CHECK(length(root_identity)=64),removed_ms INTEGER NOT NULL CHECK(removed_ms>=0)) WITHOUT ROWID;

CREATE TABLE service_requests(
        namespace TEXT NOT NULL, epoch INTEGER NOT NULL CHECK(epoch>=0), nonce TEXT NOT NULL,
        caller TEXT NOT NULL, job TEXT NOT NULL REFERENCES service_jobs(id) ON DELETE CASCADE,
        PRIMARY KEY(namespace,epoch,nonce,caller)) WITHOUT ROWID;

CREATE TABLE service_retry_requests(
        namespace TEXT NOT NULL,epoch INTEGER NOT NULL,nonce TEXT NOT NULL,caller TEXT NOT NULL,
        source_job TEXT NOT NULL CHECK(length(source_job)=64),
        PRIMARY KEY(namespace,epoch,nonce,caller),
        FOREIGN KEY(namespace,epoch,nonce,caller) REFERENCES service_requests(namespace,epoch,nonce,caller) ON DELETE CASCADE) WITHOUT ROWID;

CREATE TABLE service_semantic_review(
 project TEXT PRIMARY KEY REFERENCES projects(binding),signature TEXT NOT NULL CHECK(length(signature)<=129)) WITHOUT ROWID;

CREATE TABLE service_setup_pending(project TEXT PRIMARY KEY REFERENCES projects(binding)) WITHOUT ROWID;

CREATE TABLE service_source_outcomes(
        project TEXT PRIMARY KEY REFERENCES projects(binding) ON DELETE CASCADE,
        seq INTEGER NOT NULL CHECK(typeof(seq)='integer' AND seq>0),
        job TEXT NOT NULL CHECK(length(job)=64),
        state TEXT NOT NULL CHECK(state IN('succeeded','failed','canceled')),
        outcome BLOB NOT NULL CHECK(length(outcome)<=2048)) WITHOUT ROWID;

CREATE TABLE service_watch(
        project TEXT PRIMARY KEY REFERENCES projects(binding),
        enabled INTEGER NOT NULL CHECK(enabled IN(0,1)),
        requested INTEGER NOT NULL CHECK(requested>=0),
        indexed INTEGER NOT NULL CHECK(indexed>=0 AND indexed<=requested),
        configuration_epoch INTEGER NOT NULL CHECK(configuration_epoch>=0),
        roots BLOB NOT NULL CHECK(length(roots)<=98304),roots_observed INTEGER NOT NULL CHECK(roots_observed IN(0,1)),
        native_roots INTEGER NOT NULL CHECK(native_roots BETWEEN 0 AND 17),
        backend TEXT NOT NULL,failure TEXT,observed_ms INTEGER,last_reconciled_ms INTEGER,
        attempts INTEGER NOT NULL CHECK(attempts BETWEEN 0 AND 3),next_ms INTEGER NOT NULL,
        blocked_revision INTEGER) WITHOUT ROWID;

CREATE TABLE service_watch_events(
        project TEXT PRIMARY KEY REFERENCES projects(binding),worker TEXT NOT NULL,
        configuration_epoch INTEGER NOT NULL CHECK(configuration_epoch>=0),
        sequence INTEGER NOT NULL CHECK(sequence>0)) WITHOUT ROWID;

CREATE TABLE service_watch_traversal(
        project TEXT PRIMARY KEY REFERENCES projects(binding) ON DELETE CASCADE,
        configuration_epoch INTEGER NOT NULL CHECK(configuration_epoch>=0),
        observation BLOB NOT NULL CHECK(length(observation)<=8192),
        observed_ms INTEGER NOT NULL CHECK(observed_ms>=0)) WITHOUT ROWID;

CREATE INDEX service_events_project ON service_events(project,seq);

CREATE INDEX service_jobs_pending ON service_jobs(project,state,seq);

CREATE UNIQUE INDEX service_lifecycle_active ON service_lifecycle((1)) WHERE phase NOT IN('complete','abandoned');

CREATE INDEX service_plan_items_stage ON service_plan_items(plan,phase,project);

CREATE INDEX service_plan_jobs_job ON service_plan_jobs(job);

CREATE TRIGGER service_event_accept AFTER INSERT ON service_jobs BEGIN
    INSERT INTO service_events(observed_ms,project,kind,job,job_kind,epoch,attempts)
    VALUES(CAST(unixepoch('subsec')*1000 AS INTEGER),NEW.project,'job_pending',NEW.id,NEW.kind,NEW.epoch,NEW.attempts);
END;

CREATE TRIGGER service_event_cancel AFTER INSERT ON service_job_cancellation BEGIN
    INSERT INTO service_events(observed_ms,project,kind,job,job_kind,epoch,attempts)
    SELECT CAST(unixepoch('subsec')*1000 AS INTEGER),project,'cancellation_requested',id,kind,epoch,attempts
    FROM service_jobs WHERE id=NEW.job;
END;

CREATE TRIGGER service_event_pause AFTER UPDATE OF paused ON projects WHEN NEW.paused<>OLD.paused BEGIN
    INSERT INTO service_events(observed_ms,project,kind) VALUES(CAST(unixepoch('subsec')*1000 AS INTEGER),NEW.binding,CASE WHEN NEW.paused=1 THEN 'paused' ELSE 'resumed' END);
END;

CREATE TRIGGER service_event_registration AFTER INSERT ON projects BEGIN
    INSERT INTO service_events(observed_ms,project,kind) VALUES(CAST(unixepoch('subsec')*1000 AS INTEGER),NEW.binding,'registered');
END;

CREATE TRIGGER service_event_retry AFTER INSERT ON service_retry_requests BEGIN
    INSERT INTO service_events(observed_ms,project,kind,job,job_kind,epoch,attempts)
    SELECT CAST(unixepoch('subsec')*1000 AS INTEGER),j.project,'job_retry_requested',j.id,j.kind,j.epoch,j.attempts
    FROM service_jobs j JOIN service_requests r ON r.job=j.id WHERE r.namespace=NEW.namespace AND r.epoch=NEW.epoch AND r.nonce=NEW.nonce AND r.caller=NEW.caller;
END;

CREATE TRIGGER service_event_transition AFTER UPDATE ON service_jobs
WHEN NEW.state<>OLD.state OR NEW.epoch<>OLD.epoch OR NEW.attempts<>OLD.attempts BEGIN
    INSERT INTO service_events(observed_ms,project,kind,job,job_kind,epoch,attempts,code)
    VALUES(CAST(unixepoch('subsec')*1000 AS INTEGER),NEW.project,'job_'||NEW.state,NEW.id,NEW.kind,NEW.epoch,NEW.attempts,
    CASE WHEN NEW.state='failed' THEN json_extract(CAST(NEW.outcome AS TEXT),'$.code') ELSE NULL END);
END;

CREATE TRIGGER service_event_watch AFTER UPDATE OF enabled ON service_watch WHEN NEW.enabled<>OLD.enabled BEGIN
    INSERT INTO service_events(observed_ms,project,kind) VALUES(CAST(unixepoch('subsec')*1000 AS INTEGER),NEW.project,
    CASE WHEN NEW.enabled=1 THEN 'watch_enabled' ELSE 'watch_disabled' END);
END;

CREATE TRIGGER service_events_bound AFTER INSERT ON service_events BEGIN
    DELETE FROM service_events WHERE seq <= NEW.seq-1024;
END;

CREATE TRIGGER service_source_outcome_updated AFTER UPDATE OF state,outcome ON service_jobs
    WHEN NEW.state IN('succeeded','failed','canceled') AND EXISTS(SELECT 1 FROM service_job_source WHERE job=NEW.id)
    BEGIN
        INSERT INTO service_source_outcomes(project,seq,job,state,outcome) VALUES(NEW.project,NEW.seq,NEW.id,NEW.state,NEW.outcome)
        ON CONFLICT(project) DO UPDATE SET seq=excluded.seq,job=excluded.job,state=excluded.state,outcome=excluded.outcome
        WHERE excluded.seq>=service_source_outcomes.seq;
    END;
