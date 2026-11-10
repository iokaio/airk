# Service-client runtime classification correction

Owner: service-client harness task; 9 November 2026. This is a release-readiness
correction, not a structural step or installed qualification result.

## Source review and hypothesis

`scripts/check-service-clients.py::classify_runtime` admits schema-2 executable
metadata only at the literal version `1.8.14`. The runtime builder has used
`single_image_version` (`>= 1.8.14`) to emit that topology, including the current
working version. `Fixture.__init__` classifies before opening its stderr file or
starting a process, so the stale literal prevents the CI service-client campaign
and shared fixture callers from reaching native staging.

The classifier chooses executable topology, transport and catalog fixture setup.
It does not approve persisted state or establish artifact authenticity. Managed
startup still stages the complete manifest with its approved digest, activates
through native admission, and verifies the selected runtime and executable
hashes. State-contract, downgrade, lifecycle and cleanup policy remain native.
Schema-1 predecessors keep their exact reviewed manifest/source identity pins;
their version labels alone never establish admission.

The correction must use the builder's single-image version policy after strict
canonical numeric `major.minor.patch` validation, retain the exact schema-2
metadata key set, supported OS/architecture values and lowercase SHA-256 form,
and reject non-object metadata and non-integer schemas before fixture effects.
Classification can inspect another supported platform; native target validation
remains authoritative before execution. No registry version is inferred from
executable packaging schema.

## Batched regressions

Independent expected classification covers the first single-image version,
subsequent patches, the clean-break baseline and later numeric minor/major
versions supported by the builder's topology policy. Sibling refusal cases cover
pre-boundary and malformed versions, metadata containers, schema type/value,
unsupported platforms, invalid digests, missing fields and unexpected fields.
Existing reviewed predecessor/source and managed-startup tests stay in the
batch as compatibility evidence.

T0 command: `python -m unittest discover -s scripts/tests -p test_service_clients.py -v`.
The first diagnostic run precedes the production correction. Results and the
subsequent frozen-input validation will be recorded below. No Cargo or expensive
native campaign is part of this component task.

## Actual reproduction and correction review

The initial diagnostic selected 47 tests and reported 35 subtest errors. A
review of the test inputs found the malformed-schema cases were masked by the
same stale version literal. The observation was tightened to use the admitted
`1.8.14` label for refusal cases and retain each case in its own temporary root.
The complete retained diagnostic (`artifacts/readiness-service-clients/diagnostic.txt`)
selected 47 tests in 0.784 seconds and reported 35 subtest errors and one failure:
30 errors from later versions, four non-object metadata errors, a boolean schema
misclassified as schema 1, and acceptance of floating-point schema `2.0`.
Existing predecessor and managed-startup tests passed. These are actual cheap
reproductions, not installed qualification.

The correction rejects non-object metadata and non-integer schemas, parses only
canonical ASCII numeric versions, and delegates the topology boundary to the
existing runtime builder. The exact metadata fields, supported target names,
digest checks, returned classification fields, schema-1 receipt/source pins,
managed startup and cleanup paths remain intact. Review checked that invalid
classification still precedes log creation and process execution. Only harness,
regression and ledger inputs changed; production runtime inputs did not.

## Qualification result

The reviewed correction passed the same T0 selection: **47 tests passed in
0.875 seconds**, with the complete output retained at
`artifacts/readiness-service-clients/validation.txt`. `git diff --check` passed
for the component diff. This validates classification and mocked fixture
boundaries, including exact predecessor/source admission and the existing
startup matrix. It does not qualify native execution, installation, other
platforms or a final VSIX. Those gates belong to the primary release-readiness
ledger and require identified runtime artifacts.
