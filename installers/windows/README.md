# AIRK standalone Windows installer

The Inno Setup package contains the verified native CLI/MCP runtime, bundled
models and legal notices. It installs per user without elevation and opens
`airk setup --runtime-source <payload> --approved-sha256 <distributor pin>`.
The native installer owns managed runtime activation, upgrades, source selection
and client enrollment. The package does not contain or install VS Code or a VSIX.

After native setup succeeds, the package adds its verified bootstrap runtime
directory to the current user's `PATH`, so `airk` is available in new terminals.
It preserves the raw other entries and the existing `REG_SZ`/`REG_EXPAND_SZ`
registry value type. Literal existing directory entries are compared without
case, outer-quote or slash differences and are never claimed as installer-owned.
Variable-bearing entries remain raw; the comparison does not assume that a
literal `REG_SZ` expands variables. Inno Setup broadcasts the environment change;
already-running terminals keep their inherited environment. A terminal opened
from an application that has not refreshed its environment can require restarting
that application first.

The per-user `user-path-owner.txt` receipt records a pending update before writing
`PATH` and marks it owned only after the saved value is verified. Interrupted or
modified receipts remain an explicit failure for review. Uninstall removes only
one exact directory entry proved by a completed receipt, preserving pre-existing
manual entries, later edits, foreign installation paths and the system `PATH`.
Failed native setup never publishes a new `PATH` entry. Failed native uninstall
does not remove it or the packaged bootstrap files.

After the user confirms Add/Remove Programs removal, the `usUninstall` callback
invokes `airk uninstall --yes` before removing the package's tracked bootstrap
files. Initialization only verifies the packaged binary; cancellation launches
no native process and changes no PATH entry. A native or PATH failure displays
the error and uses Inno's supported `Abort` from `usUninstall` to stop that cleanup.
Native uninstall
preserves index data by default. Packaged native executable hashes are checked
before setup and uninstall; modified executables are not launched.

`scripts/package-standalone-windows.py` uses the existing Windows runner's
**Inno Setup 6.7.1** compiler and records its version and SHA-256. Its first
compile stops at Inno's unsigned-uninstaller boundary. The existing Azure
signing action signs that uninstaller, the second compile embeds those verified
bytes, and the same Azure profile signs the final installer. Public metadata is
written only after both signatures, publisher, identity EKU and timestamp pass
the existing AIRK verifier. These steps do not acquire a new signing identity.

Inno Setup is by Jordan Russell and Martijn Laan. Its unchanged setup and
uninstaller engine retain their copyright and website notices. The exact
compiler distribution's `license.txt` is included as
`notices/inno-setup/LICENSE.txt` and bound in the installer inventory. See the
[Inno Setup license](https://jrsoftware.org/files/is/license.txt),
[signed-uninstaller contract](https://jrsoftware.org/ishelp/topic_setup_signeduninstaller.htm)
and [non-administrative mode](https://jrsoftware.org/ishelp/topic_setup_privilegesrequired.htm).
The [Abort contract](https://jrsoftware.org/ishelp/topic_isxfunc_abort.htm) and
pinned [6.7.1 uninstall event ordering](https://github.com/jrsoftware/issrc/blob/is-6_7_1/Projects/Src/Setup.Uninstall.pas)
define the failure-preserving post-confirmation boundary.

The current source has static lifecycle/PATH and mocked packaging/signing
contract tests. These do not execute the Pascal script or change a registry.
Actual Inno compilation, registry/type/Unicode round-trips, signed setup/uninstall and hosted signing/publication remain
separate qualification gates. Standalone installers initially use the stable
channel; preview native archives remain separate downloads.
