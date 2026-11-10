# Replay process preflight: two-strike stop

The new replay process test has hit two T2 failures. The second identifies a
production ordinary-activation defect before the test reaches replay: a planned
candidate shutdown and immediate second probe compete with the same candidate's
automatic startup backoff. Stop this service campaign before a third expensive
attempt, preserve both fixtures, and complete the cheap correction cycle first.
The authorized repair task continues; this stop is not a new permission rule.

## Actual evidence

| Run | Observed result | Scope |
| --- | --- | --- |
| First process preflight | 49 of 52 selected ran: 48 passed, one failed, three unrun. The new replay case reached completed activation, then received `runtime_upgrade_in_progress` while obtaining an epoch. | Retained diagnosis and the subsequent deterministic actor T0 case explain a frozen metadata receive surviving release. Health alone was an insufficient fixture precondition. |
| `process-preflight-remainder-r1` | Four selected, one ran and failed in 20.75 seconds, zero passed, three unrun. Error: `on-demand restart backoff active; retry later`. | This is an earlier activation failure, not proof that completed public replay starts a stopped service. No replay or Stop success was reached. |

Receipts are under `artifacts/readiness-1817/`: the original preflight and
`replay-retained-{observation,diagnosis}.json`, the reviewed actor/observer T0
batch, and `process-preflight-remainder-r1.{json,log}`. The latter receipt records
unchanged consumed inputs and its complete selection. Keep the 48 earlier
passing results under their original identities; they do not qualify new source.

The second retained fixture is
`artifacts/readiness-1817/host-fixtures/.tmp2lI46Z/runtime-owner`.
Inspection used read-only escalation under the original owner's ACL. No ACL,
receipt, checkpoint, process or startup budget was changed and no service was
restarted to obtain evidence.

Its retained records identify the exact phase:

- Initial payload inventory: `516d5a914bd7073e2b8e578523740de21a30dd64500de81ef638b7598d2af9bc`.
- Candidate inventory: `92a7443b4e53bb7b914e42c214499ebdc328e9a059d55d36dd1c30570f7ea275`.
- Candidate runtime metadata declares schema 2, Windows x86_64, version 1.8.17,
  executable SHA-256 `467ba8722828b930443db6b9536d2e88e4d512fcd4acdf14dc5f436e20c327b8`.
  This is the retained metadata identity, not a new build or independent rehash.
- `activation.json` remains in `probe`, with the exact candidate and predecessor.
  `selected.json` already selects that candidate with the initial inventory as
  previous. `activation-result.json` still records only initial installation.
- The checkpoint operation is `1ddfa7c3bfce8499f9b0ef97c81b63b1`; its journal
  remains `activation_prepared`, revision 5. Its native recovery fence remains,
  with origin `installer_after_verified_exit` and no invented service instance.
- Candidate startup receipt: exactly one attempt at Unix ms **1794289182472**.
  Candidate instance `740a83dd333dbd5913d4763afe388c20` logs start at
  **1794289182821**, shutdown requested at **1794289183616**, and stopped at
  **1794289183627**. Bootstrap records ready at **1794289183165**. No second
  candidate start appears in those retained summary records.

These records and the source locate failure inside the first reviewed update.
They do not establish a completed update, released admission, successful replay,
or complete cleanup. Pending state and the failed fixture remain preserved.

## Root cause and wrong assumption

`runtime_install::startup_attempt` is the sole origin of the reported error.
It retains up to three attempts in 120 seconds and requires 2 seconds between
the first and second, then 4 seconds before the third. It preserves its circuit
on backwards clocks. Successful health clears attempts only after 60 seconds of
uptime. These limits are deliberate safety invariants, not test knobs.

The ordinary path in `activation::resume` currently does this:

1. Under installer ownership and native quiescence, validate the candidate's
   catalog compatibility and prepare its checkpoint/launcher boundary.
2. Probe/start the exact candidate; its on-demand start charges the circuit.
3. Publish the candidate selection after successful native health.
4. Call `catalog_upgrade::activate`. Its surviving ordinary path intentionally
   drains that same candidate, proves stopped-owner exclusion, validates the
   existing catalog and returns; it performs no in-place catalog upgrade.
5. Probe/start the same candidate again through `start_selected`, which enforces
   the same on-demand startup circuit.

If the successful first probe and planned drain take less than the circuit's
cooldown, step 5 refuses. It leaves committed selection plus a pending Probe
intent and recovery fence. There is no second service start to charge because
the refusal precedes spawn. The first failed run happened to complete this
sequence; that success did not establish freedom from its timing dependency.

The fixture helper starts and shuts down the *initial, different inventory*
once. It does not consume a candidate attempt. The failing candidate's repeated
starts come from the production ordinary-activation sequence itself.

