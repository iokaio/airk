# AIRK 1.8.5 local candidate

The owner requested 1.8.5 for manual Windows installer testing. This unpublished candidate adds configurable concurrent evidence requests to Management → Service. Native and extension versions, lockfiles and the offline template catalog are synchronized at 1.8.5. Earlier 1.8.4 scenario and package receipts remain historical evidence.

## Behavior

- **Concurrent requests…** selects one of **4, 8, 16, 32 or 64**, default **16**. Unlisted, fractional and out-of-range values are rejected by native validation; the editor selector lists exactly 4, 8, 16 (default), 32 and 64 channels. Existing saved resource policies without the field use 16. Changing a resource preset preserves this setting.
- The same service-host `query_concurrency` controls the MCP evidence execution window, each frontend's independent authenticated native channel pool, and each project worker's actual query allowance across its callers. An additional status/control slot preserves headroom. The metadata service remains outside the evidence payload path.
- Native owners acknowledge live capacity changes before the service saves the policy. Subsequent frontend calls observe it. Reductions retain active charges and drain accepted work; new requests wait rather than revoking accepted queries. Idle surplus channels are retired on subsequent pool use. Queues remain bounded at 128 waiting requests per relevant admission stage, with finite deadlines and explicit refusal receipts.
- The worker's former one-query grant and shared one-materialization gate no longer serialize the pool. Worker-local broker admission supplies the configured allowance; the shared lifetime gate has a finite 65-slot ceiling and retains accepted blocking work through source retrieval, rendering, usage accounting and transport cancellation. Increasing a request allowance does not change source authorization, revision identity, source write protections or typed retry rules.

Concurrent read-only manifest selection shares its validated OS lock; manifest replacement and owning control commits retain exclusive locks. A 64-reader regression verifies writers remain excluded until every reader releases.

The implementation stays in the existing Core contracts, MCP transport/execution adapters, CLI service/client/worker adapters and Management UI. No agent orchestration, external service or new crate is introduced. The previously observed conservative Rust type-navigation limitation is outside this change.

The CLI uses the same `resources`/`configure_resources` operations through `airk service rpc`; the [CLI example](../service-resources.md#cli-access) preserves other overrides. Running workers acknowledge updates before persistence, existing frontends observe the new policy on their next request, and other Management windows refresh it through their normal observations. No agent reconnect or service restart is needed for a successful update.

## Validation and packaging

Validation results and exact manual-test artifact identities are recorded here after the checks and fresh package build complete. No normal installation, commit, push, PR, merge or publication is part of this task. Windows evidence does not qualify macOS/Linux or installed-editor manual acceptance.

Focused coverage includes original-source-preserving actual IPC tests at 4/16/32/64 independent channels, in both source-bound and pinned modes, with a barrier that requires every query to arrive before any reply is released. This proves channel parallelism rather than counting successful sequential replies. Broker tests separately hold the matching configured request count plus status capacity simultaneously and verify lower-limit drain without releasing accepted response-memory charges.
