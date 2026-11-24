# Runtime command adapter execution

Owner: `airk-cli::runtime_commands`. Native installation/managed metadata remains owned by airk-installer; resident startup remains owned by airk-service. This adapter does not create an alternative lifecycle, installer format or source-selection authority.

- Runtime metadata observations authenticate the existing installation, runtime digest and exact control root. SourceScope does not start/elect a service, settle pending activation/Stop, create missing registry state or change provider bytes.
- The public captured dispatcher constructs its heap-pinned command future in a synchronous owning factory. Native resident and managed-service futures use the same separate construction boundary only when their branch executes. Callers retain only the heap handle; common debug poll frames do not construct/copy unrelated native startup state. Constructing/dropping an unpolled command remains lazy and creates no roots. This introduces no new process, authority or cancellation policy.
- The ordinary Windows test runtime is a required boundary for metadata callers; tests must not increase thread stacks to bypass dispatcher polling/call-nesting overflow. A no-execution diagnostic measures public dispatcher future inline state against an independent64KiB regression ceiling (1/32 of the ordinary2MiB harness stack). This size ceiling alone is not proof of safe polling: the original real metadata/refusal tests must also pass with unchanged filesystem and no-start assertions.
- The reviewed fixed `service-login-current` role intentionally parses with no arguments. Platform, current I4, executing image/C10 and automatic-start authority remain execution checks. Every other reviewed served role's missing syntax arguments still refuse; unknown/pending role behavior is unchanged.

Primary validation selects `runtime_commands::connection_observation_tests`, `runtime_commands::managed_service::tests` and `role_admission::tests` with the affected feature graph. Retain actual before/after inline sizes and the original managed-mode stack overflow reproduction. Compiler/metadata tests do not qualify an installed service or an OS login journey.

Observed before repair: public run/run_with inline state was54,920/54,896 bytes, already below64KiB, while the original managed metadata test still overflowed the ordinary Windows stack. Direct Box::pin at resident await sites also failed that original test. Therefore the type-size ceiling alone neither reproduces nor proves the fix; separate synchronous construction boundaries and the actual original ordinary-stack metadata regressions are required evidence.

## HTTP connection commands removed (23 November 2026, local trust plan B, change-log entry 68)

`airk runtime http-connection`, `http-connection-info`, `http-credential`,
`http-revoke`, `http-legacy-connection` and `http-legacy-union-connection`
are removed with enrollment. `http-endpoint` remains. Local trust plan C
(23 November 2026) removed `http-trust` and `http-issuer-rotation` with
loopback TLS, and with them every `require_issuer_rotation_settled` guard of
the installer commands.
`lookup-http-connection` now requires `--purpose predecessor-cleanup` and
`--predecessor`: it is the installer's predecessor-cleanup compatibility row
(`runtime_install::http_cleanup::lookup`); since part 4 it reads no ledger
(see below). Tests: installer
`clean_break_http_disposal_cli_requires_explicit_purpose_and_predecessor`
and airk-cli `native_http_endpoint_remains_and_connection_commands_are_removed`.

`airk setup` no longer enrolls a client: it installs a reviewed payload
(`--runtime-source`, `--approved-sha256`) and names `airkctl agents connect`
as the next step. Its client, connection, scope, session and launch inputs
no longer parse (`setup_no_longer_accepts_enrollment_scope_session_or_launch_inputs`).

### Ledger removed; ownership by shape (B, part 4)

The user decided on 23 November 2026 that the credential ledger is removed on
upgrade, not kept: the service deletes `http-1/credentials.json`, the
`credential-<id>.token` files and the folder if it is then empty
(`airk_endpoint::http_endpoint::retired_credentials::remove`, exact names in
its own state root, links never followed). `RetainedLedger`,
`holds_no_credentials` and the captured ledger fixture are removed.

Predecessor cleanup therefore reads no ledger. A predecessor's agent-config
entries are recognised by their known shapes: the `Bearer`, `headersHelper`,
`http_headers_helper` and `bearer_token_env_var` forms, which name the
connection id (`airkctl` `agents::config::recognize`, and the extension's
removal-only recognizer). The lookup answers, for the connection id such an
entry names, whether the predecessor is identified: `owned` when its endpoint
metadata is complete and no newer installation can claim it, otherwise
`unavailable`. A newer installation in its own namespace issues no connection,
but if it serves the predecessor's exact endpoint URL an entry there may be its
own, so the lookup is `unavailable` (`current_namespace_ownership`). The Core
DTO (`service_http_cleanup`) drops the ledger-derived fields: the snapshot's
`owners`, and the lookup's `ownership`, `credential_file`, `credential_sha256`
and `credential_digest_algorithm`; `not_owned` is no longer a lookup status.
A consumer still removes only an AIRK-shaped entry whose URL is the
predecessor's `endpoint.url`, and never one it does not recognise.

Tests: installer
`clean_break_native_http_lookup_identifies_the_predecessor_without_a_ledger`
(with and without a retained ledger, left byte-identical),
`clean_break_http_snapshot_survives_headless_old_control_and_payload_retirement`
(a newer installation on the same URL is `unavailable`, on another port
`owned`), `clean_break_native_http_lookup_refuses_missing_or_foreign_metadata`;
`airkctl` `agents::config::tests` `legacy_*` (each old shape repaired, foreign
servers byte-identical); endpoint `retired_credentials::tests`.