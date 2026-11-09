# AIRK standalone Windows installer

The Inno Setup package contains the verified native CLI/MCP runtime, bundled
models and legal notices. It installs per user without elevation and opens
`airk setup --runtime-source <payload> --approved-sha256 <distributor pin>`.
The native installer owns managed runtime activation, upgrades, source selection
and client enrollment. The package does not contain or install VS Code or a VSIX.

Add/Remove Programs invokes `airk uninstall --yes` before removing the package's
tracked bootstrap files. A native failure aborts that cleanup. Native uninstall
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

The current source has mocked packaging/signing contract tests. Actual Inno
compilation, signed setup/uninstall and hosted signing/publication remain
separate qualification gates. Standalone installers initially use the stable
channel; preview native archives remain separate downloads.
