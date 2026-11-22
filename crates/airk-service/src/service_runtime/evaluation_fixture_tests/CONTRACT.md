# Private actual evaluation fixture

The historical ignored selector retains its original disabled startup, 2048-token
request and default service resource configuration. The additive software-v2
selector retains its native 8192-token/120-second request and requires explicit
`AIRK_ARBITER_EVALUATION_RESOURCE_CONSENT=throughput64gib4threads` before creating
state or starting a service. That consent selects only the native Core fixture
configuration Throughput, 64 GiB memory ceiling and 4 inference threads.
Service-wide GPU use is opt-in, so a fresh fixture service starts with it
off. The CUDA Answer selector states `allow_gpu: true` in that same reviewed
request. Every other selector leaves GPU use off.

After authentic Health binds this test process/image/catalog and before Ready,
the current selector sends Core ConfigureResources over actual same-user IPC,
retains its full typed request/response, then reads and retains actual Resources.
Both observations must report the exact requested configuration without a
configuration error. A refusal or unknown result follows existing startup-failure
shutdown/owned-task settlement; it cannot emit Ready. No retries, fixed sleeps,
extra work deadline or modified admission rule is introduced.

When startup refuses, the fixture retains the existing unknown marker and
joins the actual service task. If that task failed, its error is reported
alongside the startup-phase error after the observers close.

A configured ceiling does not establish host fit, available RAM, model load,
provider/CPU readiness or safe coexistence. Real snapshots retain all native
pressure, telemetry, reservation and resource facts; subsequent actual warm and
call receipts must establish those outcomes. Historical evidence remains under
its original policy. No public model metadata/source closure is manufactured.

The additive `emit_private_selected_synthetic_fixture_software_v2` selector uses
the same synthetic source and request emitter with the current Rust-owned
8192-token/120-second Answer template, including complete response options. The
original synthetic selector preserves its historical2048/30 Answer template.
Only the question is filled from this native emitter's fixed synthetic prompt;
no Python layer reconstructs or rewrites the request contract or model budget.


## External read-only sources for the configuration emitter

`emit_private_arbiter_configuration` admits only frozen scope roots under
`artifacts`, unless `AIRK_ARBITER_EVALUATION_EXTERNAL_SOURCES` holds
`{"mode":"read-only","source_roots":[...]}` naming, after canonicalization,
exactly the roots of the SHA-pinned scope file that lie outside `artifacts`.
A different, extra or duplicate root, another mode or an unknown field is
refused. The emitter still reads only service state and writes only its
private outputs under the fixture work root; it never writes into a source.
Every other emitter check is unchanged. The declaration exists so an
evaluation can enroll existing checkouts indexed in place; it adds no
authority over those checkouts.

## Answer evaluation change-set selectors (22 November 2026)

Every selector below is ignored, private and test-only. Each reads only
SHA-pinned inputs under `artifacts` and writes only to a fresh work or output
folder there. None creates a public wire or persisted product format, and none
claims release qualification.

- `restart::change_set`
  (`actual_private_arbiter_fixture_change_set_1`): the CPU fixture with
  change-set layers bound once for the process
  (`AIRK_ARBITER_EVALUATION_ANSWER_LAYERS`, default `all`). The CPU roster
  stays the restart identity. A restart may also reopen a prior change-set
  lifetime whose roster is that roster plus the same extras. Between one and
  eight extras (`AIRK_ARBITER_EVALUATION_CHANGE_SET_INPUTS` and its SHA-256)
  join only the evaluation catalog through `arbiter::catalog_only`. They are
  never seeded or copied. Each extra must be an unqualified `private-` CPU
  profile with a new manifest that reuses its roster twin's `Model` and
  `ExternalWeights` assets. A pinned replacement file (override) may supply an
  asset only when its SHA-256 equals the profile's declaration. A *listed
  candidate* is the one exception for weights. The input names a SHA-pinned
  candidate file in fixture format that lists exactly this profile, asset
  folder and fixture. The profile must be Focus, share the twin's `Model`
  assets and count, and differ only in its single `ExternalWeights` asset. A
  fresh installation must name `AIRK_ARBITER_EVALUATION_CHANGE_SET_SEED_CPU=all|focus|none`;
  a restart never seeds. The CAL observer must stay off. The emitter
  `emit_private_change_set_1_profiles` writes the width-16 Focus variants and
  the provisional-Read Context variants. It never reads or copies weights.
- `restart::change_set_2` (`emit_private_change_set_2_context_b_profiles`):
  for each CPU Context roster profile, writes one option-B variant. The
  variant has the `context_wide` task policy, a provisional calibration bound
  to that policy and a new manifest, in change set 1's input format, so the
  change-set selector runs it unchanged. The kernel builds option-B rosters
  only when the `context_b` layer is bound.
