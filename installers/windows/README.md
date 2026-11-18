# AIRK standalone Windows installer

The Inno Setup package contains the verified AIRK service/client payload,
bundled models and legal notices. It installs per user without elevation.
The packaged service's native setup role owns the installation transaction;
Inno passes the reviewed operation, places the payload, and completes that same
operation. The package does not install VS Code or a VSIX.

After native setup succeeds, the package adds its owned `bin` directory to the
current user's `PATH`, making the canonical `airkctl.exe` available in new
terminals. The optional CLI alias is a native-owned `.cmd` in that directory.
It preserves the raw other entries and the existing `REG_SZ`/`REG_EXPAND_SZ`
registry value type. Literal existing directory entries are compared without
case, outer-quote or slash differences and are never claimed as installer-owned.
Variable-bearing entries remain raw; the comparison does not assume that a
literal `REG_SZ` expands variables. Inno Setup broadcasts the environment change;
already-running terminals keep their inherited environment. A terminal opened
from an application that has not refreshed its environment can require restarting
that application first.

## CLI alias choice

The default shortcut is `rk`. Interactive setup offers another name or a blank
choice for no alias when a command conflicts on PATH. Unattended setup chooses
the first available name: `rk`, `rk1`, `rk2`, and so on. The installed
`airkctl-aliases.txt` records the actual selection and canonical client path.

Use `/CLIALIAS=name` for a specific name, `/NOCLIALIAS=1` for no alias, or
`/AUTOCLIALIAS=1` for the default automatic choice. These options are mutually
exclusive; an explicitly selected occupied name refuses installation. Existing
commands remain untouched. `/AIRKOPERATION` recovery reuses the saved choice and
rejects alias overrides. The canonical `airkctl` works with every choice.

For example, `rk service status` and `airkctl service status` are equivalent;
substitute the selected name if setup records a different alias. These installed
choices do not change portable packages' local `.\rk.cmd` launcher.

## PATH ownership and recovery

The per-user `user-path-owner.txt` receipt records a pending update before writing
`PATH` and marks it owned only after the saved value is verified. Interrupted or
modified receipts remain an explicit failure for review. Uninstall removes only
one exact directory entry proved by a completed receipt, preserving pre-existing
manual entries, later edits, foreign installation paths and the system `PATH`.
Failed native setup never publishes a new `PATH` entry. Failed native uninstall
does not remove it or the packaged bootstrap files.

After the user confirms Add/Remove Programs removal, the `usUninstall` callback
runs the native removal operation before removing the package's tracked
payload files. Initialization only verifies the packaged binary; cancellation launches
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
