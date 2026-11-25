# Focus and Arbiter

This guide describes the approved two-stage design in the current source
candidate. Native stage controls are implemented in source; model and platform
qualification remains pending. Use the running service's catalog and status to establish what your
installed version actually supports; this document does not make a model ready.

## What each model does

| Component | Model | Purpose |
| --- | --- | --- |
| Bundled retrieval | MiniLM | Creates document and question embeddings so AIRK can find candidates efficiently. |
| Focus | Qwen3-Reranker-0.6B | Scores the question and each retrieved passage together to improve the order of a bounded candidate set. |
| Arbiter | Qwen3-1.7B | Assesses the gathered evidence and selects justified additional reads within the original budget. It is the Context stage's one model. |

Focus can run independently while Context is off. When both stages are enabled
for an index, the intended flow is MiniLM candidate retrieval, Focus reranking,
then Arbiter assessment and bounded evidence gathering. Focus and Arbiter are
separate switches: Answer runs with neither, either or both. The Large Context
model (Qwen3-4B) was retired on 23 November 2026 (step LH): an installation that
had selected it finds the Context stage turned off, its index enrollments kept,
and a reason that asks to select Arbiter. Arbiter is never selected for it.

The current candidate retains the original MiniLM weights: the evaluated
replacement did not improve the fixed development tasks. Arbiter configuration
does not replace the retrieval encoder or change existing document vectors.

The MCP `answer` tool gathers a bounded packet of cited evidence for its caller.
The calling assistant uses that evidence to write the final natural-language
answer. Arbiter works within the caller's original read, token, byte and
deadline limits.

## Focus in Search

When Focus is enabled and the index is enrolled for it, an eligible hybrid or
vector Search can use the active Focus profile to rerank a bounded set of
retrieved hits. This works with Arbiter off. The remaining hits keep their native
order, and lexical-only Search keeps its native ranking. Check the live service's
stage status and Search response for the active profile and reranking receipt.

Focus can reorder only the candidates AIRK retrieved. It cannot recover an
important passage that candidate retrieval omitted. MiniLM recall and the native
retrieval recipe still matter. A relevance score is not proof that a passage is
correct, that all required evidence is present, or that no further read is useful.

Context can identify a missing definition, constraint, example, test or
counterevidence and choose among verified native reads. It cannot invent a path,
expand the caller's permissions, execute a command, modify source files or start
an autonomous agent loop. Its sufficiency assessment is fallible and relative to
the question and visible evidence.

Only decisions qualified for the installed profile are available. A profile
qualified to choose an additional read does not also establish reliable
sufficiency, clarification or semantic gap labels. Unsupported decisions retain
the native fallback and are reported in status and results.

## What you need

- A running, verified AIRK Service version that supports the stage and model
  profile. The VS Code extension is optional; CLI, MCP and browser clients use
  the same native service.
- Readable indexed evidence and an authorized index scope. Semantic candidate
  retrieval also needs the matching MiniLM model and published document vectors.
- Explicit stage configuration and enrollment of the intended immutable index
  identities. Enabling Focus does not enable Context or enroll every index.
- The complete authenticated assets for each selected stage, including its
  tokenizer, compiler/readout, calibration, qualification fixtures and notices.
  Downloaded weights alone are not an active profile.
- A qualified profile for the actual OS, architecture, backend and supported
  tasks, with enough available memory, disk and execution capacity at admission.

Both optional stages are off by default. A new, recreated or imported index does
not inherit another index's consent. Renaming an enrolled index preserves its
identity. A connection's read scope remains a separate authorization boundary;
stage enrollment does not grant an agent access to an index.

## Check support before enabling a stage

```console
airkctl arbiter --help
airkctl arbiter status
```

Inspect the actual catalog, configuration revision, task support, requirements
and per-stage availability. Use the current generated configuration schema and
command help to prepare changes; do not copy fields or flags from an older
version. A selected model is the user's intended configuration. An active model
has verified assets, supported execution and current admission. A visible or
selected entry may remain unavailable with a specific reason.

