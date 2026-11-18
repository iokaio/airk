# Installer-owned CLI aliases

The shared owner in ../shortcuts.rs validates alias names and renders wrappers.
intent.rs binds selection to the exact original private setup operation and
checks command lookup collisions. note.rs owns the readable installed alias note.
windows.rs owns held-file Windows publication/removal; the Unix adapter remains
in the shared source. tests.rs and windows/tests.rs hold focused controls.

This is installer infrastructure, not another native executable. Wrappers exec
or invoke the canonical airkctl. See ../CONTRACT.md for collision, recovery,
upgrade and uninstall ownership. Portable wrappers have a separate packaging
owner and do not activate PATH or shell profiles.
