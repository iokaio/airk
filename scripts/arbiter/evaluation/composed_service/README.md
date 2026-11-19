# Composed-service operational adapter

This directory owns the current adaptation of the preserved composed runner and
its exact current-policy helper family. Read [CONTRACT.md](CONTRACT.md) and the
[historical source mapping](../research_history/SECTION7.md) before use.

Preparation authenticates a reviewed private orchestration plan and native-emitted
inputs against maintained source. It does not execute the historical program,
run a model, assemble a passing receipt or qualify any artifact. An actual run
still requires the reviewed multiprocess-owner integration, complete private
fixture/profile/request seals and current compiled SDK/native images.

Python and Node source controls run without native setup or model imports.
Keep private inputs, plans and observations under an explicit guarded artifacts
root. Existing results retain their original identities; never replace their
source hashes with this adapter's hashes.

## Read-only preparation

Run `python -B -S scripts/arbiter/evaluation/composed_service/prepare.py --plan <absolute-private-plan.json> --plan-sha256 <reviewed-sha256>`
in the verified human shell. The supplied plan must already name the current
maintained sibling closure and contain explicit native, SDK, fixture, request,
policy and private output identities. This adapter refuses an unchanged
historical plan; it never rewrites old evidence or derives permission from a
source-rescue map. The stdout result remains `execution_authorized: false`
until a separately reviewed outer execution owner is available.

Pure checks: `python -B -S scripts/arbiter/evaluation/composed_service/test_adapter.py`.
Ten test groups include actual Node execution of inert helper assertions only;
no SDK/native session, process owner, model or private fixture is executed.

`run_composed.run` is a worker interface for that future reviewed owner. Its
required boundary supplies the original absolute monotonic deadline,
`before_dispatch`, `audit_paths` and `after_observations`; these are obligations,
not a synthetic replacement for OS process custody. Direct worker and session
helper CLIs refuse. The current adapter is source-tested, not runnable or
qualified as an integrated composed campaign yet.
