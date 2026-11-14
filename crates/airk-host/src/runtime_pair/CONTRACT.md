# Native runtime identities and executable metadata

Owner: `airk-host::runtime_pair`. The installer proves artifact authenticity;
this adapter validates bounded local metadata and the executable bytes its
callers require. It starts no process and loads no model.

Schema 1 names a service image and its predecessor inference helper. Schema 2
names one in-process service image. Schema 3 uses the existing full C7 metadata
owned by `airk_core::runtime_metadata::SplitRuntimeMetadata`: service and CLI
digests, declared I1/I2 versions and Admin inventory digest remain intact.
The CLI is not an inference helper: schemas 2 and 3 both select an in-process
service engine, reject retired stdio/helper entry points and create no role
copies. Missing metadata preserves the existing uninstalled-development path.

The public `RuntimePair` retains predecessor fields and a typed optional full
`split` record. Schema 3 serialization/target validation requires those common
fields to agree exactly with the full record; a lossy or crossed view refuses.
Schema 1/2 serialization remains byte-identical in its original field order and
helper omission rule. Strict predecessor decoding still rejects unknown keys,
null/extra helper values and missing helper identity. Strict Core schema 3
decoding refuses missing, duplicate or unknown keys and invalid claims.

`read_installed_pair` is structural only. It validates metadata and explicit
approved version/host target, but performs no hidden executable hashes and
does not authorize execution. Installer inventory passes retain their own
counted image verification. `single_executable` observes the metadata's engine
kind only; it is not artifact authentication or byte qualification.

`verify_installed_pair` verifies the actual service and, for schema 1, helper;
schema 3 additionally verifies the actual CLI at the Core-owned payload path.
`engine_identity_at_version` retains its explicitly approved predecessor
version behavior and binds the actual service hash; for schema 3 it also
verifies the actual CLI. Maintenance identity retains domain
`airk-maintenance-engine/1.4` and frames canonical complete metadata. Changing
schema-3 CLI, declared I1/I2 or Admin inventory changes that identity; schema
1/2 identities keep their original exact encoding. The metadata's Admin digest
is bound, not independently compared with compiled Admin bytes here; native
build/install qualification owns that check. Interface claims are not evidence
that a client supports those versions.

All metadata readers keep the existing 4096-byte ceiling and link/reparse
refusals. Executable hashing is streamed and bounded. Platform signatures,
ownership, inventory approval, permissions and later source mutations remain
the applicable caller's responsibilities; digests alone grant no authority.

Focused tests: `cargo nextest run --profile ci -p airk-host --no-default-features --lib --locked --timings -E 'test(runtime_pair::)'`.
Run applicable installer/checkpoint consumers after changing shared metadata.
Tests retain independently specified predecessor framing and exercise inert
schema-3 service/CLI files, complete identity, target/schema refusal and changed
or missing CLI bytes without executing a process. Each run must select nonzero
tests and report its count. This does not establish container/runtime execution
or platform qualification.
