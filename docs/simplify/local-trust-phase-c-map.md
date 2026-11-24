# Local trust plan, phase C: implementation map

Read-only map made on 23 November 2026 from `feat/arbiter-final` at
`e2ca0e47`, for phase C of the [local trust plan](local-trust-plan.md):
plain HTTP on loopback. Line numbers are as of that commit. Phase B's grant
removal, still in progress when this was written, also touches the endpoint,
installer and cleanup code, so re-check those sites after it lands. Items
marked *(inf)* are inferences, not verified in source.

## Rust

| Site | Now | Change |
| --- | --- | --- |
| `airk-core/src/service_records/mod.rs:207-262` (`LoopbackMcpUrl`) | prints and parses only `https://127.0.0.1:<port>/mcp` | `http://`; `https` only through a legacy decoder |
| `airk-core/src/service_http.rs:36-37` | descriptor has `ca_sha256`, `certificate_sha256`; `deny_unknown_fields` | remove both; tolerant legacy decode |
| `airk-endpoint` `metadata.rs:68`, `http_endpoint.rs:18-22,40` | generation check compares `ca_sha256`; mounts `tls`, `trust` | drop the compare and the mounts |
| `airk-endpoint` `tls.rs`, `rotation.rs`, `trust.rs` and their tests | CA and leaf creation, issuer rotation, trust receipts | remove; keep a small remove-only legacy retirer |
| `airk-process/src/cert_trust.rs` | Windows PowerShell (select by DER SHA-256), macOS `security`; `rustls` PEM parsing | remove-only plus receipt read; drop `rustls` |
| `airk-https/src/http_gateway.rs` (TLS acceptor, 32 handshakes, 15 s accept) | TLS listener | plain TCP; hyper's 15 s `header_read_timeout` stays |
| `http_gateway.rs:975-1070` `FencedIo`, `FlushTrackedIo` | under TLS | keep, re-stacked under TCP |
| `airk-https/src/runtime_http_commands.rs:80-172` | `trust()`, CA checks | remove |
| `airk-https` `admin_plane` `https.trust_status`, rotation refusal arm | Admin trust row | remove |
| `airk-endpoint/src/admin_contract/mod.rs:84-97` | `https://` origin and launch URL | `http://` |
| `airk-mcp/src/remote/http.rs:478-482` | origin must be `https://{authority}` | `http://` |
| `airk-service` `service_runtime/http.rs` | `TlsIdentity::load_or_create`, leaf renewal | remove TLS |
| `service_runtime.rs:336-360,1175,1728`, `service_runtime/rotation.rs`; core `service.rs:589,840,1185,1756,1761` | `admin_trust_inspect`, `HttpIssuerRotation` | remove |
| `airk-service/src/setup_role` (`plan.rs`, `mod.rs`, `effects.rs`) | `ApplyTrust` after `Start`, `RemoveTrust` on uninstall | remove `ApplyTrust`; one legacy retire step |
| `airk-core/src/install_record/mod.rs:218-236,313-320,538` | `TrustChoice`, `SetupOptions.trust`, `SetupPhase::Trust*` | drop (no backward compatibility owed); change the three installer wrappers together |
| `airk-installer` `runtime_install.rs`, `http_rotation.rs`, `issuer_lifecycle.rs`; `require_issuer_rotation_settled` (about 15 callers) | rotation and trust commands | remove with all guard calls |
| `airk-installer` `replacement.rs`, `managed.rs:522`, `retirement.rs:749` | `ClientReferences.ca_sha256`, trust removal through `trust::remove_owned` | stop writing; repoint to the legacy retirer |
| `airk-installer` `http_cleanup.rs`; core `service_http_cleanup.rs:46,128-134` | predecessor snapshot embeds the old descriptor | re-check after phase B |
| `airk-cli/src/runtime_commands.rs:36-62` | `HttpIssuerRotation`, `HttpTrust` | remove |
| `airkctl` `cli.rs`, `trust.rs`, `rotation.rs` | `service trust`, `service issuer-rotation` | remove; regenerate the CLI reference |
| `airkctl` `admin.rs:20`, `agents/config` (`recognize.rs:111-125`, `classify.rs:199-218`, `manage.rs:168`), `contract/agent_entry.rs:37-47` | admin URL `https`; recognizer accepts only `https`; `HttpsTrustMissing` | admin URL `http`; recognizer accepts both, `https` is `HttpsEndpointMismatch` and repaired; remove `HttpsTrustMissing` |
| core `agent_manual*`, `toml.rs:858`, `edit.rs` | URL from the endpoint | follows `LoopbackMcpUrl` |
| test TLS clients: `airk-cli/tests/common/http.rs`, `tests/admin_plane.rs`, `tests/common/parity.rs`, `tests/tools.rs` | rustls clients, trust tests | plain HTTP; delete the trust tests |

