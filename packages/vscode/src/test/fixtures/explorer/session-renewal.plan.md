---
schema: airk.explorer.plan/1
title: "Session renewal"
workspace: "fixture"
project: abababababababababababababababababababababababababababababababab
publication: "airk-r1:fixture:7"
extension_version: "1.5.0"
tasks:
  - id: T1
    depends_on: []
    status: completed
    evidence: "verified_snapshot airk-r1:fixture:7 cccccccccccccccc"
  - id: T2
    depends_on: [T1]
    status: ready
    evidence: none
  - id: T3
    depends_on: [T2]
    status: blocked
    evidence: none
---

# Session renewal

Stop early session expiry.

## Before you start

- Read this whole plan before acting.
- Verify every cited file against the current workspace; treat excerpts as candidates, not facts.
- Do only the task you were asked to do. Stop and report if a dependency is not complete or evidence is stale.
- Report changed files and test results, then ask before continuing.

## T1. Find the renewal path

Depends on: nothing · Status: completed

Evidence: airk.core/bug@1.0.0, verified snapshot at publication airk-r1:fixture:7.

Trace renew() callers.

## T2. Fix the clock check

Depends on: T1 · Status: ready

Make the minimal fix.

## T3. Add a regression test

Depends on: T2 · Status: blocked

Cover the expiry boundary.
