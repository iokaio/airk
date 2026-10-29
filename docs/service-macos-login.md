# Mac service bundle and optional login

1.4 development contract. The adapter and portable ownership/package checks are implemented; native framework compilation, signing, login, upgrade and installed-editor qualification remain pending on both Mac architectures. Windows tests do not qualify native Mac behavior.

## One bundle, two executables

The Mac payload places the existing lightweight `airk` and inference-only `airk-embed` in `AIRK.app/Contents/MacOS`. The Objective-C Service Management bridge compiles into the existing executable. It adds no third executable or model-bearing client process.

`Contents/Info.plist` identifies the GUI-less app. Two static files under `Contents/Library/LaunchAgents` select stable/preview login roles through `BundleProgram`. They contain no workspace, index, install-root or user-home paths. The role derives its installation from the exact approved payload location and validates the saved channel and active startup candidate before serving.

The paired hashes, build receipt, installer inventory and dependency notice inventory remain outside the signed app. This avoids a signature/hash cycle. Starting with 1.6, the pinned inference library is also in `Contents/MacOS`; its license and third-party notice text belong in `Contents/Resources/onnxruntime`, since Apple treats files in `Contents/MacOS` as code. Staging, native packaging, VSIX packaging and release verification share this layout and verify the notice hashes. The native builder fixes `MACOSX_DEPLOYMENT_TARGET=13.3`, creates an ad-hoc development signature, then hashes the final bytes. An ad-hoc signature is not Developer ID, notarization or independent distribution approval.

Engine identity resolves that same external pair receipt, so maintenance/model ownership includes the expected helper digest even when the main sits inside the app. Portable bare/app-layout tests verify that changing the helper changes engine identity and a mismatched main is refused. These checks do not execute Apple frameworks.

Explicit release signing signs the two executables, inference library and enclosing app before sealing final hashes. Explicit notarization submits the app, staples and validates its ticket, and inventories the ticket with the payload. Archives preserve both executable modes and all approved signature/ticket bytes. Native extraction/offline Gatekeeper tests must still verify that path. No quarantine removal or policy bypass is part of installation.

## Captured registration ownership

The selected approved app executes the fixed native `SMAppService` operation from inside its own bundle. The bridge checks the actual `NSBundle` URL, app identifier and fixed plist name. It never bootstraps, kicks or removes an arbitrary launchd label. Apple documents this bundle-scoped [Service Management API](https://developer.apple.com/documentation/servicemanagement/smappservice) and a [GUI-less installer example](https://developer.apple.com/documentation/servicemanagement/updating-your-app-package-installer-to-use-the-new-service-management-api).

A checksummed local receipt binds the canonical installation, channel and exact approved runtime identity. Registration saves that intent before OS effects. An interrupted attempt remains available through explicit Start/Uninstall. The receipt is at most 4 KiB; bounded scratch recovery accepts only prefixes of the exact previous or proposed record. Unknown metadata is preserved.

Private registration/removal commands and the OS foreground entry independently recheck that captured receipt. Directly invoking a hidden role cannot bypass it. Registration also rechecks stopped owners and explicit-stop policy, and the native bridge refuses registration while system consent is disabled or pending.

Read-only launchd observations additionally require the captured bundle plist path, executable path and exact fixed argument list. Existing labels, foreign origins, duplicate/nested identity fields, changed arguments, unavailable GUI domains, oversized output and unknown print grammar refuse further effects. A local receipt or `SMAppService.status` alone does not authorize adopting a raw old LaunchAgent. Old unknown/modified registrations remain under explicit manual review.

Start and runtime activation preserve revoked/pending user consent. A `requires_approval` observation directs the user to System Settings > General > Login Items; it is not silently re-enabled. Eligibility and actual native process ownership remain separate observations. Stop unregisters the captured app and waits for native owners to release their leases. Upgrade retains the old target until it is unregistered and drained, then saves the candidate target before registering it. Rollback uses the same captured process.

Status/Stop verify the approved main and startup resources without requiring intact inference-helper data. Changed main/startup/signature metadata fails closed and may require explicit OS-level diagnosis before repair. Native tests must qualify these damage cases; the portable tests establish no broader guarantee.

## Validation status

Windows host tests compile the Rust adapter with an unsupported-platform framework stub, validate receipt guards and reject foreign/nested/duplicate launchd identities. The stub performs no Apple operation. Python checks validate the static bundle resources, paired inventory, permissions and rejection of resealed foreign resources. Extension checks exercise both Mac architecture layouts and reject an extra executable.

Native Apple SDK/framework compilation, actual GUI-domain output, system consent, app/launcher execution, signed upgrades/rollback, damaged-payload stop, stapled-ticket survival, offline Gatekeeper, TCC and both architecture workloads remain unavailable on the current Windows host. Record them independently when a Mac runner is available; do not infer them from these portable checks. See [cutover qualification](service-cutover-qualification.md).

On an explicitly selected Mac GUI host, run `python scripts/check-macos-login.py --runtime <tested-pair> --approved-sha256 <independently-selected-inventory-digest>`. It uses the preview agent only after proving its label absent, exercises the copied launcher and native login/start/stop/removal, and preserves the fixture's original source. It retains its unique directory and qualification receipt on failure, including unverifiable cleanup; no uncertain app is silently deleted. Runtime upgrade/abort/recovery, actual consent interaction, signing and offline extraction remain additional cases.