- `cuda_answer::same_install`
  (`actual_private_arbiter_fixture_cuda_same_install`): reopens, or freshly
  creates (`AIRK_ARBITER_EVALUATION_CUDA_SEED_CPU=all|focus|none`), the CPU
  fixture's installation. It adds one or two unqualified CUDA Context
  profiles (at most one Compact and one Large), each with a new manifest and a
  CPU twin of the same model. With one entry, the reviewed graph result must
  name that exact manifest. With `AIRK_ARBITER_EVALUATION_CUDA_REVIEWS` (a JSON
  list of `{path, sha256, profile_sha256}`, one per roster entry in order),
  each entry's pin is its pass-1 graph diagnostic receipt's bounded policy,
  which the profile's placement must name; two entries require it. The receipt
  must be complete, without a diagnostic error, with a passing CPU payload
  check, and of the pinned pass-1 profile (`profile_sha256`) with the same id,
  model and device execution as the entry except placement and memory. The CPU
  roster stays the restart identity. A lifetime may switch between the two
  entries through ordinary Configure; the selector itself does not issue such
  a switch, and no GPU lifetime has run one yet (owner: the LG preparation
  run). The harness side exists: `scripts/arbiter-answer-eval/cuda_pair.py`
  builds authenticated, copied asset directories (the rooted opener refuses
  hard-linked files) and the pinned roster, review list and adapter. `gpu_fixture.py --model pair`
  starts the lifetime and records the pair's profile rows, so
  `stage --disable/--enable` and `run_plan.py` `switch_context` name both
  entries in one lifetime. The CUDA rows must be the post-L4 re-emitted FP16
  profiles with the four card tasks (procedure in `GPU_FIXTURE.md`).
  The roster may also hold one strict-GPU CUDA Focus profile with a CPU Focus
  twin and no review pin. It joins the Focus stage's map on the same lease and
  adapter, is probed under the evaluation binding, and warms through the
  standard charged route, where the scorer verifies strict placement.
  Revocation, idle unload and the release order cover it. Closed (22 November
  2026; earlier the seam attached Context entries only, so a CUDA Focus row
  was refused): measured on the A6000 with the W8 1,024-token Focus profile
  `a3157dd1` (warm 8.6 s, 106-112 ms per warm call, peak 2,357 MiB used
  against a 557 MiB baseline; receipt `L5C/runs/f3-focus-seam-summary.json`). Closed (L5C review M3): the first version accepted a receipt whose CPU
  payload check had failed, or a receipt of another profile of the same model.
  Both are now refused (`review_pin_tests`, 2 tests); the ignored `l5c_switch`
  selector applies the same rule and needs `L5C_<MODEL>_PASS1_PROFILE_SHA256`.
- `cuda_answer::cs1` (`actual_private_arbiter_fixture_change_set_1_cuda`, plus
  the emitter `emit_private_cuda_provisional_read_variants`): runs the
  change-set lifetime with exactly one CUDA Context variant and no Focus
  candidate. The variant may differ from its pinned reviewed CUDA base only in
  its calibration and identity. The reviewed graph result must name the base.
  The variant joins through the private CUDA Answer seam, not as a
  catalog-only extra. The bounded-hybrid gate checks graph, placement and
  adapter against the base.

## Actual private Large storage/control regression

The ignored `actual_large_finalization_and_cached_verification_keep_native_control_and_lease_custody`
selector requires an explicit SHA-pinned current three-profile input and fresh
protected work root. Its opaque same-lease EvaluationOwner selects only private
Large storage work while both stages remain disabled. One genuine native
AssetWriter publication copies the already sealed source descriptors; real
Transaction::finish hashes the complete16GB closure. A same-owner restart then
runs real AssetStore::pin over the resulting native custody. Neither operation
loads a model, supplies URLs, changes qualification or admits public Provision
or OfflineImport.

Both actual Store calls run through the production joined owned-asset boundary.
Observed channels gate entry after authentic service Health. The test requires
actual I1 Health and Cancel complete while the real hash remains active, Cancel
retain unfinished custody, and actual I1 Shutdown/service component exit plus
exclusive ServiceLease reacquisition follow real worker return. The service's
original30-second Draining cutoff is unchanged; timeout/refusal stays failed or
unknown and cannot emit a successful closure. No detached worker, abort, process
kill or fixed sleep supplies evidence. I1 Stopping is acceptance; this fixture
has no managed installer and never claims durable ManagedStopComplete. Native
test process exit is a separate external runner observation.

This qualifies only the actual private storage/control boundary under its
recorded inputs. Public source availability, release admission, inference and
model resource/numerical qualification remain separate and may be blocked.


The Store test-support hash observer now supplies the overlap event only after
actual positive file-read/digest progress and clears it at real hash-scope exit.
The Service callback is synchronous/nonblocking and performs no I/O or gating.
The full native event is retained. Complete Store-result observation survives
early control failure and every worker join; phase/observer errors cannot mask
original I/O. Native I/O/phase/cancel observations remain distinct and no
unobserved NN count is reported as measured zero.
