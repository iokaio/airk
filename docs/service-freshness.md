# Service source freshness observations

**1.8.8 P4 development:** [Saved-source verification](source-verification.md) defaults to Report for evidence reads, with bounded native checks and explicit Off/Require choices. CLI, MCP, Workbench and Inspector/Checker retain dated outcomes and uncertainty. Phase acceptance and performance qualification remain in the [ledger](research/implementation-v190.md).

Service freshness describes saved source-work intent independently of authenticated index identity and extraction coverage. It does not verify live source files. The service registry combines its watcher state, pending/running source jobs and latest source outcome in one bounded read transaction. Verification, vector-only work and other derived maintenance cannot overwrite the latest source failure. Planned repair work reuses its existing durable source-job association.

## Source work during semantic backlogs

The P6d working implementation visits bounded source-admission pages before
semantic continuations. A shared Store eligibility predicate also guards the
Vector preview and transactional debit, so a stale preview cannot consume the
next writer opportunity ahead of admissible source work. Disabled watching,
pause, retry/backoff and blocked revisions remain meaningful; source lag alone
does not authorize work. Explicitly accepted source jobs can take priority when
watching is disabled. Pending-job selection and transactional claims also prefer eligible source work
over pending semantic-only Vectors, including an explicitly requested Vector. Their saved job IDs, request receipts and remaining
work stay intact. Other accepted work retains its ordering. Automatic reconciliation and saved
source continuations can be admitted while those Vectors remain pending; the
transactional claim recheck refuses a stale Vector grant without canceling its
job or substituting another job.

The existing work-status pulse carries a reversible source-yield hint, fenced to
the current worker and work grant. A Vector turn checks it at safe boundaries,
preserves its published work and saved continuation, then relinquishes its
writer. Pause and cancellation retain precedence. The hint introduces no source
walk, model opening or registry work into retrieval calls. Pending/finished job
observation races return a quiet false hint; a wrong project or missing job
still fails validation.

The existing 32-artifact/30-second turn limits are soft budgets. Inference and an
atomic publication are not interrupted to promise a hard 30-second deadline.
Native fairness fixtures measure the existing turn and bounded admission/source
publication overhead separately, and require subsequent Vector progress. Current
qualification and remaining platform limits are in the
[implementation ledger](research/implementation-v190.md#p6d--source-admission-fairness-3-november-2026-accepted).

Explorer labels this observation **Source work**. From 1.5.5, native directory planning uses indexing's ignore policy, so an unreadable fully ignored output directory does not by itself make the watched source scope unavailable. Included-directory failures, ignore-rule errors and exhausted traversal budgets remain explicit. This changes notification scope, not semantic coverage or the meaning of a published revision; see [source watching](service-source-watching.md).

Each project retains one source-outcome summary, at most 2 KiB of outcome payload, within the registry's existing 32 MiB ceiling. Terminal transitions update it in the same transaction as the job receipt, including pending cancellation and interrupted-start settlement. Job-receipt rotation cannot erase the latest source failure. On open, existing retained terminal jobs seed missing summaries using bounded aggregation over the registry's maximum 1,024 jobs. Source attempts have distinct `source_job` IDs; a new failure with the same code is still a changed observation. A job ID is separate from the exact index revision token.

The supervisor sends a content-free sample through the existing private work-status control, normally once per second. The worker accepts it only for its exact project/worker/fencing identity and registered source/data/collection binding. Its kernel clones share one cache; retrieval calls read that cache without registry or legacy-scheduler I/O. Samples are bounded to 4 KiB and reject live-source claims or inconsistent source watermarks. Missing or rejected metadata stays unavailable.

After five seconds without a received valid sample, the cache reports unavailable while retaining the last observed watermarks, source attempt and bounded failure context. Expiry uses a monotonic clock and never releases work capacity, ownership or evidence leases. The original service observation timestamp remains visible; the interval is a cache policy, not a real-time scheduling guarantee. A stale observation does not become current merely because a client asks for Full.

Terse clients receive first/changed/requested observations through their own [status receipts](mcp-response-modes.md). Publication readiness and source availability remain separate: retained evidence stays readable when a source root disappears, while the rejected source attempt reports its actual failure. No query text, source content or embedding payload enters service brokerage for this metadata.

The [completion checklist](service-implementation-checklist.md) retains console rendering, source permission/redirect diagnostics, remaining lifecycle work and installed-platform qualification.

October 23, 2026 development evidence: the full store library passed 216 Windows tests and 215 Linux tests, followed by three focused source-summary review tests on each platform. Two cache/scope tests passed on Windows and Linux. Both platforms passed nine real stdio tests and fourteen native service process regressions; Linux also passed twenty MCP tests and seventy-eight CLI adapters. The real missing-root fixture verifies forwarding, independent clients, renewed status receipts and the exact failed source-attempt ID. Windows all-feature/all-target workspace Clippy and Linux CLI all-target Clippy passed; formatting, repository and whitespace checks passed. The main Linux image is `7872e94246e2c7215aa951e0438662af5702c3ba8b04ccfe4312de31d353c8d2`; the focused final review image is `c88460f9eb469323edc07b2afc40c7f6891402b3cf427b28e6fdaf79ec4edede`. Installed macOS/editor acceptance remains open; no VSIX was built.


## Source availability and retained reads

The 1.4.0 development worker can open already-owned retained evidence when a saved source root is missing, permission denied, redirected or replaced by a non-directory. The source identity stays bound to its original registered path; it never follows a redirect to capture another tree. New registrations still require available canonical roots. Unavailable roots never initialize an unbound collection. Absolute spelling, collection ownership and independent data-root checks precede retained reads.

Native status reports a content-free `source_availability.reason`: `missing`, `permission_denied`, `redirected`, `not_directory` or `io_unavailable`. Unknown configuration failures remain separate. Availability checks open the root directory without enumerating its contents, because canonicalization/stat can succeed even when directory access is denied. Additional registered roots receive the same availability check before capture. A failed source admission rejects new scans/rebuilds with `source_unavailable`, preserving the indexed token and retained evidence instead of publishing deletions. Availability is independent of readiness, indexed revision identity and live-source freshness.

Development checks cover actual non-root Unix permission denial at the source and an ancestor, a redirected root with a separate untouched source tree, a directory replaced by a file, and refusal to create an unbound collection. The Windows native fixture denies directory listing on its own temporary source, starts a worker afterward, queries prior evidence and checks the durable failed-job code and original bytes. Platform results and remaining console/installed-platform gates are recorded in the completion checklist.

Availability evidence, October 23, 2026: Windows passed two kernel availability regressions, four service-publication tests and the inspection regression. All fourteen prior native service scenarios passed, followed by the new real-ACL denied-root process regression with its query connection released before shutdown. Non-root Linux passed all four availability regressions, four publication tests, inspection and fourteen native service scenarios. Windows all-feature workspace Clippy and Linux CLI Clippy pass; MCP and repository/Python checks remain part of the recorded development matrix. The Unix denial fixtures restore permissions on both success and failure; the Windows fixture removes only its own explicit denial on its temporary source. No actual macOS runtime result is claimed.

The 1.8.8 [evidence guidance](evidence-guidance.md) labels outstanding observed
source watermarks as `stale` with subject `source_work_watermark`. That label
does not establish that a cited file differs on disk; original observation age,
failure and live-verification limits remain authoritative. Advice starts no work.
