# Retained macOS runtime login entry points

Owner: `airk-installer::runtime_install::macos_login`. The thin `airk-cli`
compatibility dispatcher calls this Installer owner across the crate boundary.
The existing Mac/test module cfg remains unchanged.

The public surface consists of `native`, `foreground_root` and `direct_root`.
The CLI's MacLogin, LoginForeground and LoginServe branches respectively use
those entries; the CLI reexports the existing Installer runtime_install module.
The arguments and return types use existing public RuntimeChannel, Path and
JSON/result types. No new wire, receipt, digest or runtime format is introduced.

`native` preserves captured runtime/channel identity, explicit ownership intent
before mutation, start/drain gates before registration, exact approved signed
bundle/self checks and native SMAppService result reconciliation. The two
startup entries preserve the legacy foreground and direct login distinctions,
exact selected runtime/plist scope and the manager-process checks of their
private shared startup_root adapter. No current split registration replaces
these retained predecessor/startup duties.

The native FFI, startup_root, service_manager::verify_macos_login_intent and
service_manager_macos::verify_intent remain internal. Their reachability follows
from the public CLI entry points; exporting those helpers is unnecessary.
Visibility correction alone changes no executable body, cfg, assertion,
ownership rule or qualification claim. Static caller checks supplement, rather
than replace, actual macOS compilation and native framework/platform checks.