Kept: `airk-service` `arbiter/download.rs:193-205` (outbound model downloads:
`rustls`, `tokio-rustls`, `webpki-roots`, `ring`).

## Dependencies

- `rcgen`, `x509-parser`: only `airk-endpoint`; remove from the workspace.
- `rustls`, `tokio-rustls`: remove from `airk-endpoint`, `airk-https`,
  `airk-process`, `airk-installer` and the `airk-cli` tests. They are also
  declared without use in `airk-catalog-control`, `airk-frontend`,
  `airk-host`, `airk-inference` and `airk-supervisor` *(inf: a compile
  confirms)*.
- Afterwards only `airk-service` keeps them. Re-check `Cargo.lock`,
  `THIRD_PARTY_NOTICES.md` and `supply-chain/`.

## Delivery fences

Now: hyper → `FlushTrackedIo` → rustls → `FencedIo` → socket. After:
`FlushTrackedIo<FencedIo<TcpStream>>`. The four tests in
`http_gateway/tests.rs` that reach into rustls buffers (about `:489`,
`:653`, `:1365`, `:1878`) are rewritten over a duplex counting writer. They
assert that no byte reaches the socket after a fence, a deadline or a
cancellation. The TLS-only tests (`tls_read_driven_alert`, the
`read_driven` and `below_tls` branches) are deleted.

## Admin cookie

- **Code:** `admin_contract/mod.rs:27,66-69,96,693-701`; gateway
  `http_gateway.rs:442-451`; Admin plane `admin_plane/mod.rs:495,577`.
- **New value:** `airk_admin`, `HttpOnly; SameSite=Strict; Path=/`.
- **Tests and fixtures:** `airk-cli/tests/admin_plane.rs:397,788`;
  `admin_contract/tests.rs:284-291`; `fixtures/split/i3/headers.json`,
  `admission_cases.json` (from `admission_cases.py`), `launch_exchange.json`.
- **Docs:** `admin_contract/CONTRACT.md`, `docs/admin-pages.md`.
- **Known limit:** cookies are not port-scoped, so another local HTTP service
  on 127.0.0.1 could read or overwrite `airk_admin`. `HttpOnly` and the
  server-side session binding limit that.

## Migration of old installs

- **Endpoint record:** `<control>/http-1/endpoint.json`. The current code
  fails hard on an old record. `load_descriptor` gets a legacy arm (port,
  generation, bytes digest). `Owner::start` reuses the port and publishes the
  next generation, and `publish_descriptor` accepts the legacy predecessor.
- **Agent configs:** an `https://127.0.0.1:<port>/mcp` entry is still
  recognised as AIRK's, classified `HttpsEndpointMismatch`, and rewritten by
  `agents repair` / Reconnect. The same repair strips old `headersHelper`
  and bearer entries.
- **VS Code:** accept both schemes in `recognizer.ts`, `windowAgents.ts`,
  `agents/index.ts`, `serviceHttp.ts`, `agentConfigHealth.ts` (drop
  `issuerTrusted`), `projectStatus.tsx` and `agentMigration.ts`;
  `adminOpener.ts` accepts `http:` only. Retire the trust and rotation
  commands in `explorerCommands.ts` and `adminUiContract.ts`, and the CA
  artefacts in `agentReconnect.ts`.