Review Focus and Context separately. Enable only the intended stage and enroll
only the indexes it may process. Context may use Focus only where Focus is
separately enabled and the index is enrolled for it. With Focus off, Context can
use the ordinary native candidate ordering. With Context off, Focus can still
help ordinary retrieval where supported.

The service installer includes bundled MiniLM, but no optional Focus or Context
weights. Optional model acquisition belongs to AIRK Service after you choose a
supported profile and explicitly provision it. Installing or updating AIRK does
not download those optional weights.

When a qualified profile is offered, its catalog identifies the canonical public
source for the exact compatible assets. A backup source is usable only if it has
been verified for those same assets; a backup is not promised for every profile.
A publisher's general model download is not automatically a compatible AIRK
profile. If no verified source is available, the profile remains unavailable;
AIRK does not silently choose different weights, precision or execution.

Model provisioning is an explicit management operation. A status query, ordinary
search or contextual question does not authorize a download. Review the selected
profile, transfer and staging/rollback disk requirements before provisioning.
Acceptance of a provisioning request is not completion; retain its request and
configuration identities and inspect its native outcome.

The current CLI exposes the following management operations. Replace the
uppercase placeholders with the actual observed revision, profile SHA-256 and
file paths; do not run these examples unchanged:

```console
airkctl arbiter initialize
airkctl arbiter configure --file "CONFIGURE_REQUEST_JSON"
airkctl arbiter provision --expected-revision REVISION --profile PROFILE_SHA256
airkctl arbiter import --expected-revision REVISION --profile PROFILE_SHA256 --directory "ORIGINAL_ABSOLUTE_BUNDLE_PATH"
airkctl arbiter cancel --expected-revision REVISION --stage focus
airkctl arbiter recover --expected-revision REVISION --stage context
airkctl arbiter remove --expected-revision REVISION --profile PROFILE_SHA256
```

Initialize creates disabled stage state. If retained legacy configuration exists,
use `initialize --expected-legacy-configuration SHA256` with its observed digest;
initialization does not inherit its consent. Configure requires the complete
Core-native Configure request, including the intended stage, rather than a
partial settings file. Cancel and recover accept `focus` or `context`.

Provision explicitly authorizes network acquisition. Import instead reads the
complete authenticated local bundle from its original absolute native path;
it does not download missing files or modify the source bundle. Local import
is an owner-only CLI control and is not exposed as a browser source-path input.
Recovery reconciles removal or activates an already verified cached bundle.
An incomplete acquisition requires a new explicit Provision or Import decision;
recover never silently converts local import authority into a network download.

## Hardware and latency

Focus is smaller than either Context model, but it is substantially larger than
MiniLM and still needs resource admission. It scores a question and passage
together for every candidate, so passage length and candidate count affect both
memory and latency. Context adds its own token, assessment and read work. The
combined profile must account for both stages, MiniLM and ordinary indexing.

The current Windows x64 CPU diagnostic candidates have these declared
reservations and frozen input limits. They remain separate from the profiles
actually offered by an installed catalog. Combined service, task-quality and
final profile qualification remain pending.

| Stage | Usable physical CPU cores | Model memory reservation | Maximum input |
| --- | --- | --- | --- |
| Focus | 4 | 4 GiB | 512 tokens, 4 candidates |
| Arbiter | 4 | 16 GiB | 8,192 tokens, 8 evidence cards |

These are model reservations, not whole-machine RAM recommendations. Both
resident stages need their separate reservations, plus capacity for MiniLM,
indexing, AIRK and other applications. Final installed catalog requirements take
precedence; these measurements do not establish support on another platform.

