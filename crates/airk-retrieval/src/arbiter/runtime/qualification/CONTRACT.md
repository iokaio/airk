# Experimental selected profiles

This test-support Rust owner constructs Core `Profile` values from actual,
reviewed Core `Asset` identities and physical input files. It authenticates
lengths and SHA256 values with bounded streaming reads. It emits the actual
Rust compiler asset, calibration/relevance metadata, experimental descriptor
and `asset-files.private.json` overlay; weights are not copied or relabeled.

Only the selected Focus, ContextCompact and ContextLarge identities qualify
for this constructor. The caller explicitly supplies an approved `CpuFit` and
experiment memory/disk reservations. Focus uses the recorded CPU4 baseline.
These declarations are not measurements, deadlines or end-user hardware claims.
Native inference must measure the actual fit under existing service admission
and the unchanged combined operation deadline.

`Profile.policy` binds the current native compiler source policy. The task map
separately binds the current ContextAction semantic policy or Focus policy.
The private manifest includes the complete descriptor with a zeroed self field.
Legacy profile identities are unchanged. Native release, evaluation execution
and publication reject stale selected policies before opening model assets.

Focus metadata explicitly describes uncalibrated relative no/yes relevance;
it is neither an action model nor learned probability calibration. Context raw
score collection may use the native structural GenericRead calibration with
an extreme finite margin. This is not a fitted rule or positive CAL evidence.
Raw-score consumers never call `selected_action` or configure the service with
that numeric bootstrap descriptor. Supplied calibration is structurally
authenticated and remains unqualified; fitting and A6 acceptance need their
own genuine evidence.

Every output has no qualification, qualified targets or download authority.
This constructor cannot create a release capability or substitute metadata for
NN, calibration, platform, package or task-quality qualification. Publisher
notices are actual caller-supplied assets; missing provenance is never invented.
