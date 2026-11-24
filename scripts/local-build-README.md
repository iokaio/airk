# AIRK {{VERSION}} local build ({{PLATFORM}})

Built {{BUILT}} from commit `{{COMMIT}}`{{DIRTY}} with the `{{PROFILE}}` profile.

Building this folder changes no PATH, shell profile, login startup, default per-user AIRK installation or VS Code extension. This is a development candidate for this platform, not a published release. The Mac builder applies a development app signature; that does not establish Developer ID, notarization or other-platform qualification.

## Rebuild from the checkout

The default entrypoint verifies previously prepared pinned model/static-ORT inputs without downloading them. From the original checkout, run `python scripts/local-build.py --profile debug` (or select `release`). Add `--static-environment-file <complete-verified-handoff.json>` to use an existing host-matched static build, or `--static-ort <absolute-owned-static-destination>` to select its persistent cache. The handoff is reverified before its five compiler variables enter the build child; it does not change your shell's environment.

For a new cache, `python scripts/local-build.py --profile release --download-inputs` explicitly permits the pinned model, tool and ORT source downloads plus the bounded two-job static C++ build. It does not install tools globally. An existing invalid cache is refused rather than replaced. Missing offline inputs fail before Cargo. `build.ps1` and `build.sh` forward the profile and retain that offline default; use the Python entrypoint for these input options.

## Contents

| Path | Purpose |
|---|---|
| `{{BIN}}` | The single main image, including the pinned offline MiniLM model and statically linked CPU ORT. |
| `runtime/` | Complete sealed payload, native inventory, build/input receipts and license notices. Keep these files together. |
| `tools/check-service-clients.py` | Explicit disposable canonical experiment using the actual native setup, indexing, loopback HTTP and lifecycle paths. |
| `tools/input-manifest.json` | Exact hashes and original public paths of copied support files. |
| `examples/local_http_client.py` | Standalone standard-library JSON HTTP API client. |

The service owns its in-process inference provider. Source and project workers use authenticated role commands in the same main image. Agent connections use the shared loopback HTTP endpoint; they do not launch a helper, launcher or per-connection AIRK frontend. Do not start private worker roles yourself.

## Run the owned experiment

From this folder, run Python 3.11 or later (`python3` where needed):

```text
python tools/check-service-clients.py --runtime-dir runtime --files 1 --report experiment.json
```

This is a fixture-only experiment, not a way to register an arbitrary folder or connect an existing user account. It creates original throwaway source and separate private install/state/data roots. It approves this exact payload digest in that owned installation, activates native fresh catalog initialization, applies native-reviewed Index setup and exact Project membership/grant requests, indexes once and exercises complete CLI/HTTP results. It then performs the exact instance-bound managed Stop and removes its owned temporary roots only after settlement. Failures retain their source, state and diagnostics; the script prints that path. It does not configure login startup.

Add `--model` for the separately bounded actual embedded inference/vector checks. The complete model inventory and actual embedded-model smoke already ran in the native builder; an experiment report records what this additional run actually observed.

The copied dependency closure contains only public Python sources and the pinned model catalog. It runs without the original checkout or Cargo. Its supporting modules are not a new public administrative API. To experiment with different source/scope, use native Index setup preview/apply and explicit reviewed memberships/grants; a folder path, `init`, raw registration or a URL does not provide agent authority.

## Use the local developer API

The local endpoint is plain HTTP on 127.0.0.1 and takes no token (U110). The examples read the owner-private `http-1/endpoint.json` that a running native service publishes in its control directory; `airkctl agents endpoint --output json` prints the same endpoint record. Every read names its scope: an index or index set (id or name), or an absolute workspace folder whose Explorer-applied default applies.

```text
python examples/local_http_client.py --control-dir <absolute-service-control-dir>
python examples/local_http_client.py --control-dir <absolute-service-control-dir> --tool search --arguments "<JSON argument object matching the returned schema>" --index <index-id-or-name>
```

The first command calls `GET /api/v1/tools` and prints complete schemas. The second calls `POST /api/v1/tools/search` with `index_ids` added and prints the complete tool result; `--set <id-or-name>` adds `index_set_ids` and `--workspace <absolute-folder>` uses that folder's stored default. A read that names no scope is refused `scope_required`. Use the actual advertised tool name and argument object. The example connects only to the literal `127.0.0.1` URL in the endpoint record, uses no proxy and follows no redirects. The raw API needs no MCP SDK or Workbench. A missing or not-ready endpoint must be resolved through the native service.

The one-shot experiment above stops its service before returning; its earlier URL is not a persistent endpoint for these commands. A live custom experiment must keep its explicit reviewed native owner running; the examples reread the published endpoint after restart or relocation. Use the actual published port rather than assuming the preferred port is free.

## Inspect and clean up

`runtime/airk-install.json` describes the payload and `runtime/airk-build.json` preserves the actual profile, source seal, static-input identities, import inspection and embedded-model smoke. `static-environment.json`, when generated locally, is the verified compiler-input handoff, not a client credential.

Read complete `experiment.json` before claiming an experiment passed. It distinguishes native managed Stop evidence from a process exit status. Do not delete a retained failed fixture while native lifecycle settlement is unknown. This folder itself contains build artifacts and public tools only; deleting it does not uninstall an unrelated AIRK installation. The fixture recipe qualifies only what it actually ran, not reboot/logon, external agent trust or a published release.