- **CA removal on upgrade:**
  - **Trigger:** the receipt `<control>/http-1/user-trust-1.json` in phase
    `Installed` triggers removal by its exact DER SHA-256. `Prepared` and
    `RemovePrepared` are reported as residue.
  - **Windows:** CurrentUser\Root through PowerShell.
  - **macOS:** `delete-certificate -t -Z <sha256>` on the login keychain. It
    may prompt, so it runs in the setup upgrade step, in the user session, not
    in the headless service.
  - **Linux:** no trust was ever installed there; only files are deleted.
  - **Files deleted afterwards:** `ca.pem`, `tls-identity.json`,
    `retired-private-key.json`, `user-trust-1.*`, `issuer-rotation.*` and
    `rotation-user-trust-preimage.json`. A pending rotation journal is
    discarded.

## Other owners

- **Admin:**
  - `agents.tsx`, `operations.ts` and `pages.tsx`: remove the trust card, the
    CA placeholder, `NODE_EXTRA_CA_CERTS`, `CODEX_CA_CERTIFICATE` and the
    trust steps;
  - snippets become `http://`;
  - tests `agentsSnippets`, `serviceSetup.dom`, `r23ShellLiterals` and
    `packages/ui/test/outerManagement.dom.test.mjs`;
  - rebuild `crates/airk-cli/admin-assets/`.
- **Python:**
  - `http_fixture.py` (`McpClient` on `HTTPConnection`, no CA or leaf pins);
  - delete `check-issuer-rotation.py` and its test;
  - `check-http-port-relocation.py`, `check-raw-https-clients.py`,
    `check-container.py` (it embeds `http_fixture.py`), `setup-local-test.py`
    (smoke test), `local-build.py` and `container-inputs.json`;
  - their tests.
- **`examples/`:** `local_https_client.{py,cjs,ps1}` and `examples/mcp/README.md`
  move to plain HTTP. A rename ripples to the docs and tests.
- **Installers:** `airk-service.iss`, `linux/install.sh.in`, `macos/Install
  AIRK.command.in`, their READMEs and `installers/CONTRACT.md`: remove
  `/TRUST`.
- **Fixtures and generated files:**
  - `fixtures/split/i1/control_methods.json`, `i3/admin_operations.json`,
    `i3/admission_cases.py`, `i2/agent_manual.json`,
    `agents/config_cases.json`, `cli-golden/windows/airk.json`,
    `parity/airkctl_trust_and_issuer_rotation_match_airk.json` and
    `i4/interruptions.json`;
  - regenerate `docs/cli-reference.md`, the help content and manifest, and the
    I2 and I3 TypeScript with their generators;
  - help text sources: `airk-mcp/src/help/topics.rs`.
- **Docs:** `local-https-api.md` becomes the local HTTP API;
  `agent-integration.md`, `one-airk.md`, `service-installation.md`,
  `vscode-extension.md`, `README.md`, `packages/vscode/README.md`,
  `admin-pages.md`, `cli-developer-guide.md`, `mcp.md`; the CONTRACTs for
  `setup_role`, `admin_plane`, `admin_contract`, `remote`, `installers`,
  `runtime_commands` and `agents/config`. `docs/split/*` and
  `docs/releases/*` are dated history and stay.

## Order

1. Legacy retirer and tolerant legacy readers (additive).
2. Remove the consumers: trust and rotation commands, installer rotation and
   guards, `HttpIssuerRotation`, the Admin rows, the setup steps, the VS Code
   commands; regenerate the I1 and I3 fixtures, the CLI reference and help.
3. One atomic compile: descriptor fields, `LoopbackMcpUrl`, `tls.rs` and
   `trust.rs` removed, gateway on plain TCP, service owner, cookie and
   origin, dependencies; rewritten gateway fence tests and the HTTP test
   helper.
4. Clients: airkctl recognizer and admin URL, manual snippets, VS Code,
   Admin UI and assets, I2 and I3 TypeScript.
5. Scripts, examples, installers, docs.
6. Upgrade test: an old `https` install fixture (descriptor, `Installed`
   receipt, `ca.pem`) with an injected trust runner. It shows removal by
   digest, TLS files deleted, the same port serving `http`, and agent configs
   repaired.