The wrong assumption was that an actual approved ordinary update could be used
as a deterministic setup step without reviewing its intentional drain/reprobe
against the durable startup budget. The existing
`startup_circuit_retains_failed_attempts_across_reopen_and_backwards_clock`
regression correctly checks isolated budget boundaries, but does not compose
them with this real two-probe lifecycle. The initial integration diagnostic
also lacked phase context around its first update call, so retained journals
were needed to distinguish setup failure from later replay failure.

## Required cheap regression and correction constraints

Before another process campaign, add a deterministic seam around the actual
ordinary activation sequence. Inject the start/drain observations and its clock;
exercise a successful first candidate probe, an authenticated intentional drain,
real stopped-owner/catalog checks, and an immediate continuation. The old flow
must reproduce the refusal class without a native build/start or elapsed sleep.
Keep negative cases for absent/incompatible catalogs, foreign or changed
instance/runtime/operation, unknown drain outcome, held native owner, and a
resumed call that lacks same-call evidence. Compare exact receipts and failure
states; distinguish an actual reproduction from a passing hypothesis.

Any correction must preserve startup limits, failure-attempt history, backwards
clock behavior, managed ownership, selection/checkpoint identities, source
read-only behavior and resumable failure state. Do not reset startup receipts,
sleep, widen cooldowns or retry counts, or blanket-disable the circuit for
activation/resume. A narrowly owned one-use same-call proof of an authenticated
healthy intentionally drained instance plus quiescence is one possible design;
removing an unnecessary drain/reprobe while retaining its catalog proof is
another. Neither design has been implemented or qualified by this note.

## Sibling audit and remaining uncertainty

- **Launcher boundary:** it already holds real `Quiescence` and calls
  `require_catalog_compatible` before the first probe. Moving the remaining
  ordinary catalog-exists admission there could remove the unnecessary catalog
  stop/start, but saved `probe` intents must retain equivalent admission on
  resume; a new Launcher check alone is insufficient.
- **Directory migration:** `data_migration::needed` checks default installation
  identity and actual saved relocation/storage state, not just application
  version. A modern initialized default installation can enter that path by
  source inspection; it is not proven unreachable after the clean break.
  Uncommitted activation can probe, commit, drain for migration, then reprobe;
  committed resume can also intentionally drain before probing. Preserve the
  documented selected-runtime and relocation recovery order.
- **Rollback:** it verifies and drains the exact candidate or predecessor,
  proves quiescence, then starts the prior selected image with normal startup
  admission. A successful unit drain does not prove that a healthy instance was
  observed or that the current call owns a reusable startup authorization.
- **Health:** `health_identity` authenticates the exact payload and instance even
  with checkpoint-closed admission. `health` additionally requires accepting
  policy. Generic failed/nonaccepting health is not absence or restart proof.
  Security review confirms that `drain_identity` returns unit and loses whether
  it actually observed a live accepting instance or found the owner absent;
  its success alone must not mint a healthy planned-restart witness.
- **Managed owner:** `start_selected` dispatches through the registered manager
  when it owns restart. Keep that route; an on-demand correction must not start
  a competing detached child or assume OS restart ownership from a receipt.

The ordinary activation defect is reproduced by retained process evidence and
explained by source. The migration/rollback timing exposures are source-review
findings pending cheap cases; no platform or recovery qualification is inferred.
No production/test correction or additional build/test was performed for this
second failure. The primary agent must review a coherent correction and its
cheap evidence and bring this stop note to the user before a third T2/T3 service
attempt. Unaffected checks may continue with their own original input identities.

## Chosen correction and first cheap diagnostic step

The primary agent chose calculated scheduling at the existing admission boundary,
not a circuit bypass, reset, removed catalog proof or blind delay. Only the
explicit activation/probe/rollback route may await its existing 2/4-second lawful
admission time. Ordinary verified/on-demand callers keep immediate refusal.
The original three-attempt/120-second circuit remains terminal, as do malformed,
backwards-clock and unknown outcomes. A monotonic deadline must include waiting
inside the existing 60-second startup budget; it must not reset after waiting.
Wall and monotonic observations and the calculated wait are injected for tests.
The installer operation, root, control, approved runtime and manager ownership
must still match after waiting. No spawn may be repeated after an unknown result.

The first behavior-preserving diagnostic seam is now prepared, not yet run:
`observe_startup_admission` owns the actual existing health-to-receipt block.
Production still supplies `unix_ms` and its original immediate admission. The
setup cancellation CAS, absent/healthy distinction, receipt bytes and later
bootstrap/spawn sequence are unchanged. Five new named tests plus the existing
startup-circuit case form one batch. Two prospective scheduling regressions use
real receipts and paused/manual Tokio time and are expected to reproduce the
current cooldown failure; the other four cases preserve immediate ordinary
refusal, terminal guards, healthy no-effects and the existing cap/clock contract.
No new production scheduling policy has been enabled in this diagnostic step.
