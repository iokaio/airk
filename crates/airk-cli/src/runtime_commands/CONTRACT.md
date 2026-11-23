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
are removed with enrollment. `http-endpoint`, `http-trust` and
`http-issuer-rotation` are unchanged (TLS is phase C).
`lookup-http-connection` now requires `--purpose predecessor-cleanup` and
`--predecessor`: it is the installer's predecessor-cleanup compatibility row
(`runtime_install::http_cleanup::lookup`), which reads the retained
credential ledger read-only and never treats a revoked or unavailable id as
foreign. Tests: installer
`clean_break_http_disposal_cli_requires_explicit_purpose_and_predecessor`
and airk-cli `native_http_endpoint_remains_and_connection_commands_are_removed`.

`airk setup` no longer enrolls a client: it installs a reviewed payload
(`--runtime-source`, `--approved-sha256`) and names `airkctl agents connect`
as the next step. Its client, connection, scope, session and launch inputs
no longer parse (`setup_no_longer_accepts_enrollment_scope_session_or_launch_inputs`).