Current component measurements use 20 samples and the unchanged 120-second
limit. Focus records p95 2.116398 seconds; Arbiter records p95 86.049136 seconds
and passes its separate 17/128/8192-token numerical checks. These are component
results, not end-to-end Answer latency or task qualification. The retired Large
model was never qualified (its faster attention path exceeded the numerical-error
tolerance and its dense path the 120-second scoring deadline). No
precision or deadline fallback is qualified. Historical 2,048-token/30-second
results remain historical. See the [current outcomes](final-plan.md#10-current-execution-outcomes)
and [exact execution evidence](../simplify/one-process-ledger.md).

There is no universal RAM minimum, guaranteed CPU latency or assumed GPU support.
The service must report measured requirements for the qualified model precision,
backend, OS/architecture and input limits. A small model does not mean it fits every
machine, and installing a GPU does not establish a supported execution path.
Unavailable capacity is an explicit not-applied reason rather than permission to
switch models, precision or devices silently.

## Results, limits and fallback

In AIRK Workbench, choose **+ → Gather cited context…** to open the
question panel. Review or edit the question, then choose **Gather cited context**
to use the native default budget. Context controls and their explanation appear
only while this panel is open, leaving the normal composer compact.

The advanced **Adjust native budget** section starts collapsed. Open it to
review or change the JSON budget, then choose **Gather with this budget** to send
that explicit budget. The selected read scope still applies, and the normal
result, citation, copy and export views retain the complete response.

Canonical results retain original source locations, content hashes, publication
identity and read authority. Focus ordering and any selected or omitted subset
must be disclosed. A ranked excerpt is still indexed evidence, not a new source
fact. Complete supported evidence and provenance remain available through the
normal authorized inspection, copy and export paths.

Context returns organized cited evidence, its assessments and gaps, work used
and the stop reason. The MCP `answer` operation gathers this contextual packet;
`context` expands neighboring indexed lines around a specific location. The
calling assistant can use the packet in its response. Review the cited evidence
and qualifications rather than treating a model judgment as exhaustive proof.

If an optional stage is disabled, unsupported, missing assets or temporarily
unavailable, retain ordinary MiniLM/native retrieval with the explicit stage
application or refusal reason. Do not interpret fallback as a successful Focus
ranking or a successful Context assessment. Explicit vector/hybrid requests and
saved completeness requirements keep their normal refusal rules.

Deadlines, candidate/read/round/byte/token limits and index authorization do not
reset between stages or follow-up reads. Stale or revoked authority suppresses
delivery. Excluded indexes contribute no content to that stage. Turning both
optional stages off preserves ordinary retrieval behavior.

## Offline use, disablement and removal

After a supported profile's complete assets have been verified and provisioned,
inference is local inside AIRK Service. The product does not send indexed
questions or evidence to a cloud model, run a model server or execute code from
a model repository. Offline import must authenticate the same complete assets
as online provisioning. A separately obtained manual offline bundle is a
secondary option, not a required installer payload. Missing assets remain
unavailable when offline.

Disable each stage through its supported native controls. Disablement prevents
new optional work and suppresses stale delivery while already admitted work
drains; it is not proof that native execution stopped instantly. Remove a
profile only through the service's guarded operation after its pins and active
work are released. Preserve any previous qualified bundle required for rollback.
Do not delete live model directories manually.

After an interrupted or uncertain operation, inspect and recover the original
request using its actual configuration revision. Do not replay it under a fresh
identity merely because the client lost the reply. A restart, partial download
or downloaded checkpoint never manufactures readiness.

## Qualification and support

Model download, a successful export, a correct relevance ranking and a passing
renderer test prove different things. The selected native pipeline still needs
independent evidence-quality, safe sufficiency/abstention, bounded gathering,
goal/follow-up, recovery, resource, platform and package qualification. Help
reports available controls and requirements; it does not certify those results.

When reporting a problem, retain the service/profile versions, selected versus
active stage, the explicit unavailable or stop reason and native request/receipt
identity. Review excerpts, paths and logs before sharing them. Do not post
credentials, sign-in URLs or private source text.
